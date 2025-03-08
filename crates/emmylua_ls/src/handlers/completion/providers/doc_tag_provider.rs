use crate::meta_text::meta_doc_tag;
use emmylua_parser::LuaTokenKind;
use lsp_types::{CompletionItem, MarkupContent};

use crate::handlers::completion::{completion_builder::CompletionBuilder, data::DOC_TAGS};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let trigger_token = &builder.trigger_token;
    if !matches!(
        trigger_token.kind().into(),
        LuaTokenKind::TkDocStart | LuaTokenKind::TkDocLongStart | LuaTokenKind::TkTagOther
    ) {
        return None;
    }

    for (sorted_index, tag) in DOC_TAGS.iter().enumerate() {
        add_tag_completion(builder, sorted_index, tag);
    }

    builder.stop_here();

    Some(())
}

fn add_tag_completion(builder: &mut CompletionBuilder, sorted_index: usize, tag: &str) {
    let completion_item = CompletionItem {
        label: tag.to_string(),
        kind: Some(lsp_types::CompletionItemKind::EVENT),
        documentation: Some(lsp_types::Documentation::MarkupContent(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: meta_doc_tag(tag),
        })),
        sort_text: Some(format!("{:03}", sorted_index)),
        ..Default::default()
    };

    builder.add_completion_item(completion_item);
}
