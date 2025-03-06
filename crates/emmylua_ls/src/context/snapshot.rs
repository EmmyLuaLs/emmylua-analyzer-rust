use std::sync::Arc;
use tokio::sync::RwLock;

use emmylua_code_analysis::EmmyLuaAnalysis;

use super::{
    client::ClientProxy, workspace_manager::WorkspaceManager, file_diagnostic::FileDiagnostic,
    status_bar::StatusBar,
};

#[derive(Clone)]
pub struct ServerContextSnapshot {
    pub analysis: Arc<RwLock<EmmyLuaAnalysis>>,
    pub client: Arc<ClientProxy>,
    pub file_diagnostic: Arc<FileDiagnostic>,
    pub workspace_manager: Arc<RwLock<WorkspaceManager>>,
    pub status_bar: Arc<StatusBar>,
}
