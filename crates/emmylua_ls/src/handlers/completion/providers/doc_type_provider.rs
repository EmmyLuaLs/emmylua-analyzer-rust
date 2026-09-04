//! Doc type completion: type names in `---@type` / `@param` / `@field` etc.
//!
//! The salsa version currently uses the full `FileExports::types` set with prefix/path filtering;
//! attribute contexts only suggest known built-in attribute classes like `constructor`
//! (Attribute class inheritance detection is a later step).

use std::collections::HashSet;

use emmylua_code_analysis::TypeDefKind;
use emmylua_parser::{LuaAstNode, LuaDocAttributeUse, LuaDocNameType, LuaSyntaxKind, LuaTokenKind};
use lsp_types::{CompletionItem, CompletionItemKind};

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision};

pub struct DocTypeProvider;

impl CompletionProvider for DocTypeProvider {
    fn name(&self) -> &'static str {
        "doc_type"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        completion_type_for(builder).is_some()
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        if complete_provider(builder).is_some() {
            ProviderDecision::Stop
        } else {
            ProviderDecision::NoMatch
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionType {
    Type,
    AttributeUse,
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let completion_type = completion_type_for(builder)?;
    let prefix_content = builder.trigger_token.text().to_string();
    let prefix = if let Some(last_sep) = prefix_content.rfind('.') {
        let (path, _) = prefix_content.split_at(last_sep + 1);
        path
    } else {
        ""
    };
    complete_types_by_prefix(builder, prefix, completion_type);
    Some(())
}

pub fn complete_types_by_prefix(
    builder: &mut CompletionBuilder,
    prefix: &str,
    completion_type: CompletionType,
) {
    let partial = builder.partial_name();
    let mut seen = HashSet::new();
    let mut file_ids = builder.semantic_model.main_workspace_file_ids();
    file_ids.sort();
    for file_id in file_ids {
        let Some(model) = builder.semantic_model.model_for(file_id) else {
            continue;
        };
        let Some(exports) = model.file_exports_current() else {
            continue;
        };
        for def in exports.types.iter() {
            if def.flags.meta {
                continue;
            }
            let full_name = def.full_name.as_str();
            // Prefix path matching: `ns.` only accepts types under `ns`.
            if !full_name.starts_with(prefix) {
                continue;
            }
            let relative = &full_name[prefix.len()..];
            if !partial.is_empty() && !relative.starts_with(&partial) {
                continue;
            }
            if completion_type == CompletionType::AttributeUse && def.kind != TypeDefKind::Class {
                continue;
            }
            if !seen.insert(full_name.to_string()) {
                continue;
            }
            let kind = match completion_type {
                CompletionType::AttributeUse => CompletionItemKind::CLASS,
                CompletionType::Type => match def.kind {
                    TypeDefKind::Enum => CompletionItemKind::ENUM,
                    TypeDefKind::Class => CompletionItemKind::CLASS,
                    TypeDefKind::Alias => CompletionItemKind::STRUCT,
                },
            };
            builder.add_completion_item(CompletionItem {
                label: relative.to_string(),
                kind: Some(kind),
                ..Default::default()
            });
        }
    }
}

fn completion_type_for(builder: &CompletionBuilder) -> Option<CompletionType> {
    match builder.trigger_token.kind().into() {
        LuaTokenKind::TkName => {
            let parent = builder.trigger_token.parent()?;
            let doc_name = LuaDocNameType::cast(parent)?;
            if doc_name.get_parent::<LuaDocAttributeUse>().is_some() {
                return Some(CompletionType::AttributeUse);
            }
            Some(CompletionType::Type)
        }
        LuaTokenKind::TkWhitespace => {
            let left_token = builder.trigger_token.prev_token()?;
            match left_token.kind().into() {
                LuaTokenKind::TkTagReturn | LuaTokenKind::TkTagType => {
                    return Some(CompletionType::Type);
                }
                LuaTokenKind::TkName => {
                    let parent = left_token.parent()?;
                    match parent.kind().into() {
                        LuaSyntaxKind::DocTagParam
                        | LuaSyntaxKind::DocTagField
                        | LuaSyntaxKind::DocTagAlias
                        | LuaSyntaxKind::DocTagCast => return Some(CompletionType::Type),
                        _ => {}
                    }
                }
                LuaTokenKind::TkComma | LuaTokenKind::TkDocOr => {
                    let parent = left_token.parent()?;
                    if parent.kind() == LuaSyntaxKind::DocTypeList.into() {
                        return Some(CompletionType::Type);
                    }
                }
                LuaTokenKind::TkColon => {
                    let parent = left_token.parent()?;
                    if parent.kind() == LuaSyntaxKind::DocTagClass.into() {
                        return Some(CompletionType::Type);
                    }
                }
                _ => {}
            }
            None
        }
        LuaTokenKind::TkDocAttributeUse => Some(CompletionType::AttributeUse),
        _ => None,
    }
}
