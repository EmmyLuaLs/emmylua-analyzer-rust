//! # readonly - assigning to declarations / members marked `---@readonly`
//!
//! M0: if an assignment LHS resolves to a decl/member with the `readonly` flag, report `ReadOnly`.

use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaExpr, LuaVarExpr};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct ReadOnlyChecker;

impl Checker for ReadOnlyChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::ReadOnly];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for assign_stat in root.descendants().filter_map(LuaAssignStat::cast) {
            check_assign(context, semantic_model, &assign_stat);
        }
    }
}

fn check_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    assign_stat: &LuaAssignStat,
) {
    let (vars, _) = assign_stat.get_var_and_expr_list();
    for var in vars {
        match var {
            LuaVarExpr::NameExpr(name_expr) => {
                let Some(decl_id) = semantic_model.resolve_name(name_expr.get_position()) else {
                    continue;
                };
                let Some(facts) = semantic_model.file_facts() else {
                    continue;
                };
                let Some(decl) = facts.decl_by_id(&decl_id) else {
                    continue;
                };
                if decl.readonly {
                    context.add_diagnostic(
                        DiagnosticCode::ReadOnly,
                        name_expr.get_range(),
                        t!("The variable is marked as readonly and cannot be assigned to."),
                    );
                }
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                let range = index_expr.get_range();
                if let Some(member_id) = semantic_model
                    .resolve_member(&index_expr)
                    .and_then(|resolved| resolved.member_id)
                {
                    let member_file = semantic_model
                        .resolve_member(&index_expr)
                        .and_then(|resolved| resolved.file_id)
                        .unwrap_or(semantic_model.file_id());
                    let facts = semantic_model.file_facts_of(member_file);
                    if let Some(member) = facts.and_then(|facts| facts.member_by_id(&member_id))
                        && member.readonly
                    {
                        context.add_diagnostic(
                            DiagnosticCode::ReadOnly,
                            range,
                            t!("The property is marked as readonly and cannot be assigned to."),
                        );
                    }
                }
                // `---@readonly local t = {}; t.x = 1`: assigning to a member of a readonly table.
                if let Some(prefix) = index_expr.get_prefix_expr()
                    && readonly_prefix(semantic_model, &prefix)
                {
                    context.add_diagnostic(
                        DiagnosticCode::ReadOnly,
                        range,
                        t!("The variable is marked as readonly and cannot be assigned to."),
                    );
                }
            } // LuaVarExpr currently has only Name/Index variants.
        }
    }
}

/// Whether the prefix chain ultimately lands on a `readonly` declaration (`t.x = 1` -> t; `a.b.c = 1` -> a).
fn readonly_prefix(semantic_model: &SemanticModel<'_>, expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::NameExpr(name_expr) => semantic_model
            .resolve_name(name_expr.get_position())
            .and_then(|decl_id| {
                semantic_model
                    .file_facts()
                    .and_then(|facts| facts.decl_by_id(&decl_id))
            })
            .is_some_and(|decl| decl.readonly),
        LuaExpr::IndexExpr(index_expr) => index_expr
            .get_prefix_expr()
            .is_some_and(|prefix| readonly_prefix(semantic_model, &prefix)),
        _ => false,
    }
}
