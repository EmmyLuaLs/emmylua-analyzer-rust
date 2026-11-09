use std::sync::Arc;
use tokio::sync::Mutex;

use crate::context::{UpdateEvent, lsp_features::LspFeatures};

use super::{
    AnalysisState, RequestManager, client::ClientProxy, diagnostic_service::DiagnosticService,
    status_bar::StatusBar, workspace_manager::WorkspaceManager,
};

#[derive(Clone)]
pub struct ServerContextSnapshot {
    inner: Arc<ServerContextInner>,
}

impl ServerContextSnapshot {
    pub fn new(inner: Arc<ServerContextInner>) -> Self {
        Self { inner }
    }

    pub fn analysis(&self) -> &AnalysisState {
        &self.inner.analysis
    }

    pub fn client(&self) -> &ClientProxy {
        &self.inner.client
    }

    pub fn file_diagnostic(&self) -> &DiagnosticService {
        &self.inner.file_diagnostic
    }

    pub fn workspace_manager(&self) -> &Mutex<WorkspaceManager> {
        &self.inner.workspace_manager
    }

    pub fn status_bar(&self) -> &StatusBar {
        &self.inner.status_bar
    }

    pub fn lsp_features(&self) -> &LspFeatures {
        &self.inner.lsp_features
    }

    pub fn request_manager(&self) -> &RequestManager {
        &self.inner.request_manager
    }

    pub fn update_tx(&self) -> &tokio::sync::mpsc::UnboundedSender<UpdateEvent> {
        &self.inner.update_tx
    }
}

pub struct ServerContextInner {
    pub analysis: Arc<AnalysisState>,
    pub client: Arc<ClientProxy>,
    pub file_diagnostic: Arc<DiagnosticService>,
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
    pub status_bar: Arc<StatusBar>,
    pub lsp_features: Arc<LspFeatures>,
    pub request_manager: Arc<RequestManager>,
    pub update_tx: tokio::sync::mpsc::UnboundedSender<UpdateEvent>,
}
