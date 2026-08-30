//! # resolve_completion — Completion item resolve (documentation augmentation)
//!
//! For Member / Decl payloads, split hover content into detail and documentation;
//! other payloads are returned unchanged. Overload expansion from the old DbIndex version
//! is currently approximated by the hover layer.

use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use lsp_types::{CompletionItem, Documentation, MarkupContent};

use super::completion_data::{CompletionData, CompletionDataType};
use crate::context::ClientId;

pub fn resolve_completion(
    analysis: &EmmyLuaAnalysis,
    mut completion_item: CompletionItem,
    completion_data: CompletionData,
    _client_id: ClientId,
) -> CompletionItem {
    let (file_id, offset) = match &completion_data.typ {
        CompletionDataType::Member { file_id, range }
        | CompletionDataType::Decl { file_id, range } => (FileId::new(*file_id), range.0),
        _ => return completion_item,
    };

    let Some(document) = analysis.salsa.document(file_id) else {
        return completion_item;
    };
    let Some(position) = document.to_lsp_position(offset.into()) else {
        return completion_item;
    };

    let Some(hover) = crate::handlers::hover::hover(analysis, file_id, position) else {
        return completion_item;
    };
    let value = match hover.contents {
        lsp_types::HoverContents::Markup(MarkupContent { value, .. }) => value,
        lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(value)) => value,
        _ => return completion_item,
    };

    // Split out the code block: detail is the Lua code body; the rest becomes documentation.
    if let Some((code, doc)) = extract_code_block(&value) {
        completion_item.detail = Some(code);
        if !doc.trim().is_empty() {
            completion_item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: doc,
            }));
        }
    } else {
        completion_item.detail = Some(value.clone());
    }

    // Backward compatibility with old resolve: documentation starts with a newline.
    if let Some(Documentation::MarkupContent(MarkupContent { value, .. })) =
        completion_item.documentation.as_mut()
        && !value.starts_with('\n')
    {
        value.insert(0, '\n');
    }

    completion_item
}

fn extract_code_block(value: &str) -> Option<(String, String)> {
    let prefix = "```lua\n";
    let rest = value.strip_prefix(prefix)?;
    let end = rest.find("\n```")?;
    let code = rest[..end].to_string();
    let mut doc = rest[end + 4..]
        .trim_start_matches('\n')
        .trim_end()
        .to_string();
    // Hover descriptions use `---` as a separator; completion resolve keeps only plain text descriptions.
    if doc.starts_with("---") {
        doc = doc
            .trim_start_matches("---")
            .trim_start_matches('\n')
            .trim_end()
            .to_string();
    }
    // Overload blocks are extra Lua code blocks and are not plain-text documentation for completion resolve.
    let doc = doc.split("```").next().unwrap_or(&doc);
    // `@param` / `@return` in hover are function signature comments; completion resolve keeps only plain text descriptions.
    let doc = doc
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("@*param*")
                || trimmed.starts_with("@*return*")
                || trimmed.starts_with("@*return_overload*")
                || trimmed.starts_with("&nbsp;&nbsp;in class")
                || trimmed == "---"
                || trimmed.starts_with("---"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let doc = doc.trim_end().to_string();
    Some((code, doc))
}
