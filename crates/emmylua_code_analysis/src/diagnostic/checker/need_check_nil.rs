use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaIndexExpr,
};

use crate::{DiagnosticCode, SemanticModel};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::NeedCheckNil];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
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

    Some(())
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let prefix = call_expr.get_prefix_expr()?;
    let func = semantic_model.infer_expr(prefix.clone())?;
    if func.is_optional() {
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
    let prefix = index_expr.get_prefix_expr()?;
    let prefix_type = semantic_model.infer_expr(prefix.clone())?;
    if prefix_type.is_optional() {
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
        let left_type = semantic_model.infer_expr(left.clone())?;

        if left_type.is_optional() {
            context.add_diagnostic(
                DiagnosticCode::NeedCheckNil,
                left.get_range(),
                t!("%{name} value may be nil", name = left.syntax().text()).to_string(),
                None,
            );
        }

        let right_type = semantic_model.infer_expr(right.clone())?;
        if right_type.is_optional() {
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
