use emmylua_parser::{LuaAstNode, LuaDocTypeList};
use emmylua_parser::{LuaCallExpr, LuaExpr};
use hashbrown::HashSet;
use internment::ArcIntern;
use std::{ops::Deref, sync::Arc};

use crate::semantic::infer::infer_expr_list_types;
use crate::{
    DocTypeInferContext, FileId, GenericTpl, GenericTplId, LuaFunctionType, LuaGenericType,
    TypeVisitTrait,
    db_index::{DbIndex, LuaType},
    infer_doc_type,
    semantic::{
        LuaInferCache,
        generic::{
            instantiate_type::instantiate_doc_function,
            tpl_context::TplContext,
            tpl_pattern::{
                multi_param_tpl_pattern_match_multi_return, return_type_pattern_match_target_type,
                tpl_pattern_match, variadic_tpl_pattern_match,
            },
        },
        infer::InferFailReason,
        infer_expr,
        overload_resolve::{callable_accepts_args, resolve_signature_by_args},
    },
};
use crate::{
    LuaMemberOwner, LuaSemanticDeclId, SemanticDeclLevel, infer_node_semantic_decl,
    tpl_pattern_match_args,
};

use super::{TypeSubstitutor, collect_callable_overload_groups};

pub fn instantiate_func_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    func: &LuaFunctionType,
    call_expr: LuaCallExpr,
) -> Result<LuaFunctionType, InferFailReason> {
    let file_id = cache.get_file_id().clone();
    let mut generic_tpls = HashSet::new();
    let mut contain_self = false;
    func.visit_type(&mut |t| match t {
        LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
            let tpl_id = generic_tpl.get_tpl_id();
            if tpl_id.is_func() {
                generic_tpls.insert(tpl_id);
            }
        }
        LuaType::StrTplRef(str_tpl) => {
            generic_tpls.insert(str_tpl.get_tpl_id());
        }
        LuaType::SelfInfer => {
            contain_self = true;
        }
        _ => {}
    });

    let origin_params = func.get_params();
    let mut func_params: Vec<_> = origin_params
        .iter()
        .map(|(name, t)| (name.clone(), t.clone().unwrap_or(LuaType::Unknown)))
        .collect();

    let arg_exprs = call_expr
        .get_args_list()
        .ok_or(InferFailReason::None)?
        .get_args()
        .collect::<Vec<_>>();
    let mut substitutor = TypeSubstitutor::new();
    let mut context = TplContext {
        db,
        cache,
        substitutor: &mut substitutor,
        call_expr: Some(call_expr.clone()),
    };
    if !generic_tpls.is_empty() {
        context.substitutor.add_need_infer_tpls(generic_tpls);

        if let Some(type_list) = call_expr.get_call_generic_type_list() {
            // 如果使用了`obj:abc--[[@<string>]]("abc")`强制指定了泛型, 那么我们只需要直接应用
            apply_call_generic_type_list(db, file_id, &mut context, &type_list);
        } else {
            // 如果没有指定泛型, 则需要从调用参数中推断
            infer_generic_types_from_call(
                db,
                &mut context,
                func,
                &call_expr,
                &mut func_params,
                &arg_exprs,
            )?;
        }
    }

    if contain_self && let Some(self_type) = infer_self_type(db, cache, &call_expr) {
        substitutor.add_self_type(self_type);
    }

    if let LuaType::DocFunction(f) = instantiate_doc_function(db, func, &substitutor) {
        Ok(f.deref().clone())
    } else {
        Ok(func.clone())
    }
}

fn apply_call_generic_type_list(
    db: &DbIndex,
    file_id: FileId,
    context: &mut TplContext,
    type_list: &LuaDocTypeList,
) {
    let doc_ctx = DocTypeInferContext::new(db, file_id);
    for (i, doc_type) in type_list.get_types().enumerate() {
        let typ = infer_doc_type(doc_ctx, &doc_type);
        context
            .substitutor
            .insert_type(GenericTplId::Func(i as u32), typ, true);
    }
}

pub fn as_doc_function_type(
    db: &DbIndex,
    callable_type: &LuaType,
) -> Result<Option<Arc<LuaFunctionType>>, InferFailReason> {
    Ok(match callable_type {
        LuaType::DocFunction(doc_func) => Some(doc_func.clone()),
        LuaType::Signature(sig_id) => Some(
            db.get_signature_index()
                .get(sig_id)
                .ok_or(InferFailReason::None)?
                .to_doc_func_type(),
        ),
        _ => None,
    })
}

fn infer_return_from_callable(
    db: &DbIndex,
    callable: &Arc<LuaFunctionType>,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let instantiated = instantiate_doc_function(db, callable, substitutor);
    match instantiated {
        LuaType::DocFunction(func) => func.get_ret().clone(),
        _ => callable.get_ret().clone(),
    }
}

fn fallback_return_from_overloads(db: &DbIndex, overloads: &[Arc<LuaFunctionType>]) -> LuaType {
    LuaType::from_vec(
        overloads
            .iter()
            .map(|callable| {
                let mut callable_tpls = HashSet::new();
                callable.visit_type(&mut |ty| {
                    if let LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) = ty {
                        callable_tpls.insert(generic_tpl.get_tpl_id());
                    }
                });

                let mut callable_substitutor = TypeSubstitutor::new();
                callable_substitutor.add_need_infer_tpls(callable_tpls);
                infer_return_from_callable(db, callable, &callable_substitutor)
            })
            .collect(),
    )
}

fn infer_callable_return_from_arg_types(
    context: &mut TplContext,
    callable_type: &LuaType,
    call_arg_types: Option<&[LuaType]>,
    fallback_on_no_match: bool,
) -> Result<Option<LuaType>, InferFailReason> {
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(context.db, callable_type, &mut overload_groups)?;
    if overload_groups.is_empty() {
        return Ok(None);
    }

    let Some(call_arg_types) = call_arg_types else {
        return Ok(Some(fallback_return_from_overloads(
            context.db,
            &overload_groups
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
        )));
    };

    let mut member_returns = Vec::new();
    for overloads in &overload_groups {
        let instantiated_overloads = overloads
            .iter()
            .filter_map(|callable| {
                instantiate_callable_from_arg_types(context, callable, call_arg_types)
            })
            .collect::<Vec<_>>();
        if instantiated_overloads.is_empty() {
            continue;
        }

        // Prefer candidates that produce an informative instantiated return over ones that only
        // narrow to `unknown`. This keeps explicit `fun(...): unknown` callbacks aligned with
        // unresolved closure returns while still preserving parameter-shape matching.
        let preferred_overloads = instantiated_overloads
            .iter()
            .filter(|callable| !callable.get_ret().is_unknown())
            .cloned()
            .collect::<Vec<_>>();
        let overloads_to_resolve = if preferred_overloads.is_empty() {
            &instantiated_overloads
        } else {
            &preferred_overloads
        };

        member_returns.push(
            resolve_signature_by_args(
                context.db,
                overloads_to_resolve,
                call_arg_types,
                false,
                None,
            )
            .map(|callable| callable.get_ret().clone())
            .unwrap_or_else(|_| fallback_return_from_overloads(context.db, overloads)),
        );
    }
    if member_returns.is_empty() {
        return Ok(if fallback_on_no_match {
            Some(fallback_return_from_overloads(
                context.db,
                &overload_groups
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
            ))
        } else {
            None
        });
    }

    Ok(Some(LuaType::from_vec(member_returns)))
}

pub fn infer_callable_return_from_remaining_args(
    context: &mut TplContext,
    callable_type: &LuaType,
    arg_exprs: &[LuaExpr],
) -> Result<Option<LuaType>, InferFailReason> {
    if arg_exprs.is_empty() {
        return Ok(None);
    }

    let call_arg_types =
        match infer_expr_list_types(context.db, context.cache, arg_exprs, None, infer_expr) {
            Ok(types) => types.into_iter().map(|(ty, _)| ty).collect::<Vec<_>>(),
            Err(_) => {
                return infer_callable_return_from_arg_types(context, callable_type, None, true);
            }
        };
    if call_arg_types.is_empty() {
        return Ok(None);
    }

    infer_callable_return_from_arg_types(context, callable_type, Some(&call_arg_types), true)
}

fn instantiate_callable_from_arg_types(
    context: &mut TplContext,
    callable: &Arc<LuaFunctionType>,
    call_arg_types: &[LuaType],
) -> Option<Arc<LuaFunctionType>> {
    if !callable_accepts_args(context.db, callable, call_arg_types, false, None) {
        return None;
    }

    let mut callable_tpls = HashSet::new();
    callable.visit_type(&mut |ty| {
        if let LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) = ty {
            callable_tpls.insert(generic_tpl.get_tpl_id());
        }
    });
    if callable_tpls.is_empty() {
        return Some(callable.clone());
    }

    let callable_param_types = callable
        .get_params()
        .iter()
        .map(|(_, ty)| ty.clone().unwrap_or(LuaType::Unknown))
        .collect::<Vec<_>>();
    let mut callable_substitutor = TypeSubstitutor::new();
    callable_substitutor.add_need_infer_tpls(callable_tpls.clone());
    let mut callable_context = TplContext {
        db: context.db,
        cache: context.cache,
        substitutor: &mut callable_substitutor,
        call_expr: context.call_expr.clone(),
    };
    if tpl_pattern_match_args(&mut callable_context, &callable_param_types, call_arg_types).is_err()
    {
        return None;
    }

    let instantiated = match instantiate_doc_function(context.db, callable, &callable_substitutor) {
        LuaType::DocFunction(func) => func,
        _ => callable.clone(),
    };
    let mut has_unresolved_return_tpl = false;
    instantiated.get_ret().visit_type(&mut |ty| {
        if let LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) = ty
            && callable_tpls.contains(&generic_tpl.get_tpl_id())
        {
            has_unresolved_return_tpl = true;
        }
    });
    if has_unresolved_return_tpl {
        return None;
    }

    Some(instantiated)
}

fn infer_generic_types_from_call(
    db: &DbIndex,
    context: &mut TplContext,
    func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
    func_params: &mut Vec<(String, LuaType)>,
    arg_exprs: &[LuaExpr],
) -> Result<(), InferFailReason> {
    let colon_call = call_expr.is_colon_call();
    let colon_define = func.is_colon_define();
    match (colon_define, colon_call) {
        (true, false) => {
            func_params.insert(0, ("self".to_string(), LuaType::Any));
        }
        (false, true) => {
            if !func_params.is_empty() {
                func_params.remove(0);
            }
        }
        _ => {}
    }

    let mut unresolve_tpls = vec![];
    for i in 0..func_params.len() {
        if i >= arg_exprs.len() {
            break;
        }

        if context.substitutor.is_infer_all_tpl() {
            break;
        }

        let (_, func_param_type) = &func_params[i];
        let call_arg_expr = &arg_exprs[i];
        if !func_param_type.contain_tpl() {
            continue;
        }

        if !func_param_type.is_variadic()
            && check_expr_can_later_infer(context, func_param_type, call_arg_expr)?
        {
            // 如果参数不能被后续推断, 那么我们先不处理
            unresolve_tpls.push((func_param_type.clone(), call_arg_expr.clone()));
            continue;
        }

        let arg_type = match infer_expr(db, context.cache, call_arg_expr.clone()) {
            Ok(t) => t,
            Err(InferFailReason::FieldNotFound) => LuaType::Nil, // 对于未找到的字段, 我们认为是 nil 以执行后续推断
            Err(e) => return Err(e),
        };

        if let Some(return_pattern) =
            as_doc_function_type(context.db, func_param_type)?.map(|func| func.get_ret().clone())
        {
            if let Some(inferred_return_type) =
                infer_callable_return_from_remaining_args(context, &arg_type, &arg_exprs[i + 1..])?
            {
                return_type_pattern_match_target_type(
                    context,
                    &return_pattern,
                    &inferred_return_type,
                )?;
            } else if arg_type.is_any() || arg_type.is_unknown() {
                return_type_pattern_match_target_type(context, &return_pattern, &LuaType::Unknown)?;
            }
        }

        match (func_param_type, &arg_type) {
            (LuaType::Variadic(variadic), _) => {
                let mut arg_types = vec![];
                for arg_expr in &arg_exprs[i..] {
                    let arg_type = infer_expr(db, context.cache, arg_expr.clone())?;
                    arg_types.push(arg_type);
                }
                variadic_tpl_pattern_match(context, variadic, &arg_types)?;
                break;
            }
            (_, LuaType::Variadic(variadic)) => {
                let func_param_types = func_params[i..]
                    .iter()
                    .map(|(_, t)| t)
                    .cloned()
                    .collect::<Vec<_>>();
                multi_param_tpl_pattern_match_multi_return(context, &func_param_types, variadic)?;
                break;
            }
            _ => {
                tpl_pattern_match(context, func_param_type, &arg_type)?;
            }
        }
    }

    if !context.substitutor.is_infer_all_tpl() {
        for (func_param_type, call_arg_expr) in unresolve_tpls {
            let closure_type = infer_expr(db, context.cache, call_arg_expr)?;

            tpl_pattern_match(context, &func_param_type, &closure_type)?;
        }
    }

    Ok(())
}

pub fn build_self_type(db: &DbIndex, self_type: &LuaType) -> LuaType {
    match self_type {
        LuaType::Def(id) | LuaType::Ref(id) => {
            if let Some(generic) = db.get_type_index().get_generic_params(id) {
                let mut params = Vec::new();
                for (i, generic_param) in generic.iter().enumerate() {
                    if let Some(t) = &generic_param.type_constraint {
                        params.push(t.clone());
                    } else {
                        params.push(LuaType::TplRef(Arc::new(GenericTpl::new(
                            GenericTplId::Type(i as u32),
                            ArcIntern::new(generic_param.name.clone()),
                            None,
                        ))));
                    }
                }
                let generic = LuaGenericType::new(id.clone(), params);
                return LuaType::Generic(Arc::new(generic));
            }
        }
        _ => {}
    };
    self_type.clone()
}

pub fn infer_self_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: &LuaCallExpr,
) -> Option<LuaType> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    match prefix_expr {
        LuaExpr::IndexExpr(index) => {
            let self_expr = index.get_prefix_expr()?;
            let self_type = infer_expr(db, cache, self_expr).ok()?;
            let self_type = build_self_type(db, &self_type);
            return Some(self_type);
        }
        LuaExpr::NameExpr(name) => {
            let semantic_decl_id = infer_node_semantic_decl(
                db,
                cache,
                name.syntax().clone(),
                SemanticDeclLevel::default(),
            )?;
            if let LuaSemanticDeclId::Member(member_id) = semantic_decl_id {
                let owner = db.get_member_index().get_current_owner(&member_id)?;
                if let LuaMemberOwner::Type(id) = owner {
                    let typ = LuaType::Ref(id.clone());
                    let self_type = build_self_type(db, &typ);
                    return Some(self_type);
                }
                return None;
            }
        }
        _ => {}
    }

    None
}

fn check_expr_can_later_infer(
    context: &mut TplContext,
    func_param_type: &LuaType,
    call_arg_expr: &LuaExpr,
) -> Result<bool, InferFailReason> {
    let Some(doc_function) = as_doc_function_type(context.db, func_param_type)? else {
        return Ok(false);
    };

    if let LuaExpr::ClosureExpr(_) = call_arg_expr {
        return Ok(true);
    }

    let doc_params = doc_function.get_params();
    let variadic_count = doc_params
        .iter()
        .filter_map(|(_, t)| {
            if let Some(LuaType::Variadic(_)) = t {
                Some(())
            } else {
                None
            }
        })
        .count();

    Ok(variadic_count > 1)
}
