use emmylua_code_analysis::LuaTypeDeclId;
use emmylua_parser::{LuaAstNode, LuaDocAttributeUse, LuaDocNameType, LuaSyntaxKind, LuaTokenKind};
use lsp_types::CompletionItem;
use std::collections::HashSet;

use crate::handlers::completion::{
    completion_builder::CompletionBuilder, completion_data::CompletionData,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let completion_type = check_can_add_type_completion(builder)?;

    let prefix_content = builder.trigger_token.text().to_string();
    let prefix = if let Some(last_sep) = prefix_content.rfind('.') {
        let (path, _) = prefix_content.split_at(last_sep + 1);
        path
    } else {
        ""
    };
    complete_types_by_prefix(builder, prefix, None, Some(completion_type));
    builder.stop_here();
    Some(())
}

pub fn complete_types_by_prefix(
    builder: &mut CompletionBuilder,
    prefix: &str,
    filter: Option<&HashSet<LuaTypeDeclId>>,
    completion_type: Option<CompletionType>,
) -> Option<()> {
    let completion_type = completion_type.or(Some(CompletionType::Type))?;
    let file_id = builder.semantic_model.get_file_id();
    let type_index = builder.semantic_model.get_db().get_type_index();
    let results = type_index.find_type_decls(file_id, prefix);

    for (name, type_decl) in results {
        if let Some(filter) = filter
            && type_decl
                .as_ref()
                .is_some_and(|type_decl| filter.contains(type_decl))
        {
            continue;
        }
        match completion_type {
            CompletionType::AttributeUse => {
                if let Some(decl_id) = type_decl {
                    let type_decl = builder
                        .semantic_model
                        .get_db()
                        .get_type_index()
                        .get_type_decl(&decl_id)?;
                    if type_decl.is_attribute() {
                        add_type_completion_item(builder, &name, Some(decl_id));
                    }
                }
            }
            CompletionType::Type => {
                if let Some(decl_id) = &type_decl {
                    let type_decl = builder
                        .semantic_model
                        .get_db()
                        .get_type_index()
                        .get_type_decl(decl_id)?;
                    if type_decl.is_attribute() {
                        continue;
                    }
                }
                add_type_completion_item(builder, &name, type_decl);
            }
        }
    }

    Some(())
}

pub enum CompletionType {
    Type,
    AttributeUse,
}

fn check_can_add_type_completion(builder: &CompletionBuilder) -> Option<CompletionType> {
    match builder.trigger_token.kind().into() {
        LuaTokenKind::TkName => {
            let parent = builder.trigger_token.parent()?;
            if let Some(doc_name) = LuaDocNameType::cast(parent) {
                if doc_name.get_parent::<LuaDocAttributeUse>().is_some() {
                    return Some(CompletionType::AttributeUse);
                }
                return Some(CompletionType::Type);
            }

            None
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

fn add_type_completion_item(
    builder: &mut CompletionBuilder,
    name: &str,
    type_decl: Option<LuaTypeDeclId>,
) -> Option<()> {
    let kind = match type_decl {
        Some(_) => lsp_types::CompletionItemKind::CLASS,
        None => lsp_types::CompletionItemKind::MODULE,
    };

    let data = if let Some(id) = type_decl {
        CompletionData::from_property_owner_id(builder, id.into(), None)
    } else {
        None
    };

    let completion_item = CompletionItem {
        label: name.to_string(),
        kind: Some(kind),
        data,
        ..CompletionItem::default()
    };

    builder.add_completion_item(completion_item)
}
