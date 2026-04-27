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
mod module_query;
mod profile;
mod resources;
mod semantic;
mod test_lib;
mod vfs;

pub use compilation::*;
pub use config::*;
pub use db_index::*;
pub use diagnostic::*;
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
        self.compilation.add_workspace(Workspace::new(
            std_root,
            WorkspaceImport::All,
            WorkspaceId::STD,
        ));

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
        self.compilation.file_id_by_uri(uri)
    }

    pub fn get_uri(&self, file_id: FileId) -> Option<Uri> {
        self.compilation.get_uri(file_id)
    }

    pub fn add_main_workspace(&mut self, root: PathBuf) {
        self.compilation.add_workspace(Workspace::new(
            root,
            WorkspaceImport::All,
            WorkspaceId::MAIN,
        ));
    }

    pub fn add_library_workspace(&mut self, workspace: &WorkspaceFolder) {
        let id = WorkspaceId {
            id: self.compilation.next_library_workspace_id(),
        };
        self.compilation.add_workspace(Workspace::new(
            workspace.root.clone(),
            workspace.import.clone(),
            id,
        ));
    }

    pub fn clear_non_std_workspaces(&mut self) {
        self.compilation.clear_non_std_workspaces();
    }

    pub fn update_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> Option<FileId> {
        self.compilation.update_file_by_uri(uri, text)
    }

    pub fn update_remote_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        self.compilation.update_remote_file_by_uri(uri, text)
    }

    pub fn update_file_by_path(&mut self, path: &PathBuf, text: Option<String>) -> Option<FileId> {
        let uri = file_path_to_uri(path)?;
        self.update_file_by_uri(&uri, text)
    }

    pub fn update_files_by_uri(&mut self, files: Vec<(Uri, Option<String>)>) -> Vec<FileId> {
        let _p = Profile::new("update files");
        self.compilation.update_files_by_uri(files)
    }

    #[allow(unused)]
    pub(crate) fn update_files_by_uri_sorted(
        &mut self,
        files: Vec<(Uri, Option<String>)>,
    ) -> Vec<FileId> {
        let _p = Profile::new("update files");
        self.compilation.update_files_by_uri_sorted(files)
    }

    pub fn remove_file_by_uri(&mut self, uri: &Uri) -> Option<FileId> {
        self.compilation.remove_file_by_uri(uri)
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
            let compilation = &self.compilation;
            let mut had_existing_non_std_local_files = false;
            let stale_uris = compilation
                .all_local_file_ids()
                .into_iter()
                .filter(|file_id| {
                    let is_non_std = !compilation.module_is_std(*file_id);
                    had_existing_non_std_local_files |= is_non_std;
                    is_non_std
                })
                .filter_map(|file_id| compilation.get_document(file_id).map(|doc| doc.get_file_path().to_path_buf()))
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
        let file_ids = self.compilation.all_file_ids();
        self.compilation.clear_index();
        self.compilation.update_index(file_ids);
    }

    /// 清理文件系统中不再存在的文件
    pub fn cleanup_nonexistent_files(&mut self) {
        let mut files_to_remove = Vec::new();

        // 获取所有当前在VFS中的文件
        for file_id in self.compilation.all_local_file_ids() {
            if self.compilation.module_is_std(file_id) {
                continue;
            }
            if let Some(path) = self
                .compilation
                .get_document(file_id)
                .map(|doc| doc.get_file_path().to_path_buf())
                .filter(|path| !path.exists())
                && let Some(uri) = file_path_to_uri(&path)
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
            .legacy_db()
            .get_json_schema_index()
            .has_need_resolve_schemas()
    }

    pub async fn update_schema(&mut self) {
        let urls = self
            .compilation
            .legacy_db()
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
                        .legacy_db_mut()
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
            .legacy_db_mut()
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
    use std::{
        fs,
        path::PathBuf,
        sync::Arc,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEST_ANALYSIS_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

    #[test]
    fn sibling_package_workspace_folders_keep_distinct_workspace_ids() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = TEST_ANALYSIS_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_root = std::env::temp_dir().join(format!(
            "emmylua-analysis-package-scope-{}-{}-{}",
            std::process::id(),
            unique,
            counter,
        ));
        let package_parent = temp_root.join("module");
        let socket_file = package_parent.join("socket").join("init.lua");
        let net_file = package_parent.join("net").join("init.lua");

        fs::create_dir_all(socket_file.parent().unwrap()).unwrap();
        fs::create_dir_all(net_file.parent().unwrap()).unwrap();
        fs::write(&socket_file, "return true\n").unwrap();
        fs::write(&net_file, "return true\n").unwrap();

        let mut analysis = EmmyLuaAnalysis::new();
        analysis.update_config(Arc::new(Emmyrc::default()));
        analysis.add_library_workspace(&WorkspaceFolder::with_package(
            package_parent.clone(),
            PathBuf::from("socket"),
        ));
        analysis.add_library_workspace(&WorkspaceFolder::with_package(
            package_parent.clone(),
            PathBuf::from("net"),
        ));
        analysis.update_files_by_path(vec![
            (socket_file.clone(), Some("return true\n".to_string())),
            (net_file.clone(), Some("return true\n".to_string())),
        ]);

        let socket_file_id = analysis
            .get_file_id(&file_path_to_uri(&socket_file).unwrap())
            .unwrap();
        let net_file_id = analysis
            .get_file_id(&file_path_to_uri(&net_file).unwrap())
            .unwrap();
        assert_eq!(
            analysis
                .compilation
                .find_module_by_file_id(socket_file_id)
                .unwrap()
                .full_module_name,
            "socket"
        );
        assert_eq!(
            analysis
                .compilation
                .find_module_by_file_id(net_file_id)
                .unwrap()
                .full_module_name,
            "net"
        );
        assert_ne!(
            analysis.compilation.module_workspace_id(socket_file_id),
            analysis.compilation.module_workspace_id(net_file_id)
        );

        let _ = fs::remove_dir_all(temp_root);
    }
}
