use std::ops::Deref;

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaDocTagType, LuaExpr};
use rowan::TextRange;

use crate::diagnostic::checker::generic::infer_doc_type::infer_doc_type;
use crate::diagnostic::checker::param_type_check::get_call_source_type;
use crate::{
    humanize_type, DiagnosticCode, GenericTplId, LuaDeclExtra, LuaMemberOwner, LuaSemanticDeclId,
    LuaSignature, LuaStringTplType, LuaType, RenderLevel, SemanticDeclLevel, SemanticModel,
    TypeCheckFailReason, TypeCheckResult, TypeOps, VariadicType,
};

use crate::diagnostic::checker::Checker;
use crate::diagnostic::lua_diagnostic::DiagnosticContext;

pub struct GenericConstraintMismatchChecker;

impl Checker for GenericConstraintMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::GenericConstraintMismatch];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaCallExpr(call_expr) => {
                    check_call_expr(context, semantic_model, call_expr);
                }
                LuaAst::LuaDocTagType(doc_tag_type) => {
                    check_doc_tag_type(context, semantic_model, doc_tag_type);
                }
                _ => {}
            }
        }
    }
}

fn check_doc_tag_type(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    doc_tag_type: LuaDocTagType,
) -> Option<()> {
    let type_list = doc_tag_type.get_type_list();
    for doc_type in type_list {
        let type_ref = infer_doc_type(semantic_model, &doc_type);
        let generic_type = match type_ref {
            LuaType::Generic(generic_type) => generic_type,
            _ => continue,
        };

        let generic_params = semantic_model
            .get_db()
            .get_type_index()
            .get_generic_params(&generic_type.get_base_type_id())?;
        for (i, param_type) in generic_type.get_params().iter().enumerate() {
            let extend_type = generic_params.get(i)?.1.clone()?;
            let result = semantic_model.type_check(&extend_type, &param_type);
            if !result.is_ok() {
                add_type_check_diagnostic(
                    context,
                    semantic_model,
                    doc_type.get_range(),
                    &extend_type,
                    &param_type,
                    result,
                );
            }
        }
    }
    Some(())
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let function = semantic_model
        .infer_expr(call_expr.get_prefix_expr()?.clone())
        .ok()?;

    if let LuaType::Signature(signature_id) = function {
        let signature = semantic_model
            .get_db()
            .get_signature_index()
            .get(&signature_id)?;
        let mut params = signature.get_type_params();
        let mut arg_infos = get_arg_infos(semantic_model, &call_expr)?;

        match (call_expr.is_colon_call(), signature.is_colon_define) {
            (true, true) | (false, false) => {}
            (false, true) => {
                params.insert(0, ("self".into(), Some(LuaType::SelfInfer)));
            }
            (true, false) => {
                // 往调用参数插入插入调用者类型
                arg_infos.insert(
                    0,
                    (
                        get_call_source_type(semantic_model, &call_expr)?,
                        call_expr.get_colon_token()?.get_range(),
                    ),
                );
            }
        }
        for (i, (_, param_type)) in params.iter().enumerate() {
            let param_type = if let Some(param_type) = param_type {
                param_type
            } else {
                continue;
            };

            check_param_type(
                context,
                semantic_model,
                &call_expr,
                i,
                param_type,
                signature,
                &arg_infos,
                false,
            );
        }
    }

    Some(())
}

fn check_param_type(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    param_index: usize,
    param_type: &LuaType,
    signature: &LuaSignature,
    arg_infos: &[(LuaType, TextRange)],
    from_union: bool,
) -> Option<()> {
    // 应该先通过泛型体操约束到唯一类型再进行检查
    match param_type {
        LuaType::StrTplRef(str_tpl_ref) => {
            let extend_type = get_extend_type(
                semantic_model,
                &call_expr,
                str_tpl_ref.get_tpl_id(),
                signature,
            );
            let arg_expr = call_expr.get_args_list()?.get_args().nth(param_index)?;
            let arg_type = semantic_model.infer_expr(arg_expr.clone()).ok()?;

            if from_union && !arg_type.is_string() {
                return None;
            }

            check_str_tpl_ref(
                context,
                semantic_model,
                str_tpl_ref,
                &arg_type,
                arg_expr.get_range(),
                extend_type,
            );
        }
        LuaType::TplRef(tpl_ref) => {
            let extend_type =
                get_extend_type(semantic_model, &call_expr, tpl_ref.get_tpl_id(), signature);
            check_tpl_ref(
                context,
                semantic_model,
                &extend_type,
                arg_infos.get(param_index),
            );
        }
        LuaType::Union(union_type) => {
            // 如果不是来自 union, 才展开 union 中的每个类型进行检查
            if !from_union {
                for union_member_type in union_type.into_vec().iter() {
                    check_param_type(
                        context,
                        semantic_model,
                        call_expr,
                        param_index,
                        union_member_type,
                        signature,
                        arg_infos,
                        true,
                    );
                }
            }
        }
        _ => {}
    }
    Some(())
}

fn get_extend_type(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    tpl_id: GenericTplId,
    signature: &LuaSignature,
) -> Option<LuaType> {
    match tpl_id {
        GenericTplId::Func(tpl_id) => signature.generic_params.get(tpl_id as usize)?.1.clone(),
        GenericTplId::Type(tpl_id) => {
            let prefix_expr = call_expr.get_prefix_expr()?;
            let semantic_decl = semantic_model.find_decl(
                prefix_expr.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )?;
            let member_index = semantic_model.get_db().get_member_index();
            match semantic_decl {
                LuaSemanticDeclId::Member(member_id) => {
                    let owner = member_index.get_current_owner(&member_id)?;
                    match owner {
                        LuaMemberOwner::Type(type_id) => {
                            let generic_params = semantic_model
                                .get_db()
                                .get_type_index()
                                .get_generic_params(&type_id)?;
                            generic_params.get(tpl_id as usize)?.1.clone()
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
    }
}

fn check_str_tpl_ref(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    str_tpl_ref: &LuaStringTplType,
    arg_type: &LuaType,
    range: TextRange,
    extend_type: Option<LuaType>,
) -> Option<()> {
    match arg_type {
        LuaType::StringConst(str) | LuaType::DocStringConst(str) => {
            let full_type_name = format!(
                "{}{}{}",
                str_tpl_ref.get_prefix(),
                str,
                str_tpl_ref.get_suffix()
            );
            let founded_type_decl = semantic_model
                .get_db()
                .get_type_index()
                .find_type_decl(semantic_model.get_file_id(), &full_type_name);
            if founded_type_decl.is_none() {
                context.add_diagnostic(
                    DiagnosticCode::GenericConstraintMismatch,
                    range,
                    t!("the string template type does not match any type declaration").to_string(),
                    None,
                );
            }

            if let Some(extend_type) = extend_type {
                if let Some(type_decl) = founded_type_decl {
                    let type_id = type_decl.get_id();
                    let ref_type = LuaType::Ref(type_id);
                    let result = semantic_model.type_check(&extend_type, &ref_type);
                    if result.is_err() {
                        add_type_check_diagnostic(
                            context,
                            semantic_model,
                            range,
                            &extend_type,
                            &ref_type,
                            result,
                        );
                    }
                }
            }
        }
        LuaType::String | LuaType::Any | LuaType::Unknown | LuaType::StrTplRef(_) => {}
        _ => {
            context.add_diagnostic(
                DiagnosticCode::GenericConstraintMismatch,
                range,
                t!("the string template type must be a string constant").to_string(),
                None,
            );
        }
    }
    Some(())
}

fn check_tpl_ref(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    extend_type: &Option<LuaType>,
    arg_info: Option<&(LuaType, TextRange)>,
) -> Option<()> {
    let extend_type = extend_type.clone()?;
    let arg_info = arg_info?;
    let result = semantic_model.type_check(&extend_type, &arg_info.0);
    if !result.is_ok() {
        add_type_check_diagnostic(
            context,
            semantic_model,
            arg_info.1,
            &extend_type,
            &arg_info.0,
            result,
        );
    }
    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    extend_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    let db = semantic_model.get_db();
    match result {
        Ok(_) => return,
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason,
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DonotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => "type recursion".to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::GenericConstraintMismatch,
                range,
                t!(
                    "type `%{found}` does not satisfy the constraint `%{source}`. %{reason}",
                    source = humanize_type(db, &extend_type, RenderLevel::Simple),
                    found = humanize_type(db, &expr_type, RenderLevel::Simple),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}

fn get_arg_infos(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Vec<(LuaType, TextRange)>> {
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let mut arg_infos = infer_expr_list_types(semantic_model, &arg_exprs);
    for (arg_type, arg_expr) in arg_infos.iter_mut() {
        let extend_type = try_instantiate_arg_type(semantic_model, arg_type, arg_expr, 0);
        if let Some(extend_type) = extend_type {
            *arg_type = extend_type;
        }
    }

    let arg_infos = arg_infos
        .into_iter()
        .map(|(arg_type, arg_expr)| (arg_type, arg_expr.get_range()))
        .collect();

    Some(arg_infos)
}

fn try_instantiate_arg_type(
    semantic_model: &SemanticModel,
    arg_type: &LuaType,
    arg_expr: &LuaExpr,
    depth: usize,
) -> Option<LuaType> {
    match arg_type {
        LuaType::TplRef(tpl_ref) => {
            let node_or_token = arg_expr.syntax().clone().into();
            let semantic_decl =
                semantic_model.find_decl(node_or_token, SemanticDeclLevel::default())?;
            match tpl_ref.get_tpl_id() {
                GenericTplId::Func(tpl_id) => {
                    if let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl {
                        let decl = semantic_model
                            .get_db()
                            .get_decl_index()
                            .get_decl(&decl_id)?;
                        match decl.extra {
                            LuaDeclExtra::Param { signature_id, .. } => {
                                let signature = semantic_model
                                    .get_db()
                                    .get_signature_index()
                                    .get(&signature_id)?;
                                if let Some((_, param_type)) =
                                    signature.generic_params.get(tpl_id as usize)
                                {
                                    return param_type.clone();
                                }
                            }
                            _ => return None,
                        }
                    }
                    None
                }
                GenericTplId::Type(tpl_id) => {
                    if let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl {
                        let decl = semantic_model
                            .get_db()
                            .get_decl_index()
                            .get_decl(&decl_id)?;
                        match decl.extra {
                            LuaDeclExtra::Param {
                                owner_member_id, ..
                            } => {
                                let owner_member_id = owner_member_id?;
                                let parent_owner = semantic_model
                                    .get_db()
                                    .get_member_index()
                                    .get_current_owner(&owner_member_id)?;
                                match parent_owner {
                                    LuaMemberOwner::Type(type_id) => {
                                        let generic_params = semantic_model
                                            .get_db()
                                            .get_type_index()
                                            .get_generic_params(&type_id)?;
                                        return generic_params.get(tpl_id as usize)?.1.clone();
                                    }
                                    _ => return None,
                                }
                            }
                            _ => return None,
                        }
                    }
                    None
                }
            }
        }
        LuaType::Union(union_type) => {
            if depth > 1 {
                return None;
            }
            let mut result = LuaType::Unknown;
            for union_member_type in union_type.into_vec().iter() {
                let extend_type = try_instantiate_arg_type(
                    semantic_model,
                    union_member_type,
                    arg_expr,
                    depth + 1,
                )
                .unwrap_or(union_member_type.clone());
                result = TypeOps::Union.apply(semantic_model.get_db(), &result, &extend_type);
            }
            Some(result)
        }
        _ => None,
    }
}

fn infer_expr_list_types(
    semantic_model: &SemanticModel,
    exprs: &[LuaExpr],
) -> Vec<(LuaType, LuaExpr)> {
    let mut value_types = Vec::new();
    for expr in exprs.iter() {
        let expr_type = semantic_model
            .infer_expr(expr.clone())
            .unwrap_or(LuaType::Unknown);
        match expr_type {
            LuaType::Variadic(variadic) => match variadic.deref() {
                VariadicType::Base(base) => {
                    value_types.push((base.clone(), expr.clone()));
                }
                VariadicType::Multi(vecs) => {
                    for typ in vecs {
                        value_types.push((typ.clone(), expr.clone()));
                    }
                }
            },
            _ => value_types.push((expr_type.clone(), expr.clone())),
        }
    }
    value_types
}
