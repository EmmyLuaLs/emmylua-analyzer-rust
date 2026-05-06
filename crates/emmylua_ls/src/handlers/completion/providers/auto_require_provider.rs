use emmylua_code_analysis::{
    CompilationModuleInfo, EmmyrcFilenameConvention, LuaSemanticDeclId, LuaType,
    SalsaSemanticTargetSummary, check_module_visibility, find_compilation_module_by_file_id,
    resolve_projected_module_export_type,
};
use emmylua_parser::{LuaAstNode, LuaNameExpr};
use lsp_types::{CompletionItem, Position};

use crate::{
    handlers::{
        command::make_auto_require,
        completion::{
            add_completions::get_completion_kind, completion_builder::CompletionBuilder,
            completion_data::CompletionData,
        },
    },
    util::{file_name_convert, module_name_convert},
};

use super::{CompletionProvider, ProviderDecision};

pub struct AutoRequireProvider;

impl CompletionProvider for AutoRequireProvider {
    fn name(&self) -> &'static str {
        "auto_require"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        builder.semantic_model.get_emmyrc().completion.auto_require
            && builder
                .trigger_token
                .parent()
                .and_then(LuaNameExpr::cast)
                .is_some()
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        if complete_provider(builder).is_some() {
            ProviderDecision::Continue
        } else {
            ProviderDecision::NoMatch
        }
    }
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<()> {
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
    let file_id = builder.semantic_model.get_file_id();
    let module_index = builder.semantic_model.get_db().get_module_index();
    let range = builder.trigger_token.text_range();
    let document = builder.semantic_model.get_document();
    let lsp_position = document.to_lsp_range(range)?.start;

    let mut completions = Vec::new();
    for legacy_module in module_index.get_module_infos() {
        if legacy_module.file_id == file_id
            || !legacy_module.is_visible(&emmyrc.runtime.version.to_lua_version_number())
        {
            continue;
        }

        let Some(module_info) = find_compilation_module_by_file_id(
            builder.semantic_model.get_db(),
            legacy_module.file_id,
        ) else {
            continue;
        };

        if !module_info.has_export_type() || module_index.is_std(&module_info.file_id) {
            continue;
        }

        add_module_completion_item(
            builder,
            &prefix,
            &module_info,
            file_conversion,
            lsp_position,
            &mut completions,
        );
    }

    for completion in completions {
        builder.add_completion_item(completion);
    }

    Some(())
}

fn add_module_completion_item(
    builder: &CompletionBuilder,
    prefix: &str,
    module_info: &CompilationModuleInfo,
    file_conversion: EmmyrcFilenameConvention,
    position: Position,
    completions: &mut Vec<CompletionItem>,
) -> Option<()> {
    if !check_module_visibility(&builder.semantic_model, module_info).unwrap_or(false) {
        return None;
    }

    let export_type =
        resolve_projected_module_export_type(builder.semantic_model.get_db(), module_info.file_id);
    let completion_name = module_name_convert(module_info, export_type.as_ref(), file_conversion);
    if !completion_name.to_lowercase().starts_with(prefix) {
        // 如果模块名不匹配, 则根据导出类型添加完成项
        add_completion_item_by_type(
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

    let data = module_info
        .semantic_target
        .as_ref()
        .and_then(|target| match target {
            SalsaSemanticTargetSummary::Decl(decl_id) => Some(LuaSemanticDeclId::LuaDecl(
                emmylua_code_analysis::LuaDeclId::new(module_info.file_id, decl_id.as_position()),
            )),
            SalsaSemanticTargetSummary::Signature(_) => None,
            SalsaSemanticTargetSummary::Member(_) => None,
        })
        .and_then(|property_id| CompletionData::from_property_owner_id(builder, property_id, None));
    let completion_item = CompletionItem {
        label: completion_name.clone(),
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
            completion_name,
            None,
        )),
        data,
        ..Default::default()
    };

    completions.push(completion_item);

    Some(())
}

fn add_completion_item_by_type(
    builder: &CompletionBuilder,
    prefix: &str,
    module_info: &CompilationModuleInfo,
    file_conversion: EmmyrcFilenameConvention,
    position: Position,
    completions: &mut Vec<CompletionItem>,
) -> Option<()> {
    if let Some(export_type) =
        resolve_projected_module_export_type(builder.semantic_model.get_db(), module_info.file_id)
    {
        match export_type {
            LuaType::TableConst(_) | LuaType::Def(_) => {
                let member_infos = builder.semantic_model.get_member_infos(&export_type)?;
                for member_info in member_infos {
                    let key_name = file_name_convert(
                        &member_info.key.to_path(),
                        &member_info.typ,
                        file_conversion,
                    );
                    match member_info.typ {
                        LuaType::Def(_) => {}
                        LuaType::Signature(_) => {}
                        LuaType::DocFunction(_) => {}
                        LuaType::Ref(_) => {}
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
                            label: key_name.clone(),
                            kind: Some(get_completion_kind(&member_info.typ)),
                            label_details: Some(lsp_types::CompletionItemLabelDetails {
                                detail: Some(format!("    (in {})", module_info.full_module_name)),
                                ..Default::default()
                            }),
                            command: Some(make_auto_require(
                                "",
                                builder.semantic_model.get_file_id(),
                                module_info.file_id,
                                position,
                                key_name,
                                Some(member_info.key.to_path().to_string()),
                            )),
                            data,
                            ..Default::default()
                        };

                        completions.push(completion_item);
                    }
                }
            }
            LuaType::Signature(_) => {
                let semantic_id = match module_info.semantic_target.as_ref()? {
                    SalsaSemanticTargetSummary::Decl(decl_id) => {
                        LuaSemanticDeclId::LuaDecl(emmylua_code_analysis::LuaDeclId::new(
                            module_info.file_id,
                            decl_id.as_position(),
                        ))
                    }
                    _ => return None,
                };
                if let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_id {
                    let decl = builder
                        .semantic_model
                        .get_db()
                        .get_decl_index()
                        .get_decl(&decl_id)?;
                    let name = decl.get_name();
                    if name.to_lowercase().starts_with(prefix) {
                        if builder.env_duplicate_name.contains(name) {
                            return None;
                        }

                        let completion_item = CompletionItem {
                            label: name.to_string(),
                            kind: Some(get_completion_kind(&export_type)),
                            label_details: Some(lsp_types::CompletionItemLabelDetails {
                                detail: Some(format!("    (in {})", module_info.full_module_name)),
                                ..Default::default()
                            }),
                            command: Some(make_auto_require(
                                "",
                                builder.semantic_model.get_file_id(),
                                module_info.file_id,
                                position,
                                name.to_string(),
                                None,
                            )),
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
