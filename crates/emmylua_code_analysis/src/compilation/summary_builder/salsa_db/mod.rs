mod facade;
mod inputs;
#[cfg(test)]
mod tests;
mod tracked;

use std::{fmt, path::PathBuf, sync::Arc};

use hashbrown::HashMap;
use lsp_types::Uri;

use crate::{Emmyrc, FileId, Vfs, db_index::Workspace};

pub use facade::{
    SalsaSummaryDocQueries, SalsaSummaryFileQueries, SalsaSummaryFlowQueries,
    SalsaSummaryLexicalQueries, SalsaSummaryModuleQueries, SalsaSummarySemanticQueries,
    SalsaSummaryTypeQueries,
};
use inputs::{
    SummaryConfigInput, SummarySourceFileInput, SummaryWorkspaceInput, snapshot_vfs_file,
};

pub use inputs::SalsaSummaryConfig;

#[salsa::db]
pub trait SummaryDb: salsa::Database {}

#[salsa::db]
pub struct SalsaSummaryDatabase {
    storage: salsa::Storage<Self>,
    files: HashMap<FileId, SummarySourceFileInput>,
    workspaces: Option<SummaryWorkspaceInput>,
    config: Option<SummaryConfigInput>,
}

impl Default for SalsaSummaryDatabase {
    fn default() -> Self {
        Self {
            storage: salsa::Storage::default(),
            files: HashMap::new(),
            workspaces: None,
            config: None,
        }
    }
}

#[salsa::db]
impl salsa::Database for SalsaSummaryDatabase {}

#[salsa::db]
impl SummaryDb for SalsaSummaryDatabase {}

impl fmt::Debug for SalsaSummaryDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SalsaSummaryDatabase")
            .field("file_count", &self.files.len())
            .field("has_workspaces", &self.workspaces.is_some())
            .field("has_config", &self.config.is_some())
            .finish()
    }
}

impl SalsaSummaryDatabase {
    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.config = Some(SummaryConfigInput::new(
            self,
            SalsaSummaryConfig::from_emmyrc(emmyrc),
        ));
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<Workspace>) {
        self.workspaces = Some(SummaryWorkspaceInput::new(self, workspaces));
    }

    pub fn set_file(
        &mut self,
        file_id: FileId,
        path: Option<PathBuf>,
        text: String,
        is_remote: bool,
    ) {
        let input = SummarySourceFileInput::new(self, file_id, path, text, is_remote);
        self.files.insert(file_id, input);
    }

    pub fn set_file_from_vfs(&mut self, vfs: &Vfs, file_id: FileId) -> bool {
        let Some((path, text, is_remote)) = snapshot_vfs_file(vfs, file_id) else {
            return false;
        };
        self.set_file(file_id, path, text, is_remote);
        true
    }

    pub fn remove_file(&mut self, file_id: FileId) {
        self.files.remove(&file_id);
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.workspaces = None;
    }

    pub fn file(&self) -> SalsaSummaryFileQueries<'_> {
        SalsaSummaryFileQueries::new(self)
    }

    pub fn doc(&self) -> SalsaSummaryDocQueries<'_> {
        SalsaSummaryDocQueries::new(self)
    }

    pub fn lexical(&self) -> SalsaSummaryLexicalQueries<'_> {
        SalsaSummaryLexicalQueries::new(self)
    }

    pub fn flow(&self) -> SalsaSummaryFlowQueries<'_> {
        SalsaSummaryFlowQueries::new(self)
    }

    pub fn module(&self) -> SalsaSummaryModuleQueries<'_> {
        SalsaSummaryModuleQueries::new(self)
    }

    pub fn semantic(&self) -> SalsaSummarySemanticQueries<'_> {
        SalsaSummarySemanticQueries::new(self)
    }

    pub fn types(&self) -> SalsaSummaryTypeQueries<'_> {
        SalsaSummaryTypeQueries::new(self)
    }
}

#[derive(Debug)]
pub struct SalsaSummaryHost {
    db: SalsaSummaryDatabase,
    vfs: Vfs,
}

impl SalsaSummaryHost {
    pub fn new(emmyrc: Arc<Emmyrc>) -> Self {
        let mut db = SalsaSummaryDatabase::default();
        let mut vfs = Vfs::new();
        db.update_config(emmyrc.clone());
        vfs.update_config(emmyrc);
        Self { db, vfs }
    }

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.db.update_config(emmyrc.clone());
        self.vfs.update_config(emmyrc);
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<Workspace>) {
        self.db.set_workspaces(workspaces);
    }

    #[cfg(test)]
    fn set_file(&mut self, file_id: FileId, path: Option<PathBuf>, text: String, is_remote: bool) {
        self.db.set_file(file_id, path, text, is_remote);
    }

    #[cfg(test)]
    fn set_file_from_vfs(&mut self, vfs: &Vfs, file_id: FileId) -> bool {
        self.db.set_file_from_vfs(vfs, file_id)
    }

    pub fn sync_file(&mut self, file_id: FileId) -> bool {
        self.db.set_file_from_vfs(&self.vfs, file_id)
    }

    pub fn update_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let file_id = self.vfs.set_file_content(uri, text);
        if self.sync_file(file_id) {
            file_id
        } else {
            self.remove_file(file_id);
            file_id
        }
    }

    pub fn update_remote_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let file_id = self.vfs.set_remote_file_content(uri, text);
        if self.sync_file(file_id) {
            file_id
        } else {
            self.remove_file(file_id);
            file_id
        }
    }

    pub fn update_files_by_uri(&mut self, files: Vec<(Uri, Option<String>)>) -> Vec<FileId> {
        let mut file_ids = Vec::with_capacity(files.len());
        for (uri, text) in files {
            file_ids.push(self.update_file_by_uri(&uri, text));
        }
        file_ids
    }

    pub fn remove_file_by_uri(&mut self, uri: &Uri) -> Option<FileId> {
        let file_id = self.vfs.remove_file(uri)?;
        self.remove_file(file_id);
        Some(file_id)
    }

    pub fn remove_file(&mut self, file_id: FileId) {
        self.db.remove_file(file_id);
    }

    pub fn clear(&mut self) {
        self.db.clear();
    }

    pub fn file(&self) -> SalsaSummaryFileQueries<'_> {
        SalsaSummaryFileQueries::new(&self.db)
    }

    pub fn doc(&self) -> SalsaSummaryDocQueries<'_> {
        SalsaSummaryDocQueries::new(&self.db)
    }

    pub fn lexical(&self) -> SalsaSummaryLexicalQueries<'_> {
        SalsaSummaryLexicalQueries::new(&self.db)
    }

    pub fn flow(&self) -> SalsaSummaryFlowQueries<'_> {
        SalsaSummaryFlowQueries::new(&self.db)
    }

    pub fn module(&self) -> SalsaSummaryModuleQueries<'_> {
        SalsaSummaryModuleQueries::new(&self.db)
    }

    pub fn semantic(&self) -> SalsaSummarySemanticQueries<'_> {
        SalsaSummarySemanticQueries::new(&self.db)
    }

    pub fn types(&self) -> SalsaSummaryTypeQueries<'_> {
        SalsaSummaryTypeQueries::new(&self.db)
    }
}
