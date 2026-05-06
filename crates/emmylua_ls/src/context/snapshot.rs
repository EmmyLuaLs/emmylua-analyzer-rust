use std::sync::Arc;
use tokio::sync::RwLock;

use crate::context::lsp_features::LspFeatures;

use super::{
    AnalysisLock, client::ClientProxy, file_diagnostic::FileDiagnostic, status_bar::StatusBar,
    workspace_manager::WorkspaceManager,
};

#[derive(Clone)]
pub struct ServerContextSnapshot {
    inner: Arc<ServerContextInner>,
}

impl ServerContextSnapshot {
    pub fn new(inner: Arc<ServerContextInner>) -> Self {
        Self { inner }
    }

    pub fn analysis(&self) -> &AnalysisLock {
        &self.inner.analysis
    }

    pub fn client(&self) -> &ClientProxy {
        &self.inner.client
    }

    pub fn file_diagnostic(&self) -> &FileDiagnostic {
        &self.inner.file_diagnostic
    }

    pub fn workspace_manager(&self) -> &RwLock<WorkspaceManager> {
        &self.inner.workspace_manager
    }

    pub fn status_bar(&self) -> &StatusBar {
        &self.inner.status_bar
    }

    pub fn lsp_features(&self) -> &LspFeatures {
        &self.inner.lsp_features
    }
}

pub struct ServerContextInner {
    pub analysis: Arc<AnalysisLock>,
    pub client: Arc<ClientProxy>,
    pub file_diagnostic: Arc<FileDiagnostic>,
    pub workspace_manager: Arc<RwLock<WorkspaceManager>>,
    pub status_bar: Arc<StatusBar>,
    pub lsp_features: Arc<LspFeatures>,
}
