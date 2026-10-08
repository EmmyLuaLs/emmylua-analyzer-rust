//! Pure data state for WorkspaceManager.
//!
//! The goal is to let `WorkspaceManager` only orchestrate reload/reindex, while workspace
//! folders, open files, matcher, and client config live here.

use std::collections::HashMap;
use std::path::PathBuf;

use emmylua_code_analysis::{Emmyrc, WorkspaceFileMatcher, WorkspaceFolder, uri_to_file_path};
use lsp_types::Uri;

use crate::handlers::ClientConfig;

#[derive(Debug, Clone, Default)]
pub(crate) struct OpenFilesSnapshot {
    pub(crate) version: u64,
    pub(crate) files: Vec<(Uri, String)>,
}

#[derive(Debug)]
pub struct WorkspaceState {
    pub workspace_folders: Vec<WorkspaceFolder>,
    pub watcher: Option<notify::RecommendedWatcher>,
    pub open_file_texts: HashMap<Uri, String>,
    pub open_file_state_version: u64,
    pub match_file_pattern: WorkspaceFileMatcher,
    pub client_config: ClientConfig,
}

impl WorkspaceState {
    pub fn new(client_config: ClientConfig) -> Self {
        Self {
            workspace_folders: Vec::new(),
            watcher: None,
            open_file_texts: HashMap::new(),
            open_file_state_version: 0,
            match_file_pattern: WorkspaceFileMatcher::default(),
            client_config,
        }
    }

    pub fn update_match_state(&mut self, emmyrc: &Emmyrc) {
        self.match_file_pattern = WorkspaceFileMatcher::new(&self.workspace_folders, emmyrc);
    }

    pub fn sync_open_file(&mut self, uri: Uri, text: String) {
        self.open_file_texts.insert(uri, text);
        self.open_file_state_version = self.open_file_state_version.wrapping_add(1);
    }

    pub fn close_open_file(&mut self, uri: &Uri) {
        self.open_file_texts.remove(uri);
        self.open_file_state_version = self.open_file_state_version.wrapping_add(1);
    }

    pub fn is_open_file(&self, uri: &Uri) -> bool {
        self.open_file_texts.contains_key(uri)
    }

    pub fn workspace_open_files(&self) -> Vec<(Uri, String)> {
        self.open_file_texts
            .iter()
            .filter(|(uri, _)| self.is_workspace_file(uri))
            .map(|(uri, text)| (uri.clone(), text.clone()))
            .collect()
    }

    pub(crate) fn workspace_open_files_snapshot(&self) -> OpenFilesSnapshot {
        OpenFilesSnapshot {
            version: self.open_file_state_version,
            files: self.workspace_open_files(),
        }
    }

    pub fn is_workspace_file(&self, uri: &Uri) -> bool {
        if self.workspace_folders.is_empty() {
            return true;
        }

        let Some(file_path) = uri_to_file_path(uri) else {
            return true;
        };

        self.match_file_pattern.is_match(&file_path)
    }

    pub fn config_root(&self) -> Option<PathBuf> {
        self.workspace_folders
            .first()
            .map(|workspace| workspace.root.clone())
    }
}
