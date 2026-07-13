//! Need-check-nil checker — salsa-native.

use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaIndexExpr,
    PathTrait,
};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for expr in root.descendants::<LuaExpr>() {
        match expr {
            LuaExpr::CallExpr(call) => check_call(context, model, call),
            LuaExpr::BinaryExpr(binary) => check_binary(context, model, binary),
            LuaExpr::IndexExpr(index) => check_index(context, model, index),
            _ => {}
        }
    }
}

fn check_call(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(prefix) = call.get_prefix_expr() else {
        return;
    };
    let Ok(func) = model.infer_expr(prefix.clone()) else {
        return;
    };
    if func.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!("function %{name} may be nil", name = prefix.syntax().text()).to_string(),
            None,
        );
    }
}

fn check_index(context: &mut DiagnosticContext, model: &SemanticModel, index: LuaIndexExpr) {
    let Some(prefix) = index.get_prefix_expr() else {
        return;
    };
    let Ok(prefix_type) = model.infer_expr(prefix.clone()) else {
        return;
    };
    if prefix_type.is_nullable() {
        if assign_rhs_asserts_lhs_prefix(&prefix, &index) {
            return;
        }

        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!("%{name} may be nil", name = prefix.syntax().text()).to_string(),
            None,
        );
    }
}

fn check_binary(context: &mut DiagnosticContext, model: &SemanticModel, binary: LuaBinaryExpr) {
    let Some(op_token) = binary.get_op_token() else {
        return;
    };
    if !matches!(
        op_token.get_op(),
        BinaryOperator::OpAdd
            | BinaryOperator::OpSub
            | BinaryOperator::OpMul
            | BinaryOperator::OpDiv
            | BinaryOperator::OpMod
    ) {
        return;
    }
    let Some((left, right)) = binary.get_exprs() else {
        return;
    };

    let Ok(left_type) = model.infer_expr(left.clone()) else {
        return;
    };
    if left_type.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            left.get_range(),
            t!("%{name} value may be nil", name = left.syntax().text()).to_string(),
            None,
        );
    }

    let Ok(right_type) = model.infer_expr(right.clone()) else {
        return;
    };
    if right_type.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            right.get_range(),
            t!("%{name} value may be nil", name = right.syntax().text()).to_string(),
            None,
        );
    }
}

fn assign_rhs_asserts_lhs_prefix(prefix: &LuaExpr, index_expr: &LuaIndexExpr) -> bool {
    // 只认可同一条赋值语句里的 RHS assert, 避免跨语句误消除 nil 诊断.
    let Some(assign) = index_expr.ancestors::<LuaAssignStat>().next() else {
        return false;
    };

    let (vars, exprs) = assign.get_var_and_expr_list();
    let index_range = index_expr.get_range();
    // 当前被检查的索引必须属于赋值左侧, 例如 `res[1][1]` 中的外层访问.
    if !vars
        .iter()
        .any(|var| var.get_range().contains_range(index_range))
    {
        return false;
    }

    let Some(prefix_path) = expr_access_path(prefix) else {
        return false;
    };

    // RHS 里 assert 的必须是同一个访问路径, 例如 `assert(res[1])` 才能保护 `res[1][1]`.
    exprs
        .iter()
        .any(|expr| expr_contains_asserted_path(expr, &prefix_path))
}

fn expr_contains_asserted_path(expr: &LuaExpr, expected_path: &str) -> bool {
    match expr {
        // 闭包体不会在当前赋值求值时立即执行, 里面的 assert 不能保护当前左侧访问.
        LuaExpr::ClosureExpr(_) => false,
        LuaExpr::CallExpr(call_expr) => {
            // assert 的第一个参数才是被证明非 nil 的值.
            if call_expr.is_assert()
                && call_expr
                    .get_args_list()
                    .and_then(|args| args.get_args().next())
                    .and_then(|arg| expr_access_path(&arg))
                    .as_deref()
                    == Some(expected_path)
            {
                return true;
            }

            // assert 可能嵌在调用前缀或参数里, 递归查找当前 RHS 表达式树.
            if call_expr
                .get_prefix_expr()
                .is_some_and(|prefix| expr_contains_asserted_path(&prefix, expected_path))
            {
                return true;
            }

            call_expr.get_args_list().is_some_and(|args| {
                args.get_args()
                    .any(|arg| expr_contains_asserted_path(&arg, expected_path))
            })
        }
        LuaExpr::ParenExpr(paren_expr) => paren_expr
            .get_expr()
            .is_some_and(|inner| expr_contains_asserted_path(&inner, expected_path)),
        _ => expr
            .children::<LuaExpr>()
            .any(|child| expr_contains_asserted_path(&child, expected_path)),
    }
}

fn expr_access_path(expr: &LuaExpr) -> Option<String> {
    // 只比较稳定的变量/索引访问路径
    match expr {
        LuaExpr::NameExpr(name_expr) => name_expr.get_access_path().map(|path| path.to_string()),
        LuaExpr::IndexExpr(index_expr) => index_expr.get_access_path().map(|path| path.to_string()),
        LuaExpr::ParenExpr(paren_expr) => expr_access_path(&paren_expr.get_expr()?),
        _ => None,
    }
}
