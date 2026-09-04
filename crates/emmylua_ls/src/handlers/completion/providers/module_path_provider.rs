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

    let mut seen = HashSet::new();
    let mut completions = Vec::new();
    for file_id in file_ids {
        if file_id == current_file {
            continue;
        }
        // let Some(model) = SalsaSemanticModel::new(db, file_id) else {
        //     continue;
        // };
        let Some(module_name) = builder.semantic_model.module_name_of(file_id) else {
            continue;
        };
        if module_name == module_path {
            continue;
        }
        let relative = if module_path.is_empty() {
            module_name.as_str()
        } else if let Some(rest) = module_name.strip_prefix(&format!("{}.", module_path)) {
            rest
        } else {
            continue;
        };
        // Only suggest direct child segments; intermediate segments appear as folders derived from other submodule paths.
        let (segment, is_file) = match relative.find('.') {
            Some(dot) => (&relative[..dot], false),
            None => (relative, true),
        };
        if !segment_prefix.is_empty() && !segment.starts_with(segment_prefix) {
            continue;
        }
        if !seen.insert(segment.to_string()) {
            continue;
        }

        let filter_text = if module_path.is_empty() {
            segment.to_string()
        } else {
            format!("{}.{}", module_path, segment)
        };
        let text_edit = text_edit_range.map(|text_edit_range| {
            CompletionTextEdit::Edit(TextEdit {
                range: text_edit_range,
                new_text: filter_text.clone(),
            })
        });
        let kind = if is_file {
            lsp_types::CompletionItemKind::FILE
        } else {
            lsp_types::CompletionItemKind::FOLDER
        };
        let detail = is_file
            .then(|| {
                builder
                    .semantic_model
                    .file_uri_of(file_id)
                    .map(|uri| uri.to_string())
            })
            .flatten();
        completions.push(CompletionItem {
            label: segment.to_string(),
            kind: Some(kind),
            filter_text: Some(filter_text),
            text_edit,
            detail,
            ..Default::default()
        });
    }
    completions.sort_by(|a, b| a.label.cmp(&b.label));
    for completion_item in completions {
        builder.add_completion_item(completion_item)?;
    }
    Some(())
}
