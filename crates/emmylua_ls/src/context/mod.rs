mod client;
mod client_id;
mod file_diagnostic;
mod snapshot;
mod status_bar;
mod workspace_manager;

pub use client::ClientProxy;
pub use client_id::{ClientId, get_client_id};
use emmylua_code_analysis::EmmyLuaAnalysis;
pub use file_diagnostic::FileDiagnostic;
use lsp_server::{Connection, ErrorCode, Message, RequestId, Response};
use lsp_types::ClientCapabilities;
pub use snapshot::ServerContextSnapshot;
pub use status_bar::ProgressTask;
pub use status_bar::StatusBar;
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
pub use workspace_manager::WorkspaceFileMatcher;
pub use workspace_manager::WorkspaceManager;
pub use workspace_manager::load_emmy_config;

use crate::context::snapshot::ServerContextInner;

pub struct ServerContext {
    #[allow(unused)]
    conn: Connection,
    cancellations: Arc<Mutex<HashMap<RequestId, CancellationToken>>>,
    inner: Arc<ServerContextInner>,
}

impl ServerContext {
    pub fn new(conn: Connection, client_capabilities: Arc<ClientCapabilities>) -> Self {
        let client = Arc::new(ClientProxy::new(Connection {
            sender: conn.sender.clone(),
            receiver: conn.receiver.clone(),
        }));

        let analysis = Arc::new(RwLock::new(EmmyLuaAnalysis::new()));
        let status_bar = Arc::new(StatusBar::new(client.clone()));
        let file_diagnostic = Arc::new(FileDiagnostic::new(
            analysis.clone(),
            status_bar.clone(),
            client.clone(),
        ));
        let workspace_manager = Arc::new(RwLock::new(WorkspaceManager::new(
            analysis.clone(),
            client.clone(),
            status_bar.clone(),
            file_diagnostic.clone(),
        )));

        ServerContext {
            conn,
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            inner: Arc::new(ServerContextInner {
                analysis,
                client,
                file_diagnostic,
                workspace_manager,
                status_bar,
                client_capabilities,
            }),
        }
    }

    pub fn snapshot(&self) -> ServerContextSnapshot {
        ServerContextSnapshot::new(self.inner.clone())
    }

    pub fn send(&self, response: Response) {
        let _ = self.conn.sender.send(Message::Response(response));
    }

    pub async fn task<F, Fut>(&self, req_id: RequestId, exec: F)
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Option<Response>> + Send + 'static,
    {
        let cancel_token = CancellationToken::new();

        {
            let mut cancellations = self.cancellations.lock().await;
            cancellations.insert(req_id.clone(), cancel_token.clone());
        }

        let sender = self.conn.sender.clone();
        let cancellations = self.cancellations.clone();

        tokio::spawn(async move {
            let res = exec(cancel_token.clone()).await;
            if cancel_token.is_cancelled() {
                let response = Response::new_err(
                    req_id.clone(),
                    ErrorCode::RequestCanceled as i32,
                    "cancel".to_string(),
                );
                let _ = sender.send(Message::Response(response));
            } else if res.is_none() {
                let response = Response::new_err(
                    req_id.clone(),
                    ErrorCode::InternalError as i32,
                    "internal error".to_string(),
                );
                let _ = sender.send(Message::Response(response));
            } else if let Some(it) = res {
                let _ = sender.send(Message::Response(it));
            }

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
        let mut workspace_manager = self.inner.workspace_manager.write().await;
        workspace_manager.watcher = None;
    }

    pub async fn send_response(&self, response: Response) {
        self.inner.client.on_response(response).await;
    }
}
