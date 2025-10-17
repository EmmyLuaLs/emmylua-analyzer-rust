use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaExpr, LuaFuncStat, LuaReturnStat, LuaSyntaxKind, LuaVarExpr,
};
use rowan::{NodeOrToken, TextRange};

use crate::{
    DiagnosticCode, LuaSemanticDeclId, LuaSignatureId, LuaType, SemanticDeclLevel, SemanticModel,
    SignatureReturnStatus, TypeCheckFailReason, TypeCheckResult,
    diagnostic::checker::{assign_type_mismatch::check_table_expr, humanize_lint_type},
};

use super::{Checker, DiagnosticContext, get_return_stats};

pub struct ReturnTypeMismatch;

impl Checker for ReturnTypeMismatch {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::ReturnTypeMismatch,
        DiagnosticCode::AssignTypeMismatch,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for closure_expr in root.descendants::<LuaClosureExpr>() {
            check_closure_expr(context, semantic_model, &closure_expr);
        }
    }
}

fn check_closure_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), closure_expr);
    let signature = context.db.get_signature_index().get(&signature_id)?;
    if signature.resolve_return != SignatureReturnStatus::DocResolve {
        return None;
    }
    let return_type = signature.get_return_type();
    let self_type = get_self_type(semantic_model, closure_expr);
    for return_stat in get_return_stats(closure_expr) {
        check_return_stat(
            context,
            semantic_model,
            &self_type,
            &return_type,
            &return_stat,
        );
    }
    Some(())
}

fn check_return_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    self_type: &Option<LuaType>,
    return_type: &LuaType,
    return_stat: &LuaReturnStat,
) -> Option<()> {
    let return_exprs = return_stat.get_expr_list().collect::<Vec<_>>();
    let (return_expr_types, return_expr_ranges) = {
        let infos = semantic_model.infer_expr_list_types(&return_exprs, None);
        let mut return_expr_types = infos.iter().map(|(typ, _)| typ.clone()).collect::<Vec<_>>();
        // 解决 setmetatable 的返回值类型问题
        let setmetatable_index = has_setmetatable(semantic_model, return_stat);
        if let Some(setmetatable_index) = setmetatable_index {
            return_expr_types[setmetatable_index] = LuaType::Any;
        }
        let return_expr_ranges = infos.iter().map(|(_, range)| *range).collect::<Vec<_>>();
        (return_expr_types, return_expr_ranges)
    };

    if return_expr_types.is_empty() || return_expr_ranges.is_empty() {
        return None;
    }

    match return_type {
        LuaType::Variadic(variadic) => {
            for (index, return_expr_type) in return_expr_types.iter().enumerate() {
                let doc_return_type = variadic.get_type(index)?;
                let mut check_type = doc_return_type;
                if doc_return_type.is_self_infer()
                    && let Some(self_type) = self_type
                {
                    check_type = self_type;
                }

                let result = semantic_model.type_check_detail(check_type, return_expr_type);
                if result.is_err() {
                    if return_expr_type.is_table()
                        && let Some(return_expr) = return_exprs.get(index)
                    {
                        check_table_expr(
                            context,
                            semantic_model,
                            rowan::NodeOrToken::Node(return_expr.syntax().clone()),
                            return_expr,
                            Some(check_type),
                        );
                    }

                    add_type_check_diagnostic(
                        context,
                        semantic_model,
                        index,
                        *return_expr_ranges
                            .get(index)
                            .unwrap_or(&return_stat.get_range()),
                        check_type,
                        return_expr_type,
                        result,
                    );
                }
            }
        }
        _ => {
            let mut check_type = return_type;
            if return_type.is_self_infer()
                && let Some(self_type) = self_type
            {
                check_type = self_type;
            }
            let return_expr_type = &return_expr_types[0];
            let return_expr_range = return_expr_ranges[0];
            let result = semantic_model.type_check_detail(check_type, return_expr_type);
            if result.is_err() {
                if return_expr_type.is_table()
                    && let Some(return_expr) = return_exprs.first()
                {
                    // 表字段已经报错了, 则不添加返回值不匹配的诊断避免干扰
                    if let Some(add_diagnostic) = check_table_expr(
                        context,
                        semantic_model,
                        rowan::NodeOrToken::Node(return_expr.syntax().clone()),
                        return_expr,
                        Some(return_type),
                    ) && add_diagnostic
                    {
                        return Some(());
                    }
                }
                add_type_check_diagnostic(
                    context,
                    semantic_model,
                    0,
                    return_expr_range,
                    return_type,
                    return_expr_type,
                    result,
                );
            }
        }
    }

    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index: usize,
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
                DiagnosticCode::ReturnTypeMismatch,
                range,
                t!(
                    "Annotations specify that return value %{index} has a type of `%{source}`, returning value of type `%{found}` here instead. %{reason}",
                    index = index + 1,
                    source = humanize_lint_type(db, param_type),
                    found = humanize_lint_type(db, expr_type),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}

fn has_setmetatable(semantic_model: &SemanticModel, return_stat: &LuaReturnStat) -> Option<usize> {
    for (index, expr) in return_stat.get_expr_list().enumerate() {
        match expr {
            LuaExpr::CallExpr(call_expr) => {
                if call_expr.is_setmetatable() {
                    return Some(index);
                }
            }
            _ => {
                let decl = semantic_model.find_decl(
                    NodeOrToken::Node(expr.syntax().clone()),
                    SemanticDeclLevel::Trace(50),
                );
                if let Some(LuaSemanticDeclId::LuaDecl(decl_id)) = decl {
                    let decl = semantic_model.get_db().get_decl_index().get_decl(&decl_id);
                    if let Some(decl) = decl
                        && decl.get_value_syntax_id()?.get_kind()
                            == LuaSyntaxKind::SetmetatableCallExpr
                    {
                        return Some(index);
                    }
                }
            }
        }
    }
    None
}

/// 获取 self 实际类型
fn get_self_type(semantic_model: &SemanticModel, closure_expr: &LuaClosureExpr) -> Option<LuaType> {
    let parent = closure_expr.syntax().parent()?;
    let func_stat = LuaFuncStat::cast(parent)?;
    let func_name = func_stat.get_func_name()?;
    match func_name {
        LuaVarExpr::IndexExpr(index_expr) => {
            let prefix_expr = index_expr.get_prefix_expr()?;
            semantic_model.infer_expr(prefix_expr).ok()
        }
        _ => None,
    }
}
