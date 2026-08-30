//! `--- @see <??>`: type name and workspace file/module name completion.

use emmylua_code_analysis::SalsaSemanticModel;
use lsp_types::{CompletionItem, CompletionItemKind};

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision};

pub struct SeeCompletionProvider;

impl CompletionProvider for SeeCompletionProvider {
    fn name(&self) -> &'static str {
        "see"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        builder.trigger_token.kind() == emmylua_parser::LuaTokenKind::TkDocSeeContent.into()
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        let partial = builder.get_trigger_text();
        let db = builder.semantic_model.db();
        let mut names = Vec::new();

        // Global type definitions.
        for file_id in db.file_ids() {
            if let Some(model) = SalsaSemanticModel::new(db, file_id)
                && let Some(exports) = model.file_exports_current()
            {
                for def in &exports.types {
                    if def.name.starts_with(&partial) {
                        names.push((def.name.to_string(), CompletionItemKind::CLASS));
                    }
                }
            }
        }
        names.sort_by(|a, b| a.0.cmp(&b.0));
        names.dedup_by(|a, b| a.0 == b.0);

        // Workspace files (module/file candidates from the old desc provider).
        let mut files = Vec::new();
        for file_id in db.file_ids() {
            if let Some(path) = db.file_path(file_id) {
                if let Some(stem) = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                {
                    files.push(stem);
                }
            }
        }
        files.sort();
        files.dedup();

        for (name, kind) in names {
            builder.add_completion_item(CompletionItem {
                label: name,
                kind: Some(kind),
                ..Default::default()
            });
        }
        for name in files {
            builder.add_completion_item(CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::FILE),
                ..Default::default()
            });
        }
        ProviderDecision::Stop
    }
}
