//! # enum_value_mismatch - the constant compared with an enum variable is not in the enum value set
//!
//! M0: `enum_var == constant` (if/elseif condition): the constant literal text must be in the set of enum `@field` value nodes
//! text set. Typed extraction of `@field` constant values (IntegerConst etc.) is left for later.

use emmylua_parser::{BinaryOperator, LuaAst, LuaAstNode, LuaExpr, LuaLiteralToken};

use crate::DiagnosticCode;
use crate::LuaType;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::member::type_def_of;

use super::{CheckContext, Checker};

pub struct EnumValueMismatchChecker;

impl Checker for EnumValueMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::EnumValueMismatch];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for node in root.descendants().filter_map(LuaAst::cast) {
            let condition_expr = match node {
                LuaAst::LuaIfStat(if_stat) => if_stat.get_condition_expr(),
                LuaAst::LuaElseIfClauseStat(elseif_stat) => elseif_stat.get_condition_expr(),
                _ => None,
            };
            if let Some(expr) = condition_expr {
                check_condition(context, semantic_model, &expr);
            }
        }
    }
}

fn check_condition(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    condition_expr: &LuaExpr,
) {
    let LuaExpr::BinaryExpr(binary_expr) = condition_expr else {
        return;
    };
    let Some(op_token) = binary_expr.get_op_token() else {
        return;
    };
    if !matches!(
        op_token.get_op(),
        BinaryOperator::OpEq | BinaryOperator::OpNe
    ) {
        return;
    }
    let Some((left_expr, right_expr)) = binary_expr.get_exprs() else {
        return;
    };
    let left_type = semantic_model.type_of_expr(left_expr.get_syntax_id());
    let right_type = semantic_model.type_of_expr(right_expr.get_syntax_id());
    // The left side is an enum variable and the right side is a constant (or vice versa).
    check_pair(context, semantic_model, &right_expr, &left_type);
    check_pair(context, semantic_model, &left_expr, &right_type);
}

fn check_pair(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    value_expr: &LuaExpr,
    enum_type: &LuaType,
) {
    let (LuaType::Ref(id) | LuaType::Def(id)) = enum_type else {
        return;
    };
    let Some(def) = type_def_of(semantic_model, id) else {
        return;
    };
    if def.kind != crate::salsa_builder::def::TypeDefKind::Enum {
        return;
    }
    // Constant literal text.
    let Some(value_text) = literal_text(value_expr) else {
        return;
    };
    // Enum value text set: `@field` (owner=TypeDef) plus runtime table fields (`local Status = {...}`, owner=Decl).
    let tree = semantic_model.syntax_tree().expect("tree");
    let root = tree.get_red_root();
    let mut member_refs = semantic_model.members_of_owner(&def.id).as_slice().to_vec();
    if let Some(facts) = semantic_model.file_facts_of(def.file_id)
        && let Some(decl) = facts.decl_named(def.name.as_str())
    {
        member_refs.extend(semantic_model.members_of_owner(&decl.id));
    }
    let enum_value_texts: Vec<String> = member_refs
        .iter()
        .filter_map(|member_ref| {
            let facts = semantic_model.file_facts_of(member_ref.file_id)?;
            let member = facts.member_by_id(&member_ref.id)?;
            let value_syntax = member.value_syntax?;
            let node = value_syntax.to_node_from_root(&root)?;
            // Runtime member: LuaExpr literal; `@field`: doc type literal node.
            if let Some(text) = LuaExpr::cast(node.clone()).and_then(|expr| literal_text(&expr)) {
                return Some(text);
            }
            let raw = node.text().to_string();
            Some(raw.trim_matches(['"', '\'']).to_string())
        })
        .collect();
    if enum_value_texts.iter().any(|text| text == &value_text) {
        return;
    }
    context.add_diagnostic(
        DiagnosticCode::EnumValueMismatch,
        value_expr.get_range(),
        t!(
            "Value '%{value}' does not match any enum value. Expected one of: %{enum_values}",
            value = value_text,
            enum_values = enum_value_texts.join(", ")
        ),
    );
}

/// Text of a literal expression (number / string / boolean).
fn literal_text(expr: &LuaExpr) -> Option<String> {
    let LuaExpr::LiteralExpr(lit) = expr else {
        return None;
    };
    match lit.get_literal()? {
        LuaLiteralToken::Number(number) => Some(format!("{}", number.get_number_value())),
        LuaLiteralToken::String(str) => Some(str.get_value()),
        LuaLiteralToken::Bool(bool) => Some(bool.is_true().to_string()),
        LuaLiteralToken::Nil(_) => Some("nil".to_string()),
        _ => None,
    }
}
