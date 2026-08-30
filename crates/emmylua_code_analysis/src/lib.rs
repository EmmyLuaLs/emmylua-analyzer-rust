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
mod salsa_builder;
mod semantic_model;
mod test_lib;
mod vfs;

use crate::check::LuaDiagnostic;
pub use crate::salsa_builder::def::ModuleVisibility as SalsaModuleVisibility;
/// Public types for the salsa semantic layer.
pub use crate::salsa_builder::def::{
    Decl, DeclKind, LuaMemberKey, Member, MemberRef, ModuleExport, SalsaGenericParam, SemanticId,
    Signature as SalsaSignature, SignatureDoc as SalsaSignatureDoc,
    SignatureReturnCast as SalsaSignatureReturnCast, TypeDef, TypeDefKind, TypeScope,
    TypeVisibility,
};
pub use crate::salsa_builder::exports::{FileExports, GlobalExport, MemberExport};
pub use crate::salsa_builder::facts::FileFacts;
pub use crate::salsa_builder::{DocumentView, SalsaDatabase};
pub use check::{DiagnosticCode, get_default_severity, is_code_default_enable};
pub use config::*;
pub use locale::get_locale_code;
use lsp_types::Uri;
pub use resources::get_best_resources_dir;
pub use resources::load_resource_from_include_dir;
use resources::load_resource_std;
pub use salsa_builder::*;
/// Public alias for the new semantic model (rename to `SemanticModel` after the legacy semantic module is folded in).
pub use semantic_model::SemanticModel as SalsaSemanticModel;
/// Salsa member lookup result (completion candidate).
pub use semantic_model::member::MemberInfo as SalsaMemberInfo;
/// Salsa semantic-layer type rendering (unified humanize entry point).
pub use semantic_model::render::{
    humanize_type as humanize_semantic_type,
    humanize_type_detailed as humanize_semantic_type_detailed,
    humanize_type_with_level as humanize_semantic_type_with_level,
};
pub use semantic_model::{ResolvedMember, SemanticInfo};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
pub use test_lib::VirtualWorkspace;

pub use vfs::*;

#[macro_use]
extern crate rust_i18n;

rust_i18n::i18n!("./locales", fallback = "en");

pub fn set_locale(locale: &str) {
    rust_i18n::set_locale(locale);
}

#[derive(Debug, Clone)]
pub struct EmmyLuaAnalysis {
    pub salsa: SalsaDatabase,
    pub diagnostic: LuaDiagnostic,
    pub emmyrc: Arc<Emmyrc>,
}

impl EmmyLuaAnalysis {
    pub fn new() -> Self {
        let emmyrc = Arc::new(Emmyrc::default());
        let mut salsa = SalsaDatabase::new();
        salsa.update_config(emmyrc.clone());
        Self {
            salsa,
            diagnostic: LuaDiagnostic::new(),
            emmyrc,
        }
    }

    pub fn init_std_lib(&mut self, create_resources_dir: Option<String>) {
        let is_jit = self.emmyrc.runtime.version.is_luajit();
        let (std_root, files) = load_resource_std(create_resources_dir, is_jit);
        self.salsa.add_std_workspace(std_root);

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
        self.salsa.add_protected_paths(protected);
        self.update_files_by_path(files);
    }

    pub fn get_file_id(&self, uri: &Uri) -> Option<FileId> {
        self.salsa.lookup_file_id(uri)
    }

    pub fn get_uri(&self, file_id: FileId) -> Option<Uri> {
        self.salsa.file_uri(file_id)
    }

    pub fn add_main_workspace(&mut self, root: PathBuf) {
        self.salsa.add_main_workspace(root);
    }

    /// Register a library workspace.
    pub fn add_library_workspace(&mut self, workspace: &WorkspaceFolder) {
        self.salsa.add_library_workspace(workspace);
    }

    /// Clear non-std workspaces (keep built-in std).
    pub fn clear_non_std_workspaces(&mut self) {
        self.salsa.clear_non_std_workspaces();
    }

    pub fn update_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> Option<FileId> {
        Some(self.salsa.set_file_content(uri, text))
    }

    pub fn update_remote_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        self.salsa.set_file_content(uri, text)
    }

    pub fn update_file_by_path(&mut self, path: &PathBuf, text: Option<String>) -> Option<FileId> {
        let uri = file_path_to_uri(path)?;
        self.update_file_by_uri(&uri, text)
    }

    pub fn update_files_by_uri(&mut self, files: Vec<(Uri, Option<String>)>) -> Vec<FileId> {
        files
            .into_iter()
            .map(|(uri, text)| self.salsa.set_file_content(&uri, text))
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
        let file_id = self.salsa.lookup_file_id(uri)?;
        self.salsa.remove_file(file_id);
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
        use std::collections::HashMap;

        let open_paths: HashSet<_> = open_files
            .iter()
            .filter_map(|(uri, _)| uri_to_file_path(uri))
            .collect();
        let mut kept_paths = open_paths.clone();
        kept_paths.extend(files.iter().map(|(path, _)| path.clone()));
        // Built-in std and other protected files are not workspace files; they must not be deleted on reload.
        kept_paths.extend(self.salsa.protected_paths().iter().cloned());

        let old_files = self.salsa.file_input_map();

        // Compute the local files that need to be removed.
        let stale_uris: Vec<Uri> = old_files
            .values()
            .filter_map(|input| {
                let path = input.path(&self.salsa).as_ref()?;
                if kept_paths.contains(path) {
                    None
                } else {
                    file_path_to_uri(path)
                }
            })
            .collect();

        // Build the final file table in bulk to avoid O(n^2) salsa writes from per-file remove/update.
        let mut new_files = old_files;
        let mut path_to_id: HashMap<PathBuf, FileId> = new_files
            .iter()
            .filter_map(|(id, input)| {
                let path = input.path(&self.salsa).as_ref()?.clone();
                Some((path, *id))
            })
            .collect();

        // Files on disk (not open in the editor).
        for (path, text) in files
            .into_iter()
            .filter(|(path, _)| !open_paths.contains(path))
        {
            let uri = file_path_to_uri(&path);
            let id = path_to_id
                .get(&path)
                .copied()
                .unwrap_or_else(|| self.salsa.allocate_file_id());
            if let Some(text) = text {
                let input = self
                    .salsa
                    .upsert_file_input(id, Some(path.clone()), uri, text);
                path_to_id.insert(path.clone(), id);
                new_files.insert(id, input);
            } else {
                new_files.remove(&id);
                path_to_id.remove(&path);
            }
        }

        // Open unsaved files.
        for (uri, text) in open_files {
            let path = uri_to_file_path(&uri);
            let id = self
                .salsa
                .lookup_file_id(&uri)
                .or_else(|| path.as_ref().and_then(|path| path_to_id.get(path).copied()))
                .unwrap_or_else(|| self.salsa.allocate_file_id());
            let input = self
                .salsa
                .upsert_file_input(id, path.clone(), Some(uri.clone()), text);
            new_files.insert(id, input);
            if let Some(path) = &path {
                path_to_id.insert(path.clone(), id);
            }
        }

        self.salsa.replace_workspace_files(new_files);
        stale_uris
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        self.emmyrc = config.clone();
        self.diagnostic.update_config(config.clone());
        self.salsa.update_config(config);
    }

    pub fn get_emmyrc(&self) -> Arc<Emmyrc> {
        self.emmyrc.clone()
    }

    // ── Salsa analysis layer ──

    /// Semantic model: accesses only the salsa analysis layer.
    pub fn semantic_model(&self, file_id: FileId) -> Option<semantic_model::SemanticModel<'_>> {
        semantic_model::SemanticModel::new(&self.salsa, file_id)
    }

    /// Salsa query snapshot: clones the shared memo table and can be sent to worker threads for concurrent queries.
    pub fn salsa_snapshot(&self) -> SalsaDatabase {
        self.salsa.clone()
    }

    pub fn diagnose_salsa(
        &self,
        file_id: FileId,
        config: Arc<check::CheckConfig>,
    ) -> Option<Vec<lsp_types::Diagnostic>> {
        let model = self.semantic_model(file_id)?;
        let diagnostics = check::check_file(&model, config);
        let line_index = self.salsa.line_index(file_id)?;
        let text = self.salsa.get_file_text(file_id)?;
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

    /// Salsa has no index-rebuild concept (memos invalidate automatically); keep an empty implementation for compatibility.
    pub fn reindex(&mut self) {}

    /// Remove files that no longer exist on disk.
    pub fn cleanup_nonexistent_files(&mut self) {
        let mut files_to_remove = Vec::new();

        for file_id in self.salsa.file_ids() {
            if let Some(path) = self.salsa.file_path(file_id).filter(|path| !path.exists())
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

// Whether the first parameter should not be treated as `self`
pub fn first_param_may_not_self(typ: &LuaType) -> bool {
    if typ.is_table()
        || matches!(
            typ,
            LuaType::TplRef(_) | LuaType::StrTplRef(_) | LuaType::Any | LuaType::Unknown
        )
    {
        return true;
    }

    if let LuaType::Union(u) = typ {
        return u.into_vec().iter().any(first_param_may_not_self);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// M4: analysis.salsa is the only analysis layer — after file updates salsa exposes facts and types.
    #[test]
    fn test_analysis_salsa_direct_field_sync() {
        use lsp_types::Uri;
        use std::str::FromStr;

        let mut analysis = EmmyLuaAnalysis::new();
        let uri = Uri::from_str("file:///C:/ws/sync.lua").unwrap();
        let fid = analysis
            .update_file_by_uri(&uri, Some("local x = 1\nlocal y = x".to_string()))
            .expect("file id");

        let model = analysis.semantic_model(fid).expect("salsa model");
        let decls = model.decls().expect("decls");
        assert_eq!(decls.len(), 2, "salsa 事实同步");
        let y = decls.iter().find(|d| d.name == "y").expect("y decl");
        assert_eq!(
            model.type_of_decl(&y.id),
            Some(LuaType::IntegerConst(1)),
            "salsa 类型查询可用"
        );

        // Line index: TextRange → LSP line/column.
        let index = analysis.salsa.line_index(fid).expect("line index");
        let text = analysis.salsa.get_file_text(fid).expect("text");
        let (line, col) = index
            .get_line_col(y.name_range.start(), text)
            .expect("line col");
        assert_eq!((line, col), (1, 6), "第二行 local y 的 y 列");
    }

    #[test]
    fn test_reload_workspace_preserves_protected_paths() {
        use std::path::PathBuf;

        let mut analysis = EmmyLuaAnalysis::new();
        let protected_path = PathBuf::from("C:/protected/std.lua");
        analysis
            .salsa
            .add_protected_paths(vec![protected_path.clone()]);
        analysis
            .update_file_by_path(&protected_path, Some("return 1".to_string()))
            .expect("file id");

        let removed = analysis.reload_workspace_files(Vec::new(), Vec::new());
        assert!(removed.is_empty(), "protected file should not be removed");

        let uri = file_path_to_uri(&protected_path).unwrap();
        let fid = analysis.get_file_id(&uri).expect("file should remain");
        assert_eq!(
            analysis.salsa.file_path(fid).as_deref(),
            Some(protected_path.as_path())
        );
    }

    /// M4: configuration is written directly to salsa.
    #[test]
    fn test_update_config_salsa() {
        let mut analysis = EmmyLuaAnalysis::new();
        let emmyrc = Arc::new(Emmyrc::default());
        analysis.update_config(emmyrc.clone());
        assert!(Arc::ptr_eq(&analysis.emmyrc, &emmyrc));
    }
}
