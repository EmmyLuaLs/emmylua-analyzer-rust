//! circle_doc_class: checks for circular `---@class` inheritance.
//!
//! Mirrors the old `diagnostic::checker::circle_doc_class`: BFS the parent-type chain from each class,
//! and when it reaches itself -> report Circularly inherited classes (range: from the name to the end of supers).

use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaDocTagClass};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::salsa_builder::def::TypeDefKind;
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
    let name = name_token.get_name_text();
    let Some(facts) = semantic_model.file_facts() else {
        return;
    };
    let Some(class_def) = facts.type_def_by_name(&name) else {
        return;
    };
    if class_def.kind != TypeDefKind::Class {
        return;
    }
    let class_id = class_def.id.clone();
    let mut queue = vec![class_def.clone()];
    let mut visited = HashSet::new();
    while let Some(current) = queue.pop() {
        if !visited.insert(current.id.clone()) {
            continue;
        }
        for super_name in &current.super_names {
            let Some(super_def) = semantic_model.resolve_type_def(super_name) else {
                continue;
            };
            if super_def.id == class_id {
                context.add_diagnostic(
                    DiagnosticCode::CircleDocClass,
                    get_lint_range(tag).unwrap_or_else(|| tag.get_range()),
                    t!("Circularly inherited classes."),
                );
                return;
            }
            if !visited.contains(&super_def.id) {
                queue.push(super_def);
            }
        }
    }
}

fn get_lint_range(tag: &LuaDocTagClass) -> Option<TextRange> {
    let start = tag.get_name_token()?.get_range().start();
    let end = tag.get_supers()?.get_range().end();
    Some(TextRange::new(start, end))
}
