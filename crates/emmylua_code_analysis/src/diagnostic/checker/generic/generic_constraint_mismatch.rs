use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaDocTagType};
use rowan::TextRange;

use crate::diagnostic::checker::generic::infer_type::infer_type;
use crate::diagnostic::checker::param_type_check::get_call_source_type;
use crate::{
    humanize_type, DiagnosticCode, GenericTplId, LuaMemberOwner, LuaSemanticDeclId, LuaSignature,
    LuaStringTplType, LuaType, RenderLevel, SemanticDeclLevel, SemanticModel, TypeCheckFailReason,
    TypeCheckResult,
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
        let type_ref = infer_type(semantic_model, &doc_type);
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

        let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
        let mut arg_infos =
            semantic_model.infer_multi_value_adjusted_expression_types(&arg_exprs, None);
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

            match param_type {
                LuaType::StrTplRef(str_tpl_ref) => {
                    let extend_type = get_extend_type(
                        semantic_model,
                        &call_expr,
                        str_tpl_ref.get_tpl_id(),
                        signature,
                    );
                    check_str_tpl_ref(
                        context,
                        semantic_model,
                        &call_expr,
                        i,
                        str_tpl_ref,
                        extend_type,
                    );
                }
                LuaType::TplRef(tpl_ref) => {
                    let extend_type = get_extend_type(
                        semantic_model,
                        &call_expr,
                        tpl_ref.get_tpl_id(),
                        signature,
                    );
                    check_tpl_ref(context, semantic_model, &extend_type, arg_infos.get(i));
                }
                _ => {}
            }
        }
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
    call_expr: &LuaCallExpr,
    param_index: usize,
    str_tpl_ref: &LuaStringTplType,
    extend_type: Option<LuaType>,
) -> Option<()> {
    let arg_expr = call_expr.get_args_list()?.get_args().nth(param_index)?;
    let arg_type = semantic_model.infer_expr(arg_expr.clone()).ok()?;
    let range = arg_expr.get_range();

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
        add_type_check_diagnostic(context, semantic_model, arg_info.1, &extend_type, result);
    }
    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    extend_type: &LuaType,
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
                    "the generic constraint must be a subclass of `%{source}`. %{reason}",
                    source = humanize_type(db, &extend_type, RenderLevel::Simple),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}
