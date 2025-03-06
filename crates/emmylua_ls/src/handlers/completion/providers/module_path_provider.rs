use emmylua_code_analysis::Emmyrc;
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaExpr, LuaLiteralExpr, LuaStringToken,
};
use lsp_types::{CompletionItem, CompletionTextEdit, TextEdit};
use rowan::TextRange;

use crate::handlers::completion::completion_builder::CompletionBuilder;

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let string_token = LuaStringToken::cast(builder.trigger_token.clone())?;
    let call_expr_prefix = string_token
        .get_parent::<LuaLiteralExpr>()?
        .get_parent::<LuaCallArgList>()?
        .get_parent::<LuaCallExpr>()?
        .get_prefix_expr()?;

    let emmyrc = builder.semantic_model.get_emmyrc();
    match call_expr_prefix {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            if !is_require_call(emmyrc, &name) {
                return None;
            }
        }
        _ => return None,
    }

    let version_number = emmyrc.runtime.version.to_lua_version_number();
    let prefix_content = string_token.get_value();
    let parts: Vec<&str> = prefix_content
        .split(|c| c == '.' || c == '/' || c == '\\')
        .collect();
    let module_path = if parts.len() > 1 {
        parts[..parts.len() - 1].join(".")
    } else {
        "".to_string()
    };

    let prefix = if let Some(last_sep) = prefix_content.rfind(|c| c == '/' || c == '\\' || c == '.')
    {
        let (path, _) = prefix_content.split_at(last_sep + 1);
        path
    } else {
        ""
    };

    let text_edit_range = get_text_edit_range(builder, string_token)?;

    let db = builder.semantic_model.get_db();
    let mut module_completions = Vec::new();
    let module_info = db.get_module_index().find_module_node(&module_path)?;
    for (name, module_id) in &module_info.children {
        let child_module_node = db.get_module_index().get_module_node(module_id)?;
        let filter_text = format!("{}{}", prefix, name);
        let text_edit = TextEdit {
            range: text_edit_range.clone(),
            new_text: filter_text.clone(),
        };
        if let Some(child_file_id) = child_module_node.file_ids.first() {
            let child_module_info = db.get_module_index().get_module(*child_file_id)?;
            if child_module_info.is_visible(&version_number) {
                let uri = db.get_vfs().get_uri(child_file_id)?;
                let completion_item = CompletionItem {
                    label: name.clone(),
                    kind: Some(lsp_types::CompletionItemKind::FILE),
                    filter_text: Some(filter_text.clone()),
                    text_edit: Some(CompletionTextEdit::Edit(text_edit)),
                    detail: Some(uri.to_string()),
                    ..Default::default()
                };
                module_completions.push(completion_item);
            }
        } else {
            let completion_item = CompletionItem {
                label: name.clone(),
                kind: Some(lsp_types::CompletionItemKind::FOLDER),
                filter_text: Some(filter_text.clone()),
                text_edit: Some(CompletionTextEdit::Edit(text_edit)),
                ..Default::default()
            };

            module_completions.push(completion_item);
        }
    }

    let _ = module_info;
    for completion_item in module_completions {
        builder.add_completion_item(completion_item)?;
    }
    builder.stop_here();

    Some(())
}

fn is_require_call(emmyrc: &Emmyrc, name: &str) -> bool {
    for fun in &emmyrc.runtime.require_like_function {
        if name == fun {
            return true;
        }
    }

    name == "require"
}

fn get_text_edit_range(
    builder: &mut CompletionBuilder,
    string_token: LuaStringToken,
) -> Option<lsp_types::Range> {
    let text = string_token.get_text();
    let range = string_token.get_range();
    if text.len() == 0 {
        return None;
    }

    let mut start_offset = u32::from(range.start());
    let mut end_offset = u32::from(range.end());
    if text.starts_with('"') || text.starts_with('\'') {
        start_offset += 1;
    }

    if text.ends_with('"') || text.ends_with('\'') {
        end_offset -= 1;
    }

    let new_text_range = TextRange::new(start_offset.into(), end_offset.into());
    let lsp_range = builder
        .semantic_model
        .get_document()
        .to_lsp_range(new_text_range);

    lsp_range
}
