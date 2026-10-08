//! auto_require: workspace module surface completion (module names / exported types / members / globals).

use emmylua_code_analysis::{LuaMemberKey, LuaType, ModuleExport, TypeDefKind};
use emmylua_parser::LuaAstNode;
use lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails};

use crate::handlers::command::make_auto_require;
use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision};

pub struct AutoRequireProvider;

impl CompletionProvider for AutoRequireProvider {
    fn name(&self) -> &'static str {
        "auto_require"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        builder.get_emmyrc().completion.auto_require
            && builder
                .trigger_token
                .parent()
                .and_then(emmylua_parser::LuaNameExpr::cast)
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

    let partial = builder.partial_name();
    if partial.is_empty() {
        return None;
    }

    let current_file = builder.semantic_model.file_id();
    let mut file_ids = builder.semantic_model.main_workspace_file_ids();
    file_ids.sort();

    let document = builder.get_document();
    let position = document
        .to_lsp_range(builder.trigger_token.text_range())?
        .start;

    for file_id in file_ids {
        if file_id == current_file {
            continue;
        }
        let Some(module_name) = builder.semantic_model.module_name_of(file_id) else {
            continue;
        };
        let Some(exports) = builder.semantic_model.file_exports(file_id) else {
            continue;
        };

        let module_label = builder
            .semantic_model
            .file_path_of(file_id)
            .as_deref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| module_name.clone());

        let exported_name = match &exports.module {
            Some(ModuleExport::Decl { name, .. }) | Some(ModuleExport::Global { name }) => {
                Some(name.as_str())
            }
            _ => None,
        };

        for def in exports.types.iter() {
            let label = def.name.to_string();
            if !label.to_lowercase().starts_with(&partial.to_lowercase()) {
                continue;
            }
            let kind = if exported_name == Some(def.full_name.as_str()) {
                CompletionItemKind::MODULE
            } else {
                match def.kind {
                    TypeDefKind::Enum => CompletionItemKind::CLASS,
                    _ => CompletionItemKind::CLASS,
                }
            };
            if !builder.env_duplicate_name.insert(label.clone()) {
                continue;
            }
            let mut item = CompletionItem {
                label: label.clone(),
                kind: Some(kind),
                insert_text: None,
                detail: None,
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(format!("    (in {module_label})")),
                    description: None,
                }),
                ..Default::default()
            };
            if exports.module.is_some() {
                item.command = Some(make_auto_require(
                    "",
                    current_file,
                    file_id,
                    position,
                    label.clone(),
                    None,
                ));
            }
            builder.add_completion_item(item);
        }

        for export in &exports.members {
            let LuaMemberKey::Name(name) = &export.key else {
                continue;
            };
            let label = name.to_string();
            if !label.to_lowercase().starts_with(&partial.to_lowercase()) {
                continue;
            }
            let ty = builder
                .semantic_model
                .type_of_member(&export.member)
                .unwrap_or(LuaType::Unknown);
            let kind = if matches!(ty, LuaType::DocFunction(_) | LuaType::Function) {
                CompletionItemKind::FUNCTION
            } else {
                CompletionItemKind::FIELD
            };
            if !builder.env_duplicate_name.insert(label.clone()) {
                continue;
            }
            let mut item = CompletionItem {
                label: label.clone(),
                kind: Some(kind),
                insert_text: None,
                detail: None,
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(format!("    (in {module_label})")),
                    description: None,
                }),
                ..Default::default()
            };
            if exports.module.is_some() {
                item.command = Some(make_auto_require(
                    "",
                    current_file,
                    file_id,
                    position,
                    label.clone(),
                    Some(name.to_string()),
                ));
            }
            builder.add_completion_item(item);
        }

        for global in &exports.globals {
            let label = global.name.to_string();
            if !label.to_lowercase().starts_with(&partial.to_lowercase()) {
                continue;
            }
            if !builder.env_duplicate_name.insert(label.clone()) {
                continue;
            }
            let mut item = CompletionItem {
                label: label.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                insert_text: None,
                detail: None,
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(format!("    (in {module_label})")),
                    description: None,
                }),
                ..Default::default()
            };
            if exports.module.is_some() {
                item.command = Some(make_auto_require(
                    "",
                    current_file,
                    file_id,
                    position,
                    label.clone(),
                    None,
                ));
            }
            builder.add_completion_item(item);
        }

        // `return processError`: when a module exports a single declaration (function),
        // complete it as that declaration; place it after types/members so it does not
        // shadow same-named type exports (KeepClass semantics).
        if let Some(ModuleExport::Decl { name, decl }) = &exports.module {
            let label = name.to_string();
            if label.to_lowercase().starts_with(&partial.to_lowercase())
                && builder.env_duplicate_name.insert(label.clone())
            {
                let ty = builder
                    .semantic_model
                    .type_of_decl(decl)
                    .unwrap_or(LuaType::Unknown);
                let kind = if ty.is_function() {
                    CompletionItemKind::FUNCTION
                } else {
                    CompletionItemKind::VARIABLE
                };
                let mut item = CompletionItem {
                    label: label.clone(),
                    kind: Some(kind),
                    insert_text: None,
                    detail: None,
                    label_details: Some(CompletionItemLabelDetails {
                        detail: Some(format!("    (in {module_label})")),
                        description: None,
                    }),
                    ..Default::default()
                };
                item.command = Some(make_auto_require(
                    "",
                    current_file,
                    file_id,
                    position,
                    label.clone(),
                    None,
                ));
                builder.add_completion_item(item);
            }
        }
    }
    let mut seen_modules = std::collections::HashSet::new();
    for file_id in builder.semantic_model.main_workspace_file_ids() {
        let Some(module_name) = builder.semantic_model.module_name_of(file_id) else {
            continue;
        };
        let Some(exports) = builder.semantic_model.file_exports(file_id) else {
            continue;
        };
        if exports.module.is_none() {
            continue;
        }
        if let Some(exported_name) = match &exports.module {
            Some(ModuleExport::Decl { name, .. }) | Some(ModuleExport::Global { name }) => {
                Some(name.as_str())
            }
            _ => None,
        } && exports
            .types
            .iter()
            .any(|def| def.full_name.as_str() == exported_name)
        {
            continue;
        }
        let stem = builder
            .semantic_model
            .file_path_of(file_id)
            .as_deref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| module_name.clone());
        if !stem.to_lowercase().starts_with(&partial.to_lowercase()) {
            continue;
        }
        if !seen_modules.insert(stem.clone()) || !builder.env_duplicate_name.insert(stem.clone()) {
            continue;
        }
        let mut item = CompletionItem {
            label: stem.clone(),
            kind: Some(CompletionItemKind::MODULE),
            insert_text: Some(stem.to_string()),
            detail: None,
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!("    (in {stem})")),
                description: None,
            }),
            ..Default::default()
        };
        item.command = Some(make_auto_require(
            "",
            current_file,
            file_id,
            position,
            stem.clone(),
            None,
        ));
        builder.add_completion_item(item);
    }
    Some(())
}
