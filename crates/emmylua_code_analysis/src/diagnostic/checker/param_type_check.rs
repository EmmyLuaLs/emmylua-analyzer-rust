use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaIndexExpr};
use rowan::TextRange;

use crate::{
    DiagnosticCode, LuaSemanticDeclId, LuaType, RenderLevel, SemanticDeclLevel, SemanticModel,
    TypeCheckFailReason, TypeCheckResult,
    diagnostic::checker::assign_type_mismatch::check_table_expr, humanize_type,
};

use super::{Checker, DiagnosticContext};

pub struct ParamTypeCheckChecker;

impl Checker for ParamTypeCheckChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::ParamTypeNotMatch,
        DiagnosticCode::AssignTypeMismatch,
    ];

    /// a simple implementation of param type check, later we will do better
    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            if let LuaAst::LuaCallExpr(call_expr) = node {
                check_call_expr(context, semantic_model, call_expr);
            }
        }
    }
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let mut params = func.get_params().to_vec();
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let (mut arg_types, mut arg_ranges): (Vec<LuaType>, Vec<TextRange>) = semantic_model
        .infer_expr_list_types(&arg_exprs, None)
        .into_iter()
        .unzip();
    dbg!(&arg_types);

    let colon_call = call_expr.is_colon_call();
    let colon_define = func.is_colon_define();
    match (colon_call, colon_define) {
        (true, true) | (false, false) => {}
        (false, true) => {
            // 插入 self 参数
            params.insert(0, ("self".into(), Some(LuaType::SelfInfer)));
        }
        (true, false) => {
            // 往调用参数插入插入调用者类型
            arg_types.insert(0, get_call_source_type(semantic_model, &call_expr)?);
            arg_ranges.insert(0, call_expr.get_colon_token()?.get_range());
        }
    }

    for (idx, param) in params.iter().enumerate() {
        if param.0 == "..." {
            if arg_types.len() < idx {
                break;
            }

            if let Some(variadic_type) = param.1.clone() {
                check_variadic_param_match_args(
                    context,
                    semantic_model,
                    &variadic_type,
                    &arg_types[idx..],
                    &arg_ranges[idx..],
                );
            }

            break;
        }

        if let Some(param_type) = param.1.clone() {
            let arg_type = arg_types.get(idx).unwrap_or(&LuaType::Any);
            let mut check_type = param_type.clone();
            // 对于第一个参数, 他有可能是`:`调用, 所以需要特殊处理
            if idx == 0
                && param_type.is_self_infer()
                && let Some(result) = get_call_source_type(semantic_model, &call_expr)
            {
                check_type = result;
            }
            let result = semantic_model.type_check_detail(&check_type, arg_type);
            if result.is_err() {
                // 这里执行了`AssignTypeMismatch`的检查
                if arg_type.is_table() {
                    let arg_expr_idx = match (colon_call, colon_define) {
                        (true, false) => {
                            if idx == 0 {
                                continue;
                            } else {
                                idx - 1
                            }
                        }
                        _ => idx,
                    };

                    // 表字段已经报错了, 则不添加参数不匹配的诊断避免干扰
                    if let Some(arg_expr) = arg_exprs.get(arg_expr_idx)
                        && let Some(add_diagnostic) = check_table_expr(
                            context,
                            semantic_model,
                            arg_expr,
                            Some(&param_type),
                            Some(arg_type),
                        )
                        && add_diagnostic
                    {
                        continue;
                    }
                }

                try_add_diagnostic(
                    context,
                    semantic_model,
                    *arg_ranges.get(idx)?,
                    &param_type,
                    arg_type,
                    result,
                );
            }
        }
    }

    Some(())
}

fn check_variadic_param_match_args(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    variadic_type: &LuaType,
    arg_types: &[LuaType],
    arg_ranges: &[TextRange],
) {
    for (arg_type, arg_range) in arg_types.iter().zip(arg_ranges.iter()) {
        let result = semantic_model.type_check_detail(variadic_type, arg_type);
        if result.is_err() {
            try_add_diagnostic(
                context,
                semantic_model,
                *arg_range,
                variadic_type,
                arg_type,
                result,
            );
        }
    }
}

fn try_add_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    param_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    if let (LuaType::Integer, LuaType::FloatConst(f)) = (param_type, expr_type)
        && f.fract() == 0.0
    {
        return;
    }

    add_type_check_diagnostic(
        context,
        semantic_model,
        range,
        param_type,
        expr_type,
        result,
    );
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    param_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    let db = semantic_model.get_db();
    match result {
        Ok(_) => (),
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason,
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DonotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => "type recursion".to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::ParamTypeNotMatch,
                range,
                t!(
                    "expected `%{source}` but found `%{found}`. %{reason}",
                    source = humanize_type(db, param_type, RenderLevel::Simple),
                    found = humanize_type(db, expr_type, RenderLevel::Simple),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}

pub fn get_call_source_type(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<LuaType> {
    match call_expr.get_prefix_expr()? {
        LuaExpr::IndexExpr(index_expr) => {
            let decl = semantic_model.find_decl(
                index_expr.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )?;

            if let LuaSemanticDeclId::Member(member_id) = decl
                && let Some(LuaSemanticDeclId::Member(member_id)) =
                    semantic_model.get_member_origin_owner(member_id)
            {
                let root = semantic_model
                    .get_db()
                    .get_vfs()
                    .get_syntax_tree(&member_id.file_id)?
                    .get_red_root();
                let cur_node = member_id.get_syntax_id().to_node_from_root(&root)?;
                let index_expr = LuaIndexExpr::cast(cur_node)?;

                return index_expr.get_prefix_expr().map(|prefix_expr| {
                    semantic_model
                        .infer_expr(prefix_expr.clone())
                        .unwrap_or(LuaType::SelfInfer)
                });
            }

            return if let Some(prefix_expr) = index_expr.get_prefix_expr() {
                let expr_type = semantic_model
                    .infer_expr(prefix_expr.clone())
                    .unwrap_or(LuaType::SelfInfer);
                Some(expr_type)
            } else {
                None
            };
        }
        LuaExpr::NameExpr(name_expr) => {
            let decl = semantic_model.find_decl(
                name_expr.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )?;
            if let LuaSemanticDeclId::Member(member_id) = decl {
                let root = semantic_model
                    .get_db()
                    .get_vfs()
                    .get_syntax_tree(&member_id.file_id)?
                    .get_red_root();
                let cur_node = member_id.get_syntax_id().to_node_from_root(&root)?;
                let index_expr = LuaIndexExpr::cast(cur_node)?;

                return index_expr.get_prefix_expr().map(|prefix_expr| {
                    semantic_model
                        .infer_expr(prefix_expr.clone())
                        .unwrap_or(LuaType::SelfInfer)
                });
            }

            return None;
        }
        _ => {}
    }

    None
}
