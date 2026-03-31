#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        clippy::panic,
        clippy::panic_in_result_fn
    )
)]

mod compilation;
mod config;
mod db_index;
mod diagnostic;
mod locale;
mod profile;
mod resources;
mod semantic;
mod test_lib;
mod vfs;

pub use compilation::*;
pub use config::*;
pub use db_index::*;
pub use diagnostic::*;
pub use emmylua_codestyle::*;
use hashbrown::HashMap;
pub use locale::get_locale_code;
use lsp_types::Uri;
pub use profile::Profile;
pub use resources::get_best_resources_dir;
pub use resources::load_resource_from_include_dir;
use resources::load_resource_std;
use schema_to_emmylua::SchemaConverter;
pub use semantic::*;
use std::str::FromStr;
use std::{collections::HashSet, path::PathBuf, sync::Arc};
pub use test_lib::VirtualWorkspace;
use tokio_util::sync::CancellationToken;
pub use vfs::*;

#[macro_use]
extern crate rust_i18n;

rust_i18n::i18n!("./locales", fallback = "en");

pub fn set_locale(locale: &str) {
    rust_i18n::set_locale(locale);
}

#[derive(Debug)]
pub struct EmmyLuaAnalysis {
    pub compilation: LuaCompilation,
    pub diagnostic: LuaDiagnostic,
    pub emmyrc: Arc<Emmyrc>,
    #[cfg(test)]
    reindex_count: usize,
}

impl EmmyLuaAnalysis {
    pub fn new() -> Self {
        let emmyrc = Arc::new(Emmyrc::default());
        Self {
            compilation: LuaCompilation::new(emmyrc.clone()),
            diagnostic: LuaDiagnostic::new(),
            emmyrc,
            #[cfg(test)]
            reindex_count: 0,
        }
    }

    pub fn init_std_lib(&mut self, create_resources_dir: Option<String>) {
        let is_jit = matches!(self.emmyrc.runtime.version, EmmyrcLuaVersion::LuaJIT);
        let (std_root, files) = load_resource_std(create_resources_dir, is_jit);
        self.compilation
            .get_db_mut()
            .get_module_index_mut()
            .add_workspace_root(std_root, WorkspaceId::STD);

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
        self.update_files_by_path(files);
    }

    pub fn get_file_id(&self, uri: &Uri) -> Option<FileId> {
        self.compilation.get_db().get_vfs().get_file_id(uri)
    }

    pub fn get_uri(&self, file_id: FileId) -> Option<Uri> {
        self.compilation.get_db().get_vfs().get_uri(&file_id)
    }

    pub fn add_main_workspace(&mut self, root: PathBuf) {
        self.compilation
            .get_db_mut()
            .get_module_index_mut()
            .add_workspace_root(root, WorkspaceId::MAIN);
    }

    pub fn add_library_workspace(&mut self, root: PathBuf) {
        let module_index = self.compilation.get_db_mut().get_module_index_mut();
        let id = WorkspaceId {
            id: module_index.next_library_workspace_id(),
        };
        module_index.add_workspace_root(root, id);
    }

    pub fn clear_non_std_workspaces(&mut self) {
        self.compilation
            .get_db_mut()
            .get_module_index_mut()
            .clear_non_std_workspaces();
    }

    pub fn update_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> Option<FileId> {
        let is_removed = text.is_none();
        let file_id = self
            .compilation
            .get_db_mut()
            .get_vfs_mut()
            .set_file_content(uri, text);

        self.compilation.remove_index(vec![file_id]);
        if !is_removed {
            self.compilation.update_index(vec![file_id]);
        }

        Some(file_id)
    }

    pub fn update_remote_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let is_removed = text.is_none();
        let fid = self
            .compilation
            .get_db_mut()
            .get_vfs_mut()
            .set_remote_file_content(uri, text);

        self.compilation.remove_index(vec![fid]);
        if !is_removed {
            self.compilation.update_index(vec![fid]);
        }
        fid
    }

    pub fn update_file_by_path(&mut self, path: &PathBuf, text: Option<String>) -> Option<FileId> {
        let uri = file_path_to_uri(path)?;
        self.update_file_by_uri(&uri, text)
    }

    pub fn update_files_by_uri(&mut self, files: Vec<(Uri, Option<String>)>) -> Vec<FileId> {
        let mut removed_files = HashSet::new();
        let mut updated_files = HashSet::new();
        {
            let _p = Profile::new("update files");
            for (uri, text) in files {
                let is_new_text = text.is_some();
                let file_id = self
                    .compilation
                    .get_db_mut()
                    .get_vfs_mut()
                    .set_file_content(&uri, text);
                removed_files.insert(file_id);
                if is_new_text {
                    updated_files.insert(file_id);
                }
            }
        }
        self.compilation
            .remove_index(removed_files.into_iter().collect());
        let updated_files: Vec<FileId> = updated_files.into_iter().collect();
        self.compilation.update_index(updated_files.clone());
        updated_files
    }

    #[allow(unused)]
    pub(crate) fn update_files_by_uri_sorted(
        &mut self,
        files: Vec<(Uri, Option<String>)>,
    ) -> Vec<FileId> {
        let mut removed_files = HashSet::new();
        let mut updated_files = HashSet::new();
        {
            let _p = Profile::new("update files");
            for (uri, text) in files {
                let is_new_text = text.is_some();
                let file_id = self
                    .compilation
                    .get_db_mut()
                    .get_vfs_mut()
                    .set_file_content(&uri, text);
                removed_files.insert(file_id);
                if is_new_text {
                    updated_files.insert(file_id);
                }
            }
        }
        self.compilation
            .remove_index(removed_files.into_iter().collect());
        let mut updated_files: Vec<FileId> = updated_files.into_iter().collect();
        updated_files.sort();
        self.compilation.update_index(updated_files.clone());
        updated_files
    }

    pub fn remove_file_by_uri(&mut self, uri: &Uri) -> Option<FileId> {
        if let Some(file_id) = self.compilation.get_db_mut().get_vfs_mut().remove_file(uri) {
            self.compilation.remove_index(vec![file_id]);
            return Some(file_id);
        }

        None
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
        let open_paths: HashSet<_> = open_files
            .iter()
            .filter_map(|(uri, _)| uri_to_file_path(uri))
            .collect();
        let mut kept_paths = open_paths.clone();
        kept_paths.extend(files.iter().map(|(path, _)| path.clone()));

        let (had_existing_non_std_local_files, stale_uris) = {
            let db = self.compilation.get_db();
            let vfs = db.get_vfs();
            let module_index = db.get_module_index();
            let mut had_existing_non_std_local_files = false;
            let stale_uris = vfs
                .get_all_local_file_ids()
                .into_iter()
                .filter(|file_id| {
                    let is_non_std = !module_index.is_std(file_id);
                    had_existing_non_std_local_files |= is_non_std;
                    is_non_std
                })
                .filter_map(|file_id| vfs.get_file_path(&file_id).cloned())
                .filter(|path| !kept_paths.contains(path))
                .filter_map(|path| file_path_to_uri(&path))
                .collect::<Vec<_>>();
            (had_existing_non_std_local_files, stale_uris)
        };
        for uri in &stale_uris {
            self.remove_file_by_uri(uri);
        }

        self.update_files_by_path(
            files
                .into_iter()
                .filter(|(path, _)| !open_paths.contains(path))
                .collect(),
        );
        self.update_files_by_uri(
            open_files
                .into_iter()
                .map(|(uri, text)| (uri, Some(text)))
                .collect(),
        );
        if had_existing_non_std_local_files {
            self.reindex();
        }
        stale_uris
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        self.emmyrc = config.clone();
        self.compilation.update_config(config.clone());
        self.diagnostic.update_config(config);
    }

    pub fn get_emmyrc(&self) -> Arc<Emmyrc> {
        self.emmyrc.clone()
    }

    pub fn diagnose_file(
        &self,
        file_id: FileId,
        cancel_token: CancellationToken,
    ) -> Option<Vec<lsp_types::Diagnostic>> {
        self.diagnostic
            .diagnose_file(&self.compilation, file_id, cancel_token)
    }

    pub fn reindex(&mut self) {
        #[cfg(test)]
        {
            self.reindex_count += 1;
        }
        let file_ids = self.compilation.get_db().get_vfs().get_all_file_ids();
        self.compilation.clear_index();
        self.compilation.update_index(file_ids);
    }

    /// 清理文件系统中不再存在的文件
    pub fn cleanup_nonexistent_files(&mut self) {
        let mut files_to_remove = Vec::new();

        // 获取所有当前在VFS中的文件
        let vfs = self.compilation.get_db().get_vfs();
        for file_id in vfs.get_all_local_file_ids() {
            if self
                .compilation
                .get_db()
                .get_module_index()
                .is_std(&file_id)
            {
                continue;
            }
            if let Some(path) = vfs.get_file_path(&file_id).filter(|path| !path.exists())
                && let Some(uri) = file_path_to_uri(path)
            {
                files_to_remove.push(uri);
            }
        }

        // 移除不存在的文件
        for uri in files_to_remove {
            self.remove_file_by_uri(&uri);
        }
    }

    pub fn check_schema_update(&self) -> bool {
        self.compilation
            .get_db()
            .get_json_schema_index()
            .has_need_resolve_schemas()
    }

    pub async fn update_schema(&mut self) {
        let urls = self
            .compilation
            .get_db()
            .get_json_schema_index()
            .get_need_resolve_schemas();
        let mut url_contents = HashMap::new();
        for url in urls {
            if url.scheme() == "file" {
                if let Ok(path) = url.to_file_path() {
                    if path.exists() {
                        let result = read_file_with_encoding(&path, "utf-8");
                        if let Some(content) = result {
                            url_contents.insert(url.clone(), content);
                        } else {
                            log::error!("Failed to read schema file: {:?}", url);
                        }
                    }
                }
            } else {
                #[cfg(feature = "reqwest")]
                {
                    let result = reqwest::get(url.as_str()).await;
                    if let Ok(response) = result {
                        if let Ok(content) = response.text().await {
                            url_contents.insert(url.clone(), content);
                        } else {
                            log::error!("Failed to read schema content from URL: {:?}", url);
                        }
                    } else {
                        log::error!("Failed to fetch schema from URL: {:?}", url);
                    }
                }
            }
        }

        if url_contents.is_empty() {
            return;
        }

        let converter = SchemaConverter::new(true);
        for (url, json_content) in url_contents {
            match converter.convert_from_str(&json_content) {
                Ok(convert_result) => {
                    let uri = match Uri::from_str(url.as_str()) {
                        Ok(uri) => uri,
                        Err(e) => {
                            log::error!("Failed to convert URL to URI {:?}: {}", url, e);
                            continue;
                        }
                    };
                    let file_id =
                        self.update_remote_file_by_uri(&uri, Some(convert_result.annotation_text));
                    if let Some(f) = self
                        .compilation
                        .get_db_mut()
                        .get_json_schema_index_mut()
                        .get_schema_file_mut(&url)
                    {
                        *f = JsonSchemaFile::Resolved(LuaTypeDeclId::local(
                            file_id,
                            &convert_result.root_type_name,
                        ));
                    }
                }
                Err(e) => {
                    log::error!("Failed to convert schema from URL {:?}: {}", url, e);
                }
            }
        }

        self.compilation
            .get_db_mut()
            .get_json_schema_index_mut()
            .reset_rest_schemas();
    }
}

impl Default for EmmyLuaAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for EmmyLuaAnalysis {}
unsafe impl Sync for EmmyLuaAnalysis {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_workspace_files_skips_reindex_when_bootstrapping_workspace() {
        let mut analysis = EmmyLuaAnalysis::new();
        let workspace_root = std::env::current_dir().unwrap();
        let file_path = workspace_root.join("__reload_workspace_startup_test.lua");
        analysis.add_main_workspace(workspace_root);

        analysis.reload_workspace_files(
            vec![(file_path.clone(), Some("return true\n".to_string()))],
            Vec::new(),
        );

        assert_eq!(analysis.reindex_count, 0);
        assert!(
            analysis
                .get_file_id(&file_path_to_uri(&file_path).unwrap())
                .is_some()
        );
    }

    #[test]
    fn reload_workspace_files_reindexes_existing_workspace_files() {
        let mut analysis = EmmyLuaAnalysis::new();
        let workspace_root = std::env::current_dir().unwrap();
        let file_path = workspace_root.join("__reload_workspace_existing_test.lua");
        analysis.add_main_workspace(workspace_root);
        analysis.update_files_by_path(vec![(file_path.clone(), Some("return true\n".to_string()))]);

        analysis.reload_workspace_files(
            vec![(file_path, Some("return false\n".to_string()))],
            Vec::new(),
        );

        assert_eq!(analysis.reindex_count, 1);
    }
}
