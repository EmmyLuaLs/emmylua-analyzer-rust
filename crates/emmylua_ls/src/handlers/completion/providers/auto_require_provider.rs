use emmylua_code_analysis::{
    check_export_visibility, EmmyrcFilenameConvention, LuaType, ModuleInfo,
};
use emmylua_parser::{LuaAstNode, LuaNameExpr};
use lsp_types::{CompletionItem, Position};

use crate::{
    handlers::{
        command::make_auto_require,
        completion::{completion_builder::CompletionBuilder, completion_data::CompletionData},
    },
    util::{key_name_convert, module_name_convert},
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let enable = builder.semantic_model.get_emmyrc().completion.auto_require;
    if !enable {
        return None;
    }

    let name_expr = LuaNameExpr::cast(builder.trigger_token.parent()?)?;
    // optimize for large project
    let prefix = name_expr.get_name_text()?.to_lowercase();
    let emmyrc = builder.semantic_model.get_emmyrc();
    let file_conversion = emmyrc.completion.auto_require_naming_convention;
    let version_number = emmyrc.runtime.version.to_lua_version_number();
    let file_id = builder.semantic_model.get_file_id();
    let module_infos = builder
        .semantic_model
        .get_db()
        .get_module_index()
        .get_module_infos();
    let range = builder.trigger_token.text_range();
    let document = builder.semantic_model.get_document();
    let lsp_position = document.to_lsp_range(range)?.start;

    let mut completions = Vec::new();
    for module_info in module_infos {
        if module_info.is_visible(&version_number)
            && module_info.file_id != file_id
            && module_info.export_type.is_some()
        {
            add_module_completion_item(
                builder,
                &prefix,
                &module_info,
                file_conversion,
                lsp_position,
                &mut completions,
            );
        }
    }

    for completion in completions {
        builder.add_completion_item(completion);
    }

    Some(())
}

fn add_module_completion_item(
    builder: &CompletionBuilder,
    prefix: &str,
    module_info: &ModuleInfo,
    file_conversion: EmmyrcFilenameConvention,
    position: Position,
    completions: &mut Vec<CompletionItem>,
) -> Option<()> {
    if !check_export_visibility(&builder.semantic_model, &module_info).unwrap_or(false) {
        return None;
    }

    let completion_name = module_name_convert(module_info, file_conversion);
    if !completion_name.to_lowercase().starts_with(prefix) {
        try_add_member_completion_items(
            builder,
            prefix,
            module_info,
            file_conversion,
            position,
            completions,
        );
        return None;
    }

    if builder.env_duplicate_name.contains(&completion_name) {
        return None;
    }

    let data = if let Some(property_id) = &module_info.property_owner_id {
        CompletionData::from_property_owner_id(builder, property_id.clone(), None)
    } else {
        None
    };
    let completion_item = CompletionItem {
        label: completion_name,
        kind: Some(lsp_types::CompletionItemKind::MODULE),
        label_details: Some(lsp_types::CompletionItemLabelDetails {
            detail: Some(format!("    (in {})", module_info.full_module_name)),
            ..Default::default()
        }),
        command: Some(make_auto_require(
            "",
            builder.semantic_model.get_file_id(),
            module_info.file_id,
            position,
            None,
        )),
        data,
        ..Default::default()
    };

    completions.push(completion_item);

    Some(())
}

fn try_add_member_completion_items(
    builder: &CompletionBuilder,
    prefix: &str,
    module_info: &ModuleInfo,
    file_conversion: EmmyrcFilenameConvention,
    position: Position,
    completions: &mut Vec<CompletionItem>,
) -> Option<()> {
    if let Some(export_type) = &module_info.export_type {
        match export_type {
            LuaType::TableConst(_) | LuaType::Def(_) => {
                let member_infos = builder.semantic_model.get_member_infos(export_type)?;
                for member_info in member_infos {
                    let key_name = key_name_convert(
                        &member_info.key.to_path(),
                        &member_info.typ,
                        file_conversion,
                    );
                    match member_info.typ {
                        LuaType::Def(_) => {}
                        LuaType::Signature(_) => {}
                        _ => {
                            continue;
                        }
                    }

                    if key_name.to_lowercase().starts_with(prefix) {
                        if builder.env_duplicate_name.contains(&key_name) {
                            continue;
                        }

                        let data = if let Some(property_owner_id) = &member_info.property_owner_id {
                            let is_visible = builder.semantic_model.is_semantic_visible(
                                builder.trigger_token.clone(),
                                property_owner_id.clone(),
                            );
                            if !is_visible {
                                continue;
                            }
                            CompletionData::from_property_owner_id(
                                builder,
                                property_owner_id.clone(),
                                None,
                            )
                        } else {
                            None
                        };

                        let completion_item = CompletionItem {
                            label: key_name,
                            kind: Some(lsp_types::CompletionItemKind::MODULE),
                            label_details: Some(lsp_types::CompletionItemLabelDetails {
                                detail: Some(format!("    (in {})", module_info.full_module_name)),
                                ..Default::default()
                            }),
                            command: Some(make_auto_require(
                                "",
                                builder.semantic_model.get_file_id(),
                                module_info.file_id,
                                position,
                                Some(member_info.key.to_path().to_string()),
                            )),
                            data,
                            ..Default::default()
                        };

                        completions.push(completion_item);
                    }
                }
            }
            _ => {}
        }
    }
    Some(())
}
