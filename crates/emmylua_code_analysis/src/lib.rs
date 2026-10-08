#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        clippy::panic,
        clippy::panic_in_result_fn
    )
)]

mod check;
mod config;
mod locale;
mod resources;
mod semantic_db;
mod semantic_model;
mod test_lib;
mod vfs;

use crate::check::LuaDiagnostic;
pub use crate::semantic_db::SemanticDatabase;
pub use crate::semantic_db::def::ModuleVisibility;
/// Public types for the semantic layer.
pub use crate::semantic_db::def::{
    Decl, DeclKind, DocGenericParam, ExportKey, LuaMemberKey, Member, MemberRef, ModuleExport,
    OwnerId, SemanticId, Signature, SignatureDoc, SignatureReturnCast, TypeDef, TypeDefKind,
    TypeScope, TypeVisibility,
};
pub use crate::semantic_db::exports::{
    FileExportContribution, FileExports, GlobalExport, MemberExport,
};
pub use crate::semantic_db::facts::FileFacts;
pub use check::{
    CheckConfig, CheckProfile, DiagnosticCode, get_default_severity, is_code_default_enable,
};
pub use config::*;
pub use locale::get_locale_code;
use lsp_types::Uri;
pub use resources::get_best_resources_dir;
pub use resources::load_resource_from_include_dir;
use resources::load_resource_std;
pub use semantic_db::*;
/// Semantic model entry point.
pub use semantic_model::SemanticModel;
/// Semantic member lookup result (completion candidate).
pub use semantic_model::member::MemberInfo;
/// Semantic type rendering (unified humanize entry point).
pub use semantic_model::render::{
    humanize_type as humanize_semantic_type,
    humanize_type_detailed as humanize_semantic_type_detailed,
    humanize_type_with_level as humanize_semantic_type_with_level,
};
pub use semantic_model::{ResolvedMember, SemanticInfo};
use std::{path::PathBuf, sync::Arc};
pub use test_lib::VirtualWorkspace;

pub use vfs::*;

#[macro_use]
extern crate rust_i18n;

rust_i18n::i18n!("./locales", fallback = "en");

pub fn set_locale(locale: &str) {
    rust_i18n::set_locale(locale);
}

#[derive(Debug)]
pub struct EmmyLuaAnalysis {
    pub db: SemanticDatabase,
    pub diagnostic: LuaDiagnostic,
    pub emmyrc: Arc<Emmyrc>,
}

impl EmmyLuaAnalysis {
    pub fn new() -> Self {
        let emmyrc = Arc::new(Emmyrc::default());
        let mut db = SemanticDatabase::new();
        db.update_config(emmyrc.clone());
        Self {
            db,
            diagnostic: LuaDiagnostic::new(),
            emmyrc,
        }
    }

    pub fn init_std_lib(&mut self, create_resources_dir: Option<String>) {
        let is_jit = self.emmyrc.runtime.version.is_luajit();
        let (std_root, files) = load_resource_std(create_resources_dir, is_jit);
        self.db.add_std_workspace(std_root);

        let files = files
            .into_iter()
            .filter_map(|file| {
                if file.path.ends_with(".lua") {
                    Some((PathBuf::from(file.path), Some(file.content)))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let protected = files
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        self.db.add_protected_paths(protected);
        self.update_files_by_path(files);
    }

    pub fn get_file_id(&self, uri: &Uri) -> Option<FileId> {
        self.db.lookup_file_id(uri)
    }

    pub fn get_uri(&self, file_id: FileId) -> Option<Uri> {
        self.db.file_uri(file_id)
    }

    pub fn add_main_workspace(&mut self, root: PathBuf) {
        self.db.add_main_workspace(root);
    }

    /// Register a library workspace.
    pub fn add_library_workspace(&mut self, workspace: &WorkspaceFolder) {
        self.db.add_library_workspace(workspace);
    }

    /// Clear non-std workspaces (keep built-in std).
    pub fn clear_non_std_workspaces(&mut self) {
        self.db.clear_non_std_workspaces();
    }

    pub fn update_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> Option<FileId> {
        let file_id = self
            .db
            .lookup_file_id(uri)
            .unwrap_or_else(|| self.db.allocate_file_id());
        let change = match text {
            Some(text) => FileChange::Set {
                file_id,
                path: uri_to_file_path(uri),
                uri: Some(uri.clone()),
                text,
            },
            None => FileChange::Remove { file_id },
        };
        let _ = self.db.apply_file_change(change);
        Some(file_id)
    }

    pub fn update_file_by_path(&mut self, path: &PathBuf, text: Option<String>) -> Option<FileId> {
        let uri = file_path_to_uri(path)?;
        self.update_file_by_uri(&uri, text)
    }

    pub fn update_files_by_uri(&mut self, files: Vec<(Uri, Option<String>)>) -> Vec<FileId> {
        if files.len() > 1 {
            let (_, file_ids) = self.db.apply_batch_with_ids(BatchChange { files });
            return file_ids;
        }
        files
            .into_iter()
            .filter_map(|(uri, text)| self.update_file_by_uri(&uri, text))
            .collect()
    }

    #[allow(unused)]
    pub(crate) fn update_files_by_uri_sorted(
        &mut self,
        files: Vec<(Uri, Option<String>)>,
    ) -> Vec<FileId> {
        let mut updated_files = self.update_files_by_uri(files);
        updated_files.sort();
        updated_files
    }

    pub fn remove_file_by_uri(&mut self, uri: &Uri) -> Option<FileId> {
        let file_id = self.db.lookup_file_id(uri)?;
        let _ = self.db.apply_file_change(FileChange::Remove { file_id });
        Some(file_id)
    }

    pub fn update_files_by_path(&mut self, files: Vec<(PathBuf, Option<String>)>) -> Vec<FileId> {
        let files = files
            .into_iter()
            .filter_map(|(path, text)| {
                let uri = file_path_to_uri(&path)?;
                Some((uri, text))
            })
            .collect();
        self.update_files_by_uri(files)
    }

    pub fn reload_workspace_files(
        &mut self,
        files: Vec<(PathBuf, Option<String>)>,
        open_files: Vec<(Uri, String)>,
    ) -> Vec<Uri> {
        self.db.reload_workspace_files(files, open_files)
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        self.emmyrc = config.clone();
        self.diagnostic.update_config(config.clone());
        self.db.update_config(config);
    }

    pub fn get_emmyrc(&self) -> Arc<Emmyrc> {
        self.emmyrc.clone()
    }

    /// Semantic model: accesses only the semantic analysis layer.
    pub fn semantic_model(&self, file_id: FileId) -> SemanticModel<'_> {
        SemanticModel::new(&self.db, file_id)
    }

    pub fn diagnose_file_with_config(
        &self,
        file_id: FileId,
        config: Arc<CheckConfig>,
    ) -> Option<Vec<lsp_types::Diagnostic>> {
        let model = self.semantic_model(file_id);
        let diagnostics = check::check_file(&model, config);
        let line_index = self.db.line_index(file_id)?;
        let text = self.db.get_file_text(file_id)?;
        Some(
            diagnostics
                .into_iter()
                .filter_map(|d| {
                    let start = line_index.get_line_col(d.range.start(), text)?;
                    let end = line_index.get_line_col(d.range.end(), text)?;
                    Some(lsp_types::Diagnostic {
                        range: lsp_types::Range {
                            start: lsp_types::Position {
                                line: start.0 as u32,
                                character: start.1 as u32,
                            },
                            end: lsp_types::Position {
                                line: end.0 as u32,
                                character: end.1 as u32,
                            },
                        },
                        severity: Some(d.severity),
                        code: Some(lsp_types::NumberOrString::String(
                            d.code.get_name().to_string(),
                        )),
                        code_description: None,
                        source: Some("EmmyLua".to_string()),
                        message: d.message,
                        related_information: None,
                        tags: d.tags,
                        data: d.data,
                    })
                })
                .collect(),
        )
    }

    pub fn diagnose_file(
        &self,
        file_id: FileId,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Option<Vec<lsp_types::Diagnostic>> {
        self.diagnostic.diagnose_file(self, file_id, cancel_token)
    }

    /// Remove files that no longer exist on disk.
    pub fn cleanup_nonexistent_files(&mut self) {
        let mut files_to_remove = Vec::new();

        for file_id in self.db.file_ids() {
            if let Some(path) = self.db.file_path(file_id).filter(|path| !path.exists())
                && let Some(uri) = file_path_to_uri(&path)
            {
                files_to_remove.push(uri);
            }
        }

        for uri in files_to_remove {
            self.remove_file_by_uri(&uri);
        }
    }
}

impl Default for EmmyLuaAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for EmmyLuaAnalysis {}
unsafe impl Sync for EmmyLuaAnalysis {}
