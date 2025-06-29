use emmylua_code_analysis::{DbIndex, LuaCompilation, SemanticModel};
use lsp_types::{CompletionItem, Documentation, MarkedString, MarkupContent};

use crate::{
    context::ClientId,
    handlers::hover::{build_hover_content_for_completion, HoverBuilder},
};

use super::completion_data::{CompletionData, CompletionDataType};

pub fn resolve_completion(
    compilation: &LuaCompilation,
    semantic_model: &SemanticModel,
    db: &DbIndex,
    completion_item: &mut CompletionItem,
    completion_data: CompletionData,
    client_id: ClientId,
) -> Option<()> {
    // todo: resolve completion
    match completion_data.typ {
        CompletionDataType::PropertyOwnerId(property_id) => {
            let hover_builder =
                build_hover_content_for_completion(compilation, semantic_model, db, property_id);
            if let Some(mut hover_builder) = hover_builder {
                update_function_signature_info(&mut hover_builder, completion_data.overload_count);
                if client_id.is_vscode() {
                    build_vscode_completion_item(completion_item, hover_builder, None);
                } else {
                    build_other_completion_item(completion_item, hover_builder, None);
                }
            }
        }
        CompletionDataType::Overload((property_id, index)) => {
            let hover_builder =
                build_hover_content_for_completion(compilation, semantic_model, db, property_id);
            if let Some(mut hover_builder) = hover_builder {
                update_function_signature_info(&mut hover_builder, completion_data.overload_count);
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

pub fn update_function_signature_info(
    hover_builder: &mut HoverBuilder,
    overload_count: Option<usize>,
) {
    if let Some(overload_count) = overload_count {
        if overload_count > 0 {
            if let Some(signature_overload) = &mut hover_builder.signature_overload {
                for signature in signature_overload.iter_mut() {
                    if let MarkedString::LanguageString(s) = signature {
                        s.value = format!("{} (+{} overloads)", s.value, overload_count);
                    }
                }
            }
            if let MarkedString::LanguageString(s) = &mut hover_builder.type_description {
                s.value = format!("{} (+{} overloads)", s.value, overload_count);
            }
        }
    }
}

fn build_vscode_completion_item(
    completion_item: &mut CompletionItem,
    hover_builder: HoverBuilder,
    overload_index: Option<usize>,
) -> Option<()> {
    let type_description = overload_index
        .and_then(|index| {
            hover_builder
                .signature_overload
                .and_then(|overloads| overloads.get(index).cloned())
        })
        .unwrap_or_else(|| hover_builder.type_description.clone());

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

    let type_description = overload_index
        .and_then(|index| {
            hover_builder
                .signature_overload
                .and_then(|overloads| overloads.get(index).cloned())
        })
        .unwrap_or_else(|| hover_builder.type_description.clone());

    match type_description {
        MarkedString::String(s) => {
            result.push_str(&format!("\n{}\n", s));
        }
        MarkedString::LanguageString(s) => {
            result.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
        }
    }
    if let Some(location_path) = hover_builder.location_path {
        match location_path {
            MarkedString::String(s) => {
                result.push_str(&format!("\n{}\n", s));
            }
            _ => {}
        }
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
