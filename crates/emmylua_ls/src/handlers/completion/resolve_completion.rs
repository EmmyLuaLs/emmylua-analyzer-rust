use emmylua_code_analysis::{DbIndex, SemanticModel};
use emmylua_parser::{LuaAstNode, LuaSyntaxToken};
use lsp_types::{CompletionItem, Documentation, MarkedString, MarkupContent};
use rowan::{TextSize, TokenAtOffset};

use crate::{
    context::ClientId,
    handlers::hover::{HoverBuilder, build_hover_content_for_completion},
};

use super::completion_data::{CompletionData, CompletionDataType};

pub fn resolve_completion(
    semantic_model: &SemanticModel,
    db: &DbIndex,
    completion_item: &mut CompletionItem,
    completion_data: CompletionData,
    client_id: ClientId,
) -> Option<()> {
    let trigger_token =
        get_completion_trigger_token(semantic_model, completion_data.trigger_offset);

    // todo: resolve completion
    match completion_data.typ {
        CompletionDataType::PropertyOwnerId(property_id) => {
            let hover_builder = build_hover_content_for_completion(
                semantic_model,
                db,
                property_id,
                trigger_token.clone(),
            );
            if let Some(hover_builder) = hover_builder {
                if client_id.is_vscode() {
                    build_vscode_completion_item(completion_item, hover_builder, None);
                } else {
                    build_other_completion_item(completion_item, hover_builder, None);
                }
            }
        }
        CompletionDataType::Overload((property_id, index)) => {
            let hover_builder = build_hover_content_for_completion(
                semantic_model,
                db,
                property_id,
                trigger_token.clone(),
            );
            if let Some(hover_builder) = hover_builder {
                if client_id.is_vscode() {
                    build_vscode_completion_item(completion_item, hover_builder, Some(index));
                } else {
                    build_other_completion_item(completion_item, hover_builder, Some(index));
                }
            }
        }
        _ => {}
    }
    Some(())
}

fn get_completion_trigger_token(
    semantic_model: &SemanticModel,
    trigger_offset: Option<u32>,
) -> Option<LuaSyntaxToken> {
    let offset = TextSize::from(trigger_offset?);
    let root = semantic_model.get_root();
    if offset > root.syntax().text_range().end() {
        return None;
    }

    match root.syntax().token_at_offset(offset) {
        TokenAtOffset::Single(token) => Some(token),
        TokenAtOffset::Between(left, _) => Some(left),
        TokenAtOffset::None => None,
    }
}

fn build_vscode_completion_item(
    completion_item: &mut CompletionItem,
    hover_builder: HoverBuilder,
    overload_index: Option<usize>,
) -> Option<()> {
    let (type_description, overload_comment) = overload_index
        .and_then(|index| {
            hover_builder
                .signature_overload
                .as_ref()
                .and_then(|overloads| overloads.get(index).cloned())
                .map(|overload| (overload.signature, overload.comment))
        })
        .unwrap_or_else(|| (hover_builder.primary.clone(), None));

    match type_description {
        MarkedString::String(s) => {
            completion_item.detail = Some(s);
        }
        MarkedString::LanguageString(s) => {
            completion_item.detail = Some(s.value);
        }
    }

    let documentation = {
        let mut result = String::new();
        let mut first_line = true;
        if let Some(comment) = overload_comment {
            result.push_str(&format!("\n{}\n", comment));
        }
        for description in hover_builder.annotation_description {
            match description {
                MarkedString::String(s) => {
                    if first_line && s == "---" {
                        first_line = false;
                    } else {
                        result.push_str(&format!("\n{}\n", s));
                    }
                }
                MarkedString::LanguageString(s) => {
                    result.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
                }
            }
        }

        if let Some(type_expansion) = hover_builder.type_expansion {
            for type_expansion in type_expansion {
                result.push_str(&format!("\n```{}\n{}\n```\n", "lua", type_expansion));
            }
        }

        result.trim_end().to_string()
    };

    if !documentation.is_empty() {
        completion_item.documentation = Some(Documentation::MarkupContent(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: documentation,
        }));
    }
    Some(())
}

fn build_other_completion_item(
    completion_item: &mut CompletionItem,
    hover_builder: HoverBuilder,
    overload_index: Option<usize>,
) -> Option<()> {
    let mut result = String::new();

    let (type_description, overload_comment) = overload_index
        .and_then(|index| {
            hover_builder
                .signature_overload
                .as_ref()
                .and_then(|overloads| overloads.get(index).cloned())
                .map(|overload| (overload.signature, overload.comment))
        })
        .unwrap_or_else(|| (hover_builder.primary.clone(), None));

    match type_description {
        MarkedString::String(s) => {
            result.push_str(&format!("\n{}\n", s));
        }
        MarkedString::LanguageString(s) => {
            result.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
        }
    }
    if let Some(comment) = overload_comment {
        result.push_str(&format!("\n{}\n", comment));
    }
    if let Some(MarkedString::String(s)) = hover_builder.location_path {
        result.push_str(&format!("\n{}\n", s));
    }
    for marked_string in hover_builder.annotation_description {
        match marked_string {
            MarkedString::String(s) => {
                result.push_str(&format!("\n{}\n", s));
            }
            MarkedString::LanguageString(s) => {
                result.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
            }
        }
    }

    if let Some(type_expansion) = hover_builder.type_expansion {
        for type_expansion in type_expansion {
            result.push_str(&format!("\n```{}\n{}\n```\n", "lua", type_expansion));
        }
    }

    completion_item.documentation = Some(Documentation::MarkupContent(MarkupContent {
        kind: lsp_types::MarkupKind::Markdown,
        value: result,
    }));
    Some(())
}
