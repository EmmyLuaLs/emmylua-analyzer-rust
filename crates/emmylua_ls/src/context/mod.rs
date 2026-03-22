mod client;
mod client_id;
mod file_diagnostic;
mod lsp_features;
mod snapshot;
mod status_bar;
mod workspace_manager;

pub use client::ClientProxy;
pub use client_id::{ClientId, get_client_id};
use emmylua_code_analysis::EmmyLuaAnalysis;
pub use file_diagnostic::FileDiagnostic;
pub use lsp_features::LspFeatures;
use lsp_server::{Connection, ErrorCode, Message, RequestId, Response};
use lsp_types::ClientCapabilities;
pub use snapshot::ServerContextSnapshot;
pub use status_bar::ProgressTask;
pub use status_bar::StatusBar;
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
pub use workspace_manager::*;

use crate::context::snapshot::ServerContextInner;

// ============================================================================
// LOCK ORDERING GUIDELINES (CRITICAL - Must Follow to Avoid Deadlocks)
// ============================================================================
//
// This module uses multiple locks (RwLock and Mutex) for concurrent access to shared state.
// To prevent deadlocks, **ALL code must acquire locks in the following order**:
//
// ## Global Lock Order (Low to High Priority):
// 1. **diagnostic_tokens** (Mutex) - File diagnostic task tokens
// 2. **workspace_diagnostic_token** (Mutex) - Workspace diagnostic task token
// 3. **config_reload_token / reindex_token** (Mutex) - Debounced workspace tasks
// 4. **reload_lock** (tokio::Mutex) - Serializes full workspace reloads
// 5. **analysis** (RwLock - READ) - Read-only access to EmmyLuaAnalysis
// 6. **workspace_manager** (RwLock - READ) - Read-only access to WorkspaceManager
// 7. **workspace_manager** (RwLock - WRITE) - Exclusive access to WorkspaceManager
// 8. **analysis** (RwLock - WRITE) - Exclusive access to EmmyLuaAnalysis
//
// ## Lock Ordering Rules:
// - **NEVER acquire a lower-priority lock while holding a higher-priority lock**
// - **ALWAYS release locks in reverse order (LIFO) or use explicit scope blocks**
// - **NEVER upgrade a read lock to a write lock (release read, then acquire write)**
// - **Minimize lock scope**: only hold locks for the minimum necessary time
// - **Avoid holding locks across `.await` points when possible**
// - **NEVER call async methods that might acquire locks while holding a lock**
//
// ## Examples:
//
// ### ✅ CORRECT - Proper lock ordering:
// ```rust
// // Acquire workspace_manager read lock first, then release before analysis write
// let should_process = {
//     let workspace_manager = context.workspace_manager().read().await;
//     workspace_manager.is_workspace_file(&uri)
// };
// if should_process {
//     let mut analysis = context.analysis().write().await;
//     analysis.update_file(&uri, text);
// }
// ```
//
// ### ❌ WRONG - ABBA deadlock risk:
// ```rust
// let mut analysis = context.analysis().write().await;  // Lock A
// // ... operations ...
// let workspace = context.workspace_manager().write().await;  // Lock B (while holding A!)
// // DEADLOCK RISK: Another thread might hold B and wait for A
// ```
//
// ### ✅ CORRECT - Release before calling async methods:
// ```rust
// let data = {
//     let workspace = context.workspace_manager().read().await;
//     workspace.get_config().clone()  // Clone data
// }; // Lock released
// init_analysis(data).await;  // Safe to call async method
// ```
//
// ### ❌ WRONG - Holding lock while calling async method:
// ```rust
// let workspace = context.workspace_manager().write().await;
// workspace.reload_workspace().await;  // May acquire analysis lock internally!
// ```
//
// ## Atomic Operations (Lock-Free):
// The following atomics can be accessed without lock ordering concerns:
// - `workspace_initialized` (AtomicBool)
// - `workspace_diagnostic_level` (AtomicU8)
// - `workspace_version` (AtomicI64)
//
// ## Notes:
// - Use `drop(lock_guard)` explicitly to release locks early when needed
// - Use scope blocks `{ ... }` to limit lock lifetime
// - When in doubt, release all locks before performing complex operations
// - If you need to modify this ordering, update this documentation AND review all call sites
// ============================================================================

pub struct ServerContext {
    #[allow(unused)]
    conn: Connection,
    cancellations: Arc<Mutex<HashMap<RequestId, CancellationToken>>>,
    inner: Arc<ServerContextInner>,
}

impl ServerContext {
    pub fn new(conn: Connection, client_capabilities: ClientCapabilities) -> Self {
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
        let lsp_features = Arc::new(LspFeatures::new(client_capabilities));
        let workspace_manager = Arc::new(RwLock::new(WorkspaceManager::new(
            analysis.clone(),
            client.clone(),
            file_diagnostic.clone(),
            lsp_features.clone(),
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
                lsp_features,
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
