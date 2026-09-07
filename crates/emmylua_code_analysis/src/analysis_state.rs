//! Shared data types used by the analysis layer.

use std::path::PathBuf;
use std::sync::Arc;

use lsp_types::Uri;
use rowan::TextSize;

use crate::FileId;
use crate::vfs::Vfs;

/// Plain file data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileData {
    pub file_id: FileId,
    pub uri: Option<Uri>,
    pub path: Option<PathBuf>,
    pub text: Arc<str>,
}

impl FileData {
    pub fn new(
        file_id: FileId,
        uri: Option<Uri>,
        path: Option<PathBuf>,
        text: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            file_id,
            uri,
            path,
            text: text.into(),
        }
    }

    pub fn text_len(&self) -> TextSize {
        TextSize::from(self.text.len() as u32)
    }
}

/// Future home of the non-Salsa workspace index.
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    // Will contain type/member/decl/module indexes in later phases.
}

/// Top-level non-Salsa analysis state.
#[derive(Debug)]
pub struct AnalysisState {
    pub vfs: Vfs,
    pub workspace: WorkspaceIndex,
}

impl Default for AnalysisState {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisState {
    pub fn new() -> Self {
        Self {
            vfs: Vfs::new(),
            workspace: WorkspaceIndex::default(),
        }
    }
}
