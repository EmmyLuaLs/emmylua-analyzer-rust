#![deny(
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::panic,
    clippy::panic_in_result_fn
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
pub use locale::get_locale_code;
use lsp_types::Uri;
pub use profile::Profile;
use resources::load_resource_std;
pub use semantic::*;
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
    lib_workspace_counter: u32,
}

impl EmmyLuaAnalysis {
    pub fn new() -> Self {
        let emmyrc = Arc::new(Emmyrc::default());
        Self {
            compilation: LuaCompilation::new(emmyrc.clone()),
            diagnostic: LuaDiagnostic::new(),
            emmyrc,
            lib_workspace_counter: 2,
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
        let id = WorkspaceId {
            id: self.lib_workspace_counter,
        };
        self.lib_workspace_counter += 1;

        self.compilation
            .get_db_mut()
            .get_module_index_mut()
            .add_workspace_root(root, id);
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

    pub fn update_file_by_path(&mut self, path: &PathBuf, text: Option<String>) -> Option<FileId> {
        let uri = file_path_to_uri(&path)?;
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
        let module = self.compilation.get_db().get_module_index();
        let std_file_ids = module.get_std_file_ids();
        let main_file_ids = module.get_main_workspace_file_ids();
        let lib_file_ids = module.get_lib_file_ids();
        self.compilation.clear_index();

        self.compilation.update_index(std_file_ids);
        self.compilation.update_index(lib_file_ids);
        self.compilation.update_index(main_file_ids);
    }

    /// 清理文件系统中不再存在的文件
    pub fn cleanup_nonexistent_files(&mut self) {
        let mut files_to_remove = Vec::new();

        // 获取所有当前在VFS中的文件
        let vfs = self.compilation.get_db().get_vfs();
        for file_id in vfs.get_all_file_ids() {
            if self
                .compilation
                .get_db()
                .get_module_index()
                .is_std(&file_id)
            {
                continue;
            }
            if let Some(path) = vfs.get_file_path(&file_id) {
                if !path.exists() {
                    if let Some(uri) = file_path_to_uri(path) {
                        files_to_remove.push(uri);
                    }
                }
            }
        }

        // 移除不存在的文件
        for uri in files_to_remove {
            self.remove_file_by_uri(&uri);
        }
    }
}

unsafe impl Send for EmmyLuaAnalysis {}
unsafe impl Sync for EmmyLuaAnalysis {}
