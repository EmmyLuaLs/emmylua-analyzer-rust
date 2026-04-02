use crate::handlers::completion::{
    completion_builder::CompletionBuilder, completion_data::CompletionData,
};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaLiteralExpr, LuaStringToken,
};
use lsp_types::{CompletionItem, CompletionTextEdit, TextEdit};

use super::get_text_edit_range_in_string;

use super::{CompletionProvider, ProviderDecision};

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

    let text_edit_range = get_text_edit_range_in_string(builder, string_token.clone())?;
    add_modules(builder, &string_token.get_value(), Some(text_edit_range));
    Some(())
}

pub fn add_modules(
    builder: &mut CompletionBuilder,
    prefix_content: &str,
    text_edit_range: Option<lsp_types::Range>,
) -> Option<()> {
    let version_number = builder
        .semantic_model
        .get_emmyrc()
        .runtime
        .version
        .to_lua_version_number();
    let parts: Vec<&str> = prefix_content.split(['.', '/', '\\']).collect();
    let module_path = if parts.len() > 1 {
        parts[..parts.len() - 1].join(".")
    } else {
        "".to_string()
    };

    let prefix = if let Some(last_sep) = prefix_content.rfind(['/', '\\', '.']) {
        let (path, _) = prefix_content.split_at(last_sep + 1);
        path
    } else {
        ""
    };

    let db = builder.semantic_model.get_db();
    let mut module_completions = Vec::new();
    let module_info = db.get_module_index().find_module_node(&module_path)?;
    for (name, module_id) in &module_info.children {
        let child_module_node = db.get_module_index().get_module_node(module_id)?;
        let filter_text = format!("{}{}", prefix, name);
        let text_edit = text_edit_range.map(|text_edit_range| {
            CompletionTextEdit::Edit(TextEdit {
                range: text_edit_range,
                new_text: filter_text.clone(),
            })
        });
        if let Some(child_file_id) = child_module_node.file_ids.first() {
            let child_module_info = db.get_module_index().get_module(*child_file_id)?;
            let data = if let Some(property_id) = &child_module_info.semantic_id {
                CompletionData::from_property_owner_id(builder, property_id.clone(), None)
            } else {
                None
            };

            if child_module_info.is_visible(&version_number) {
                let uri = db.get_vfs().get_uri(child_file_id)?;
                let completion_item = CompletionItem {
                    label: name.clone(),
                    kind: Some(lsp_types::CompletionItemKind::FILE),
                    filter_text: Some(filter_text),
                    text_edit,
                    detail: Some(uri.to_string()),
                    data,
                    ..Default::default()
                };
                module_completions.push(completion_item);
            }
        } else {
            let completion_item = CompletionItem {
                label: name.clone(),
                kind: Some(lsp_types::CompletionItemKind::FOLDER),
                filter_text: Some(filter_text),
                text_edit,
                ..Default::default()
            };

            module_completions.push(completion_item);
        }
    }

    let _ = module_info;
    for completion_item in module_completions {
        builder.add_completion_item(completion_item)?;
    }

    Some(())
}
