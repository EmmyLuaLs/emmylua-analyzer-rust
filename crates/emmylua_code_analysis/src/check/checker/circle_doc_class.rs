//! circle_doc_class: checks for circular `---@class` inheritance.
//!
//! Mirrors the old `diagnostic::checker::circle_doc_class`: BFS the parent-type chain from each class,
//! and when it reaches itself -> report Circularly inherited classes (range: from the name to the end of supers).

use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaDocTagClass};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::semantic_db::def::TypeDefKind;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct CircleDocClassChecker;

impl Checker for CircleDocClassChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::CircleDocClass];

    /// Checks classes for circular inheritance.
    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for tag in root.descendants().filter_map(LuaDocTagClass::cast) {
            check_doc_tag_class(context, semantic_model, &tag);
        }
    }
}

fn check_doc_tag_class(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    tag: &LuaDocTagClass,
) {
    let Some(name_token) = tag.get_name_token() else {
        return;
    };
    let Some(facts) = semantic_model.file_facts() else {
        return;
    };
    let Some(class_def) = facts.type_def_by_name_range(name_token.get_range()) else {
        return;
    };
    if class_def.kind != TypeDefKind::Class {
        return;
    }
    let class_id = class_def.id.clone();
    let mut queue = semantic_model.base_type_defs(&class_def.type_decl_id());
    let mut visited = HashSet::new();
    while let Some(current) = queue.pop() {
        if current.id == class_id {
            context.add_diagnostic(
                DiagnosticCode::CircleDocClass,
                get_lint_range(tag).unwrap_or_else(|| tag.get_range()),
                t!("Circularly inherited classes."),
            );
            return;
        }
        if !visited.insert(current.id.clone()) {
            continue;
        }
        queue.extend(semantic_model.base_type_defs(&current.type_decl_id()));
    }
}

fn get_lint_range(tag: &LuaDocTagClass) -> Option<TextRange> {
    let start = tag.get_name_token()?.get_range().start();
    let end = tag.get_supers()?.get_range().end();
    Some(TextRange::new(start, end))
}
