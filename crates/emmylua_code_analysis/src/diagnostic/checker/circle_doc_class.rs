//! Circle doc class checker — pure salsa.

use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaDocTagClass};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for tag in root.descendants::<LuaDocTagClass>() {
        check_class(context, model, &tag);
    }
}

fn check_class(context: &mut DiagnosticContext, model: &SemanticModel, tag: &LuaDocTagClass) {
    let Some(name) = tag.get_name_token().map(|t| t.get_name_text().to_string()) else {
        return;
    };
    let db = model.salsa_db();
    let Some(class_def) = db.doc().type_def_by_name(model.get_file_id(), &name) else {
        return;
    };
    if !matches!(
        class_def.kind,
        crate::compilation::SalsaDocTypeDefKindSummary::Class
    ) {
        return;
    }

    // BFS to detect circular inheritance through super_type_offsets
    let mut visited = HashSet::new();
    let mut queue: Vec<String> = vec![name.clone()];
    while let Some(cur) = queue.pop() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        if let Some(def) = db.doc().type_def_by_name(model.get_file_id(), &cur) {
            for offset in &def.super_type_offsets {
                if let Some(resolved) = db.doc().resolved_type_by_key(model.get_file_id(), *offset)
                {
                    if let crate::compilation::SalsaDocTypeLoweredKind::Name { name: super_name } =
                        &resolved.lowered.kind
                    {
                        if super_name.as_str() == name {
                            let range = get_lint_range(tag).unwrap_or(tag.get_range());
                            context.add_diagnostic(
                                DiagnosticCode::CircleDocClass,
                                range,
                                t!("Circularly inherited classes.").to_string(),
                                None,
                            );
                            return;
                        }
                        queue.push(super_name.to_string());
                    }
                }
            }
        }
    }
}

fn get_lint_range(tag: &LuaDocTagClass) -> Option<TextRange> {
    let start = tag.get_name_token()?.get_range().start();
    let end = tag.get_supers()?.get_range().end();
    Some(TextRange::new(start, end))
}
