//! # need_check_nil - direct call / member access on a possibly nil prefix
//!
//! M0: if the call callee type is nullable, the index prefix is nullable, or the operand of `#`
//! or of arithmetic is nullable, report NeedCheckNil; safe navigation (`?.`/`?:`) skips.

use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaIndexExpr,
    LuaUnaryExpr, PathTrait, UnaryOperator,
};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::{CheckContext, Checker};

pub struct NeedCheckNilChecker;

impl Checker for NeedCheckNilChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::NeedCheckNil];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for expr in root.descendants().filter_map(LuaExpr::cast) {
            match expr {
                LuaExpr::CallExpr(call_expr) => check_call(context, semantic_model, &call_expr),
                LuaExpr::IndexExpr(index_expr) => check_index(context, semantic_model, &index_expr),
                LuaExpr::BinaryExpr(binary_expr) => {
                    check_arithmetic(context, semantic_model, &binary_expr);
                }
                LuaExpr::UnaryExpr(unary_expr) => check_unary(context, semantic_model, &unary_expr),
                _ => {}
            }
        }
    }
}

fn check_call(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    if call_expr.has_safe_navigation() {
        return;
    }
    let Some(prefix) = call_expr.get_prefix_expr() else {
        return;
    };
    let ty = prefix_type(semantic_model, &prefix);
    if ty.is_nullable() {
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!(
                "function `%{name}` may be nil",
                name = prefix.syntax().text()
            ),
        );
    }
}

fn check_index(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
) {
    if index_expr.is_safe_index() {
        return;
    }
    let Some(prefix) = index_expr.get_prefix_expr() else {
        return;
    };
    let ty = prefix_type(semantic_model, &prefix);
    if ty.is_nullable() {
        // An `assert(res[1])` on the RHS of the same assignment can protect the LHS `res[1][1]`;
        // `assert(res[2])` protects a different prefix, so NeedCheckNil is still reported.
        if assign_rhs_asserts_lhs_prefix(&prefix, index_expr) {
            return;
        }
        context.add_diagnostic(
            DiagnosticCode::NeedCheckNil,
            prefix.get_range(),
            t!("%{name} may be nil", name = prefix.syntax().text()),
        );
    }
}

/// Arithmetic on nil raises, as indexing nil does. The operand's own flow type decides: the result
/// type is built from that same type (`integer? - 1` is `integer`), so this is where its nil is
/// reported. Concatenation and bitwise operators are left out, as in 0.25.1.
fn check_arithmetic(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    binary_expr: &LuaBinaryExpr,
) {
    if !binary_expr.get_op_token().is_some_and(|op| {
        matches!(
            op.get_op(),
            BinaryOperator::OpAdd
                | BinaryOperator::OpSub
                | BinaryOperator::OpMul
                | BinaryOperator::OpDiv
                | BinaryOperator::OpIDiv
                | BinaryOperator::OpMod
                | BinaryOperator::OpPow
        )
    }) {
        return;
    }
    let Some((left, right)) = binary_expr.get_exprs() else {
        return;
    };
    for operand in [left, right] {
        check_arithmetic_operand(context, semantic_model, &operand);
    }
}

/// `#x` raises on a nil `x` just as `x.y` does, and `-x` like `x - 1`.
fn check_unary(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    unary_expr: &LuaUnaryExpr,
) {
    let Some(op) = unary_expr.get_op_token().map(|op| op.get_op()) else {
        return;
    };
    let Some(operand) = unary_expr.get_expr() else {
        return;
    };
    match op {
        UnaryOperator::OpLen => {
            let ty = prefix_type(semantic_model, &operand);
            if ty.is_nullable() && !ty.is_nil() {
                report_may_be_nil(context, &operand);
            }
        }
        UnaryOperator::OpUnm => check_arithmetic_operand(context, semantic_model, &operand),
        _ => {}
    }
}

fn check_arithmetic_operand(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    operand: &LuaExpr,
) {
    let ty = value_type_at(semantic_model, operand);
    // Only a value that may be nil: a bare `nil` operand is an inference gap far more often than
    // nil arithmetic (a field of a `table` call result, `select` over unknown values), and its
    // result type is unchanged, so nothing is hidden by skipping it.
    if ty.is_nullable() && !ty.is_nil() {
        report_may_be_nil(context, operand);
    }
}

fn report_may_be_nil(context: &mut CheckContext<'_>, operand: &LuaExpr) {
    context.add_diagnostic(
        DiagnosticCode::NeedCheckNil,
        operand.get_range(),
        t!("%{name} may be nil", name = operand.syntax().text()),
    );
}

/// Only accept RHS asserts in the same assignment statement to avoid incorrectly suppressing nil diagnostics across statements.
fn assign_rhs_asserts_lhs_prefix(prefix: &LuaExpr, index_expr: &LuaIndexExpr) -> bool {
    let Some(assign) = index_expr.ancestors::<LuaAssignStat>().next() else {
        return false;
    };
    let (vars, exprs) = assign.get_var_and_expr_list();
    let index_range = index_expr.get_range();
    // The checked index must belong to the assignment LHS (the outer access of `res[1][1]`).
    if !vars
        .iter()
        .any(|var| var.get_range().contains_range(index_range))
    {
        return false;
    }
    let Some(prefix_path) = expr_access_path(prefix) else {
        return false;
    };
    exprs
        .iter()
        .any(|expr| expr_contains_asserted_path(expr, &prefix_path))
}

fn expr_contains_asserted_path(expr: &LuaExpr, expected_path: &str) -> bool {
    match expr {
        // Closure bodies do not run immediately during this assignment; asserts inside them cannot protect the current LHS access.
        LuaExpr::ClosureExpr(_) => false,
        LuaExpr::CallExpr(call_expr) => {
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
    match expr {
        LuaExpr::NameExpr(name_expr) => name_expr.get_access_path().map(|path| path.to_string()),
        LuaExpr::IndexExpr(index_expr) => index_expr.get_access_path().map(|path| path.to_string()),
        LuaExpr::ParenExpr(paren_expr) => expr_access_path(&paren_expr.get_expr()?),
        _ => None,
    }
}

/// The flow-aware type of `expr` read as one value: a call's result list gives its first slot, as
/// the VM reads an operand, so a call that may return nothing (`string|()`) may be nil here. Only
/// a call is projected: another expression's type is already a value, and projecting a flow type
/// built from a result list (`string.find(..) or n`) brings back a nil the flow had removed.
fn value_type_at(semantic_model: &SemanticModel<'_>, expr: &LuaExpr) -> LuaType {
    let ty = semantic_model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start());
    if !matches!(expr, LuaExpr::CallExpr(_)) || !ty.contain_multi_return() {
        return ty;
    }
    ty.get_result_slot_type(0).unwrap_or(ty)
}

/// Prefix type: name expressions use flow-sensitive type (after conditional narrowing); others use flow-aware expression type (including inline casts).
/// VM inference for array indices may degrade to Unknown; here use `T | nil` as a fallback for `T[]` indexing.
fn prefix_type(semantic_model: &SemanticModel<'_>, prefix: &LuaExpr) -> LuaType {
    if let LuaExpr::NameExpr(name_expr) = prefix
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
    {
        return semantic_model.type_of_decl_at(&decl, name_expr.get_position());
    }
    let ty = semantic_model.type_of_expr_at(prefix.get_syntax_id(), prefix.get_range().start());
    if !matches!(ty, LuaType::Unknown) {
        return ty;
    }
    let LuaExpr::IndexExpr(index_expr) = prefix else {
        return ty;
    };
    let Some(inner) = index_expr.get_prefix_expr() else {
        return ty;
    };
    let inner_ty = semantic_model.type_of_expr(inner.get_syntax_id());
    match inner_ty {
        LuaType::Array(array) => {
            let base = array.get_base().clone();
            // Indexing calls on arrays of functions do not report NeedCheckNil per legacy semantics
            // (for `for i=1,#calls do calls[i](...) end`).
            if matches!(
                base,
                LuaType::DocFunction(_) | LuaType::Signature(_) | LuaType::Function
            ) {
                base
            } else if semantic_model.strict_array_index() || base.is_nullable() {
                LuaType::optional(base)
            } else {
                base
            }
        }
        LuaType::Union(union) => {
            let mut types = Vec::new();
            let mut found = false;
            for component in union.iter() {
                if let LuaType::Array(array) = component {
                    let base = array.get_base().clone();
                    found = true;
                    if semantic_model.strict_array_index() || base.is_nullable() {
                        types.push(base);
                        types.push(LuaType::Nil);
                    } else {
                        types.push(base);
                    }
                }
            }
            if found { LuaType::union(types) } else { ty }
        }
        _ => ty,
    }
}
