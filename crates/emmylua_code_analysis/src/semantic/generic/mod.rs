mod call_constraint;
mod inference;
mod instantiate_type;
mod test;
mod type_substitutor;

use std::sync::Arc;

pub use call_constraint::{
    CallConstraintArg, CallConstraintContext, build_call_constraint_context,
    normalize_constraint_type,
};
use emmylua_parser::LuaAstNode;
use emmylua_parser::LuaExpr;
use hashbrown::HashSet;
pub(in crate::semantic::generic) use inference::{
    InferenceContext, InferencePriority, InferenceVariance, infer_type_list, infer_types_from_expr,
    multi_param_infer_multi_return, return_type_infer_types, variadic_infer_types,
};
pub use instantiate_type::*;
use rowan::NodeOrToken;
pub use type_substitutor::TypeSubstitutor;

use crate::DbIndex;
use crate::GenericTpl;
use crate::GenericTplId;
use crate::InferFailReason;
use crate::LuaDeclExtra;
use crate::LuaFunctionType;
use crate::LuaInferCache;
use crate::LuaMemberOwner;
use crate::LuaSemanticDeclId;
use crate::LuaType;
use crate::LuaTypeNode;
use crate::SemanticDeclLevel;
use crate::TypeOps;
use crate::infer_node_semantic_decl;
use crate::semantic::semantic_info::infer_token_semantic_decl;
pub use instantiate_type::get_keyof_members;

pub fn instantiate_doc_function_by_arg_types(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    doc_function: &Arc<LuaFunctionType>,
    call_arg_types: &[LuaType],
) -> Result<Arc<LuaFunctionType>, InferFailReason> {
    let generic_tpl_ids = collect_doc_function_tpl_ids(doc_function);
    if generic_tpl_ids.is_empty() {
        return Ok(doc_function.clone());
    }

    let param_types = doc_function
        .get_params()
        .iter()
        .map(|(_, typ)| typ.clone().unwrap_or(LuaType::Unknown))
        .collect::<Vec<_>>();
    let mut context = InferenceContext::new(db, cache, None);
    context.prepare_inference_slots(generic_tpl_ids);
    infer_type_list(
        &mut context,
        &param_types,
        call_arg_types,
        &LuaType::Unknown,
        InferenceVariance::Covariant,
        InferencePriority::Normal,
    )?;

    let mut substitutor = TypeSubstitutor::new();
    let generic_tpls = collect_doc_function_generic_tpls(doc_function);
    context.bridge_to_substitutor(
        &mut substitutor,
        generic_tpls.iter(),
        doc_function.get_ret(),
    );

    let doc_function_ty = LuaType::DocFunction(doc_function.clone());
    Ok(
        match instantiate_type_generic(db, &doc_function_ty, &substitutor) {
            LuaType::DocFunction(func) => func,
            _ => doc_function.clone(),
        },
    )
}

fn collect_doc_function_tpl_ids(doc_function: &LuaFunctionType) -> HashSet<GenericTplId> {
    let mut generic_tpl_ids = HashSet::new();
    doc_function.visit_nested_types(&mut |ty| match ty {
        LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
            collect_function_tpl_with_fallback_deps(&generic_tpl, &mut generic_tpl_ids);
        }
        LuaType::StrTplRef(str_tpl) => {
            let tpl_id = str_tpl.get_tpl_id();
            if !tpl_id.is_func() {
                return;
            }

            generic_tpl_ids.insert(tpl_id);
            let Some(constraint) = str_tpl.get_constraint() else {
                return;
            };

            let mut constraint_deps = HashSet::new();
            if collect_function_tpl_deps_from_fallback_type(
                constraint,
                &mut constraint_deps,
                &mut HashSet::new(),
            ) {
                generic_tpl_ids.extend(constraint_deps);
            }
        }
        _ => {}
    });

    generic_tpl_ids
}

fn collect_doc_function_generic_tpls(doc_function: &LuaFunctionType) -> Vec<Arc<GenericTpl>> {
    let mut generic_tpls = Vec::new();
    doc_function.visit_nested_types(&mut |ty| match ty {
        LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
            if generic_tpl.get_tpl_id().is_func()
                && !generic_tpls.iter().any(|existing: &Arc<GenericTpl>| {
                    existing.get_tpl_id() == generic_tpl.get_tpl_id()
                })
            {
                generic_tpls.push(generic_tpl.clone());
            }
        }
        _ => {}
    });

    generic_tpls
}

fn collect_function_tpl_with_fallback_deps(
    generic_tpl: &GenericTpl,
    generic_tpl_ids: &mut HashSet<GenericTplId>,
) {
    let tpl_id = generic_tpl.get_tpl_id();
    if !tpl_id.is_func() {
        return;
    }

    generic_tpl_ids.insert(tpl_id);
    let Some(fallback_type) = generic_tpl
        .get_default_type()
        .or(generic_tpl.get_constraint())
    else {
        return;
    };

    let mut fallback_deps = HashSet::new();
    let mut visiting_fallbacks = HashSet::new();
    visiting_fallbacks.insert(tpl_id);
    if collect_function_tpl_deps_from_fallback_type(
        fallback_type,
        &mut fallback_deps,
        &mut visiting_fallbacks,
    ) {
        generic_tpl_ids.extend(fallback_deps);
    }
}

fn collect_function_tpl_deps_from_fallback_type(
    ty: &LuaType,
    generic_tpl_ids: &mut HashSet<GenericTplId>,
    visiting_fallbacks: &mut HashSet<GenericTplId>,
) -> bool {
    let mut no_fallback_cycle =
        collect_function_tpl_dep_from_fallback_type(ty, generic_tpl_ids, visiting_fallbacks);
    ty.visit_nested_types(&mut |ty| {
        no_fallback_cycle &=
            collect_function_tpl_dep_from_fallback_type(ty, generic_tpl_ids, visiting_fallbacks);
    });
    no_fallback_cycle
}

fn collect_function_tpl_dep_from_fallback_type(
    ty: &LuaType,
    generic_tpl_ids: &mut HashSet<GenericTplId>,
    visiting_fallbacks: &mut HashSet<GenericTplId>,
) -> bool {
    match ty {
        LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
            let tpl_id = generic_tpl.get_tpl_id();
            if !tpl_id.is_func() {
                return true;
            }

            if !visiting_fallbacks.insert(tpl_id) {
                return false;
            }

            generic_tpl_ids.insert(tpl_id);
            let no_fallback_cycle = match generic_tpl
                .get_default_type()
                .or(generic_tpl.get_constraint())
            {
                Some(fallback_type) => collect_function_tpl_deps_from_fallback_type(
                    fallback_type,
                    generic_tpl_ids,
                    visiting_fallbacks,
                ),
                None => true,
            };
            visiting_fallbacks.remove(&tpl_id);
            no_fallback_cycle
        }
        LuaType::StrTplRef(str_tpl) => {
            let tpl_id = str_tpl.get_tpl_id();
            if !tpl_id.is_func() {
                return true;
            }

            if !visiting_fallbacks.insert(tpl_id) {
                return false;
            }

            generic_tpl_ids.insert(tpl_id);
            let no_fallback_cycle = match str_tpl.get_constraint() {
                Some(constraint) => collect_function_tpl_deps_from_fallback_type(
                    constraint,
                    generic_tpl_ids,
                    visiting_fallbacks,
                ),
                None => true,
            };
            visiting_fallbacks.remove(&tpl_id);
            no_fallback_cycle
        }
        _ => true,
    }
}

pub fn get_tpl_ref_extend_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    arg_type: &LuaType,
    arg_expr: LuaExpr,
    depth: usize,
) -> Option<LuaType> {
    match arg_type {
        LuaType::TplRef(tpl_ref) | LuaType::ConstTplRef(tpl_ref) => {
            if let Some(extend) = tpl_ref.get_constraint().cloned() {
                return Some(extend);
            }
            let node_or_token = arg_expr.syntax().clone().into();
            let semantic_decl = match node_or_token {
                NodeOrToken::Node(node) => {
                    infer_node_semantic_decl(db, cache, node, SemanticDeclLevel::default())
                }
                NodeOrToken::Token(token) => {
                    infer_token_semantic_decl(db, cache, token, SemanticDeclLevel::default())
                }
            }?;

            match tpl_ref.get_tpl_id() {
                GenericTplId::Func(tpl_id) => {
                    if let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl {
                        let decl = db.get_decl_index().get_decl(&decl_id)?;
                        match decl.extra {
                            LuaDeclExtra::Param { signature_id, .. } => {
                                let signature = db.get_signature_index().get(&signature_id)?;
                                if let Some(generic_param) =
                                    signature.generic_params.get(tpl_id as usize)
                                {
                                    return generic_param.constraint.clone();
                                }
                            }
                            _ => return None,
                        }
                    }
                    None
                }
                GenericTplId::Type(tpl_id) => {
                    if let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl {
                        let decl = db.get_decl_index().get_decl(&decl_id)?;
                        match decl.extra {
                            LuaDeclExtra::Param {
                                owner_member_id, ..
                            } => {
                                let owner_member_id = owner_member_id?;
                                let parent_owner =
                                    db.get_member_index().get_current_owner(&owner_member_id)?;
                                match parent_owner {
                                    LuaMemberOwner::Type(type_id) => {
                                        let generic_params =
                                            db.get_type_index().get_generic_params(type_id)?;
                                        return generic_params
                                            .get(tpl_id as usize)?
                                            .type_constraint
                                            .clone();
                                    }
                                    _ => return None,
                                }
                            }
                            _ => return None,
                        }
                    }
                    None
                }
                GenericTplId::ConditionalInfer(_) => None,
            }
        }
        LuaType::StrTplRef(str_tpl) => str_tpl.get_constraint().cloned(),
        LuaType::Union(union_type) => {
            if depth > 1 {
                return None;
            }
            let mut result = LuaType::Never;
            for union_member_type in union_type.into_vec().iter() {
                let extend_type = get_tpl_ref_extend_type(
                    db,
                    cache,
                    union_member_type,
                    arg_expr.clone(),
                    depth + 1,
                )
                .unwrap_or(union_member_type.clone());
                result = TypeOps::Union.apply(db, &result, &extend_type);
            }
            Some(result)
        }
        _ => None,
    }
}
