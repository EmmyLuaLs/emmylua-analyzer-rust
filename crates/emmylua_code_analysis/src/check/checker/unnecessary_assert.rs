//! # unnecessary_assert —— `assert()` conditions that are always truthy / always falsy
//!
//! Driven by value-range predicates (`is_always_truthy/is_always_falsy`); syntax + types only.

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaDocType, LuaExpr, LuaLiteralToken};

use crate::DiagnosticCode;
use crate::LuaType;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct UnnecessaryAssertChecker;

impl Checker for UnnecessaryAssertChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UnnecessaryAssert];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for call_expr in root.descendants().filter_map(LuaCallExpr::cast) {
            if call_expr.is_assert() {
                check_assert_rule(context, semantic_model, &call_expr);
            }
        }
    }
}

fn check_assert_rule(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    let Some(args) = call_expr.get_args_list() else {
        return;
    };
    let Some(first_expr) = args.get_args().next() else {
        return;
    };
    let expr_type = doc_literal_type(semantic_model, &first_expr).unwrap_or_else(|| {
        semantic_model.type_of_expr_at(first_expr.get_syntax_id(), first_expr.get_range().start())
    });
    if expr_type.is_always_truthy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryAssert,
            call_expr.get_range(),
            t!("Unnecessary assert: this expression is always truthy"),
        );
    } else if expr_type.is_always_falsy() {
        context.add_diagnostic(
            DiagnosticCode::UnnecessaryAssert,
            call_expr.get_range(),
            t!("Impossible assert: this expression is always falsy; prefer `error()`"),
        );
    }
}

/// Literal annotations on declarations such as `---@type false` / `---@type 1` (the projection layer may fall back to base types).
fn doc_literal_type(semantic_model: &SemanticModel<'_>, expr: &LuaExpr) -> Option<LuaType> {
    let LuaExpr::NameExpr(name_expr) = expr else {
        return None;
    };
    let decl_id = semantic_model.resolve_name(name_expr.get_position())?;
    let facts = semantic_model.file_facts()?;
    let decl = facts.decl_by_id(&decl_id)?;
    let syntax = decl.doc_type_syntax?;
    let tree = semantic_model.syntax_tree()?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaDocType::Literal(literal) = LuaDocType::cast(node)? else {
        return None;
    };
    match literal.get_literal()? {
        LuaLiteralToken::Bool(bool_token) => Some(LuaType::BooleanConst(bool_token.is_true())),
        LuaLiteralToken::Number(number_token) => match number_token.get_number_value() {
            emmylua_parser::NumberResult::Int(i) => Some(LuaType::IntegerConst(i)),
            _ => Some(LuaType::Number),
        },
        LuaLiteralToken::String(string_token) => Some(LuaType::StringConst(
            smol_str::SmolStr::new(string_token.get_value()).into(),
        )),
        LuaLiteralToken::Nil(_) => Some(LuaType::Nil),
        _ => None,
    }
}
