use std::path::{Path, PathBuf};

use emmylua_code_analysis::file_path_to_uri;
use emmylua_parser::{LuaAstToken, LuaStringToken};
use lsp_types::{CompletionItem, TextEdit};

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::get_text_edit_range_in_string;

pub fn can_add_completion(builder: &CompletionBuilder) -> bool {
    let Some(string_token) = LuaStringToken::cast(builder.trigger_token.clone()) else {
        return false;
    };

    string_token.get_value().find(['/', '\\']).is_some()
}

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let string_token = LuaStringToken::cast(builder.trigger_token.clone())?;
    let maybe_file_path = string_token.get_value();
    maybe_file_path.find(['/', '\\'])?;

    let prefix = if let Some(last_sep) = maybe_file_path.rfind(['/', '\\']) {
        let (path, _) = maybe_file_path.split_at(last_sep + 1);
        path
    } else {
        ""
    };

    let resources = builder.semantic_model.get_emmyrc().resource.paths.clone();

    let suffix = prefix;
    let text_edit_range = get_text_edit_range_in_string(builder, string_token)?;

    for resource in resources {
        let path = Path::new(&resource);
        let folder = path.join(suffix);
        if folder.exists()
            && folder.is_dir()
            && let Ok(entries) = std::fs::read_dir(folder)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    add_file_path_completion(builder, &path, name, prefix, text_edit_range);
                }
            }
        }
    }

    builder.stop_here();

    Some(())
}

fn add_file_path_completion(
    builder: &mut CompletionBuilder,
    path: &PathBuf,
    name: &str,
    prefix: &str,
    text_edit_range: lsp_types::Range,
) -> Option<()> {
    let kind: lsp_types::CompletionItemKind = if path.is_dir() {
        lsp_types::CompletionItemKind::FOLDER
    } else {
        lsp_types::CompletionItemKind::FILE
    };

    let detail = file_path_to_uri(path).map(|uri| uri.to_string());

    let filter_text = format!("{}{}", prefix, name);
    let text_edit = TextEdit {
        range: text_edit_range,
        new_text: filter_text.clone(),
    };
    let completion_item = CompletionItem {
        label: name.to_string(),
        kind: Some(kind),
        filter_text: Some(filter_text),
        text_edit: Some(lsp_types::CompletionTextEdit::Edit(text_edit)),
        detail,
        ..Default::default()
    };

    builder.add_completion_item(completion_item)?;

    Some(())
}
