//! Unknown doc tag checker — salsa-native.

use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaDocTagOther, LuaTokenKind};

use crate::semantic_model::SemanticModel;
use crate::DiagnosticCode;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let known: HashSet<&str> = model
        .get_emmyrc()
        .doc
        .known_tags
        .iter()
        .map(|t| t.as_str())
        .collect();

    let root = model.get_root().clone();
    for tag in root.descendants::<LuaDocTagOther>() {
        if let Some(tk) = tag.token_by_kind(LuaTokenKind::TkTagOther) {
            if !known.contains(tk.get_text()) {
                context.add_diagnostic(
                    DiagnosticCode::UnknownDocTag,
                    tk.get_range(),
                    t!("Unknown doc tag: `%{name}`", name = tk.get_text()).to_string(),
                    Some(serde_json::Value::String(tk.get_text().to_string())),
                );
            }
        }
    }
}
