use crate::{DiagnosticCode, SemanticModel};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaDocTagOther, LuaTokenKind};
use serde_json::Value;
use std::collections::HashSet;

use super::{Checker, DiagnosticContext};

pub struct UnknownDocTag;

impl Checker for UnknownDocTag {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UndefinedDocParam];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let known_tags: HashSet<_> = semantic_model
            .get_emmyrc()
            .doc
            .known_tags
            .iter()
            .map(|tag| tag.as_str())
            .collect();

        let root = semantic_model.get_root().clone();
        for tag_other in root.descendants::<LuaDocTagOther>() {
            check_tag(context, &tag_other, &known_tags);
        }
    }
}

fn check_tag(
    context: &mut DiagnosticContext,
    tag_other: &LuaDocTagOther,
    known_tags: &HashSet<&str>,
) -> Option<()> {
    if let Some(token) = tag_other.token_by_kind(LuaTokenKind::TkTagOther) {
        if !known_tags.contains(token.get_text()) {
            context.add_diagnostic(
                DiagnosticCode::UnknownDocTag,
                token.get_range(),
                t!("Unknown doc tag: `%{name}`", name = token.get_text()).to_string(),
                Some(Value::String(token.get_text().to_string())),
            );
        }
    }
    Some(())
}
