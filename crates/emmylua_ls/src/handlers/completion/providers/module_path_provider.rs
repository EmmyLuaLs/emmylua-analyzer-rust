//! `require("...")` module path completion.

use std::collections::HashSet;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaLiteralExpr, LuaStringToken,
};
use lsp_types::{CompletionItem, CompletionTextEdit, TextEdit};

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision, get_text_edit_range_in_string};

pub struct ModulePathProvider;

impl CompletionProvider for ModulePathProvider {
    fn name(&self) -> &'static str {
        "module_path"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        supports_provider(builder)
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        if complete_provider(builder).is_some() {
            ProviderDecision::Stop
        } else {
            ProviderDecision::NoMatch
        }
    }
}

fn supports_provider(builder: &CompletionBuilder) -> bool {
    let Some(string_token) = LuaStringToken::cast(builder.trigger_token.clone()) else {
        return false;
    };
    let Some(call_expr) = string_token
        .get_parent::<LuaLiteralExpr>()
        .and_then(|literal| literal.get_parent::<LuaCallArgList>())
        .and_then(|arg_list| arg_list.get_parent::<LuaCallExpr>())
    else {
        return false;
    };

    call_expr.is_require()
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }
    if !supports_provider(builder) {
        return None;
    }

    let string_token = LuaStringToken::cast(builder.trigger_token.clone())?;
    let text_edit_range = get_text_edit_range_in_string(builder, string_token.clone());
    add_modules(builder, &string_token.get_value(), text_edit_range);
    Some(())
}

/// Enumerate direct child segments of workspace modules by prefix (module / folder).
pub fn add_modules(
    builder: &mut CompletionBuilder,
    prefix_content: &str,
    text_edit_range: Option<lsp_types::Range>,
) -> Option<()> {
    let current_file = builder.semantic_model.file_id();
    let mut file_ids = builder.semantic_model.main_workspace_file_ids();
    file_ids.sort();

    // `a.b.cd` → parent path `a.b` + current segment prefix `cd`.
    let parts: Vec<&str> = prefix_content.split(['.', '/', '\\']).collect();
    let module_path = if parts.len() > 1 {
        parts[..parts.len() - 1].join(".")
    } else {
        String::new()
    };
    let segment_prefix = parts.last().copied().unwrap_or("");

    let typed_parent = prefix_content
        .rfind(['.', '/', '\\'])
        .map(|i| &prefix_content[..=i])
        .unwrap_or("");
    let version = builder.get_emmyrc().runtime.version.to_lua_version_number();
    let mut seen = HashSet::new();
    let mut completions = Vec::new();
    for file_id in file_ids {
        if file_id == current_file {
            continue;
        }
        for module_name in builder.semantic_model.module_names_of(file_id) {
            let relative = if module_path.is_empty() {
                module_name.as_str()
            } else if let Some(rest) = module_name.strip_prefix(&format!("{}.", module_path)) {
                rest
            } else {
                continue;
            };
            let (segment, is_file) = match relative.find('.') {
                Some(dot) => (&relative[..dot], false),
                None => (relative, true),
            };
            if !segment.starts_with(segment_prefix) {
                continue;
            }
            let filter_text = format!("{typed_parent}{segment}");
            let detail = if is_file {
                // Resolve the full candidate in the caller's root, not whichever
                // file happened to be enumerated first. Skip unavailable candidates.
                let Some(target) = builder.semantic_model.module_file_of(&filter_text) else {
                    continue;
                };
                if target == current_file {
                    continue;
                }
                let Some(info) = builder.semantic_model.module_info_of(target) else {
                    continue;
                };
                if !info.is_visible(&version) {
                    continue;
                }
                let Some(uri) = builder.semantic_model.file_uri_of(target) else {
                    continue;
                };
                Some(uri.to_string())
            } else {
                None
            };
            if !seen.insert(segment.to_string()) {
                continue;
            }
            let text_edit = text_edit_range.map(|range| {
                CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: filter_text.clone(),
                })
            });
            completions.push(CompletionItem {
                label: segment.to_string(),
                kind: Some(if is_file {
                    lsp_types::CompletionItemKind::FILE
                } else {
                    lsp_types::CompletionItemKind::FOLDER
                }),
                filter_text: Some(filter_text),
                text_edit,
                detail,
                ..Default::default()
            });
        }
    }
    completions.sort_by(|a, b| a.label.cmp(&b.label));
    for completion_item in completions {
        builder.add_completion_item(completion_item)?;
    }
    Some(())
}
