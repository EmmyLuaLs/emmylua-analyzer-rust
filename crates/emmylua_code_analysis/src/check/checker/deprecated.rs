//! deprecated: references to declarations / members / types marked `---@deprecated`.
//!
//! M0 scope: same-file declarations, same-file members (runtime `T.x` + `@field`), same-file named types,
//! and cross-file runtime members. Attribute syntax, `@field` on the inheritance chain, and `@deprecated` message text are left for later.

use emmylua_parser::{LuaAst, LuaAstNode, LuaDocNameType, LuaIndexExpr};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct DeprecatedChecker;

impl Checker for DeprecatedChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::Deprecated];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        check_name_uses(context, semantic_model);
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for node in root.descendants().filter_map(LuaAst::cast) {
            match node {
                LuaAst::LuaIndexExpr(index_expr) => {
                    check_index_expr(context, semantic_model, &index_expr);
                }
                LuaAst::LuaDocNameType(name_type) => {
                    check_doc_name_type(context, semantic_model, &name_type);
                }
                _ => {}
            }
        }
    }
}

/// Name reference -> declaration (same file) -> same-name type definition (class implementation) -> cross-file global declaration;
/// the definition location itself is not considered a reference.
fn check_name_uses(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
    let Some(facts) = semantic_model.file_facts() else {
        return;
    };
    for use_ in &facts.name_uses {
        let range = use_.syntax.get_range();
        // 1. Same-file local declaration only (no global fallback here).
        if let Some(decl_id) = semantic_model.resolve_local_name(range.start()) {
            if let Some(decl) = facts.decl_by_id(&decl_id) {
                if decl.deprecated && range != decl.name_range {
                    report_deprecated(context, range, &decl.name);
                } else if !decl.deprecated {
                    // `local Foo = {}` implements @class Foo: a same-name type definition is deprecated.
                    if let Some(def) = semantic_model.resolve_type_def(&decl.name)
                        && def.deprecated
                    {
                        report_deprecated(context, range, &decl.name);
                    }
                }
                continue;
            }
            // resolve_name falls back to a cross-file global declaration: if this file's facts do not have the declaration, fall through to step 2.
        }
        // 2. Cross-file global declaration (when no same-file declaration was resolved).
        if semantic_model.is_global_deprecated(use_.name.as_str()) {
            report_deprecated(context, range, &use_.name);
        }
    }
}

/// Index expression -> member (through the unified member-resolution entry point `resolve_member`) -> deprecated check.
fn check_index_expr(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
) {
    let Some(resolved) = semantic_model.resolve_member(index_expr) else {
        return;
    };
    let range = index_expr
        .get_index_key()
        .and_then(|key| key.get_range())
        .unwrap_or_else(|| index_expr.get_range());
    let Some(member_id) = resolved.member_id else {
        return;
    };
    let member_file = resolved.file_id.unwrap_or(semantic_model.file_id());
    let Some(facts) = semantic_model.file_facts_of(member_file) else {
        return;
    };
    let Some(member) = facts.member_by_id(&member_id) else {
        return;
    };
    if !member.deprecated {
        return;
    }
    // The member definition location itself (`T.old` in `function T.old() end`) is not a reference.
    if member_id.member_key_range() == Some(range) {
        return;
    }
    report_deprecated(context, range, &resolved.name);
}

/// Doc name type (`Old` in `---@type Old`) -> named type definition.
fn check_doc_name_type(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    name_type: &LuaDocNameType,
) {
    let Some(name) = name_type.get_name_text() else {
        return;
    };
    let Some(facts) = semantic_model.file_facts() else {
        return;
    };
    let Some(def) = facts
        .type_def_by_name(name.as_str())
        .or_else(|| facts.type_def_by_full_name(name.as_str()))
    else {
        return;
    };
    if def.deprecated {
        report_deprecated(context, name_type.get_range(), &def.name);
    }
}

fn report_deprecated(context: &mut CheckContext<'_>, range: rowan::TextRange, name: &str) {
    context.add_diagnostic(
        DiagnosticCode::Deprecated,
        range,
        t!("`%{name}` is deprecated", name = name),
    );
}
