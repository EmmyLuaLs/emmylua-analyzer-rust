mod analysis_state;
mod client;
mod client_id;
mod diagnostic_service;
mod lsp_features;
mod pull_cache;
mod query_runner;
mod snapshot;
mod status_bar;
mod update_queue;
mod workspace_manager;
mod workspace_state;

pub use analysis_state::AnalysisState;
pub use client::ClientProxy;
pub use client_id::{ClientId, get_client_id};
pub use diagnostic_service::DiagnosticService;
pub use lsp_features::LspFeatures;
use lsp_server::{Connection, ErrorCode, RequestId, Response};
use lsp_types::ClientCapabilities;
pub use pull_cache::RequestManager;
pub use query_runner::{
    CancelSource, CancelStrategy, RequestOutcome, analysis_query, snapshot_query,
};
pub use snapshot::ServerContextSnapshot;
pub use status_bar::ProgressTask;
pub use status_bar::StatusBar;
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
pub use update_queue::{UpdateEvent, spawn_update_queue};
pub use workspace_manager::*;

use crate::context::snapshot::ServerContextInner;

pub struct ServerContext {
    cancellations: Arc<Mutex<HashMap<RequestId, CancellationToken>>>,
    inner: Arc<ServerContextInner>,
}

impl ServerContext {
    pub fn new(conn: Arc<Connection>, client_capabilities: ClientCapabilities) -> Self {
        let client = Arc::new(ClientProxy::new(conn));

        let analysis = Arc::new(AnalysisState::new());
        let lsp_features = Arc::new(LspFeatures::new(client_capabilities));
        let status_bar = Arc::new(StatusBar::new(
            client.clone(),
            lsp_features.supports_work_done_progress(),
        ));
        let file_diagnostic = Arc::new(DiagnosticService::new(
            analysis.clone(),
            status_bar.clone(),
            client.clone(),
        ));
        let workspace_manager = Arc::new(Mutex::new(WorkspaceManager::new(
            client.clone(),
            file_diagnostic.clone(),
            lsp_features.clone(),
        )));

        let (update_tx, update_rx) = tokio::sync::mpsc::unbounded_channel();
        let inner = Arc::new(ServerContextInner {
            analysis,
            client,
            file_diagnostic,
            workspace_manager,
            status_bar,
            lsp_features,
            request_manager: Arc::new(RequestManager::new()),
            update_tx,
        });
        spawn_update_queue(inner.clone(), update_rx);

        ServerContext {
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            inner,
        }
    }

    pub fn snapshot(&self) -> ServerContextSnapshot {
        ServerContextSnapshot::new(self.inner.clone())
    }

    pub fn send(&self, response: Response) {
        self.inner.client.send_response(response);
    }

    pub async fn task<T, F, Fut>(&self, req_id: RequestId, exec: F)
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = RequestOutcome<T>> + Send + 'static,
    {
        let cancel_token = CancellationToken::new();

        {
            let mut cancellations = self.cancellations.lock().await;
            cancellations.insert(req_id.clone(), cancel_token.clone());
        }

        let client = self.inner.client.clone();
        let cancellations = self.cancellations.clone();

        tokio::spawn(async move {
            // Run the handler in a child task: salsa's Cancelled is thrown as a panic,
            // so capture it here via JoinHandle to avoid interrupting the request task without a reply.
            let res = tokio::spawn(exec(cancel_token.clone())).await.ok();

            let response = match res {
                Some(RequestOutcome::Ready(value)) => Response::new_ok(req_id.clone(), value),
                Some(RequestOutcome::Missing) => {
                    Response::new_ok(req_id.clone(), serde_json::Value::Null)
                }
                Some(RequestOutcome::Cancelled(_)) => Response::new_err(
                    req_id.clone(),
                    ErrorCode::RequestCanceled as i32,
                    "cancel".to_string(),
                ),
                None => Response::new_err(
                    req_id.clone(),
                    ErrorCode::InternalError as i32,
                    "internal error".to_string(),
                ),
            };
            client.send_response(response);

            let mut cancellations = cancellations.lock().await;
            cancellations.remove(&req_id);
        });
    }

    pub async fn cancel(&self, req_id: RequestId) {
        let cancellations = self.cancellations.lock().await;
        if let Some(cancel_token) = cancellations.get(&req_id) {
            cancel_token.cancel();
        }
    }

    pub async fn close(&self) {
        let mut workspace_manager = self.inner.workspace_manager.lock().await;
        workspace_manager.watcher = None;
    }

    pub async fn send_response(&self, response: Response) {
        self.inner.client.on_response(response).await;
    }
}
