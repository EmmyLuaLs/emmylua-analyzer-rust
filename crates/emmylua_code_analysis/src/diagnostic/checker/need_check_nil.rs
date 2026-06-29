use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaIndexExpr,
};

use crate::{DiagnosticCode, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct NeedCheckNilChecker;

impl Checker for NeedCheckNilChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::NeedCheckNil];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for expr in root.descendants::<LuaExpr>() {
            match expr {
                LuaExpr::CallExpr(call_expr) => {
                    check_call_expr(context, semantic_model, call_expr);
                }
                LuaExpr::BinaryExpr(binary_expr) => {
                    check_binary_expr(context, semantic_model, binary_expr);
                }
                LuaExpr::IndexExpr(index_expr) => {
                    check_index_expr(context, semantic_model, index_expr);
                }
                _ => {}
            }
        }
    }
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    if call_expr.has_safe_navigation() {
        return None;
    }

    let prefix = call_expr.get_prefix_expr()?;
    let func = semantic_model.infer_expr(prefix.clone()).ok()?;
    if func.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!("function %{name} may be nil", name = prefix.syntax().text()).to_string(),
            None,
        );
    }

    Some(())
}

fn check_index_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index_expr: LuaIndexExpr,
) -> Option<()> {
    if index_expr.is_safe_index() {
        return None;
    }

    let prefix = index_expr.get_prefix_expr()?;
    let prefix_type = semantic_model.infer_expr(prefix.clone()).ok()?;
    if prefix_type.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!("%{name} may be nil", name = prefix.syntax().text()).to_string(),
            None,
        );
    }

    Some(())
}

fn check_binary_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    binary_expr: LuaBinaryExpr,
) -> Option<()> {
    let op = binary_expr.get_op_token()?.get_op();
    if matches!(
        op,
        BinaryOperator::OpAdd
            | BinaryOperator::OpSub
            | BinaryOperator::OpMul
            | BinaryOperator::OpDiv
            | BinaryOperator::OpMod
    ) {
        let (left, right) = binary_expr.get_exprs()?;
        let left_type = semantic_model.infer_expr(left.clone()).ok()?;

        if left_type.is_nullable() {
            context.add_diagnostic(
                DiagnosticCode::NeedCheckNil,
                left.get_range(),
                t!("%{name} value may be nil", name = left.syntax().text()).to_string(),
                None,
            );
        }

        let right_type = semantic_model.infer_expr(right.clone()).ok()?;
        if right_type.is_nullable() {
            context.add_diagnostic(
                DiagnosticCode::NeedCheckNil,
                right.get_range(),
                t!("%{name} value may be nil", name = right.syntax().text()).to_string(),
                None,
            );
        }
    }

    Some(())
}
