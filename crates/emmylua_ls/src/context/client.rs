use std::{
    collections::HashMap,
    sync::{atomic::AtomicI32, Arc},
};

use lsp_server::{Connection, Message, Notification, RequestId, Response};
use lsp_types::{
    ApplyWorkspaceEditParams, ApplyWorkspaceEditResponse, ConfigurationParams,
    PublishDiagnosticsParams, RegistrationParams, ShowMessageParams, UnregistrationParams,
};
use serde::de::DeserializeOwned;
use tokio::{
    select,
    sync::{oneshot, Mutex},
};
use tokio_util::sync::CancellationToken;

pub struct ClientProxy {
    conn: Connection,
    id_counter: AtomicI32,
    response_manager: Arc<Mutex<HashMap<RequestId, oneshot::Sender<Response>>>>,
}

#[allow(unused)]
impl ClientProxy {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            id_counter: AtomicI32::new(0),
            response_manager: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn send_notification(&self, method: &str, params: impl serde::Serialize) {
        let _ = self.conn.sender.send(Message::Notification(Notification {
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap(),
        }));
    }

    async fn send_request(
        &self,
        id: RequestId,
        method: &str,
        params: impl serde::Serialize,
        cancel_token: CancellationToken,
    ) -> Option<Response> {
        let (sender, receiver) = oneshot::channel();
        self.response_manager
            .lock()
            .await
            .insert(id.clone(), sender);
        let _ = self.conn.sender.send(Message::Request(lsp_server::Request {
            id: id.clone(),
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap(),
        }));
        let response = select! {
            response = receiver => response.ok(),
            _ = cancel_token.cancelled() => None,
        };
        self.response_manager.lock().await.remove(&id);
        response
    }

    fn send_request_no_wait(&self, id: RequestId, method: &str, params: impl serde::Serialize) {
        let _ = self.conn.sender.send(Message::Request(lsp_server::Request {
            id,
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap(),
        }));
    }

    pub async fn on_response(&self, response: Response) -> Option<()> {
        let sender = self.response_manager.lock().await.remove(&response.id)?;
        let _ = sender.send(response);
        Some(())
    }

    fn next_id(&self) -> RequestId {
        let id = self
            .id_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        id.into()
    }

    pub async fn get_configuration<C>(
        &self,
        params: ConfigurationParams,
        cancel_token: CancellationToken,
    ) -> Option<Vec<C>>
    where
        C: DeserializeOwned,
    {
        let request_id = self.next_id();
        let response = self
            .send_request(request_id, "workspace/configuration", params, cancel_token)
            .await?;
        serde_json::from_value(response.result?).ok()
    }

    pub fn dynamic_register_capability(&self, registration_param: RegistrationParams) {
        let request_id = self.next_id();
        self.send_request_no_wait(request_id, "client/registerCapability", registration_param);
    }

    pub fn dynamic_unregister_capability(&self, registration_param: UnregistrationParams) {
        let request_id = self.next_id();
        self.send_request_no_wait(
            request_id,
            "client/unregisterCapability",
            registration_param,
        );
    }

    pub fn show_message(&self, message: ShowMessageParams) {
        self.send_notification("window/showMessage", message);
    }

    pub fn publish_diagnostics(&self, params: PublishDiagnosticsParams) {
        self.send_notification("textDocument/publishDiagnostics", params);
    }

    pub async fn apply_edit(
        &self,
        params: ApplyWorkspaceEditParams,
        cancel_token: CancellationToken,
    ) -> Option<ApplyWorkspaceEditResponse> {
        let request_id = self.next_id();
        let r = self
            .send_request(request_id, "workspace/applyEdit", params, cancel_token)
            .await?;
        serde_json::from_value(r.result?).ok()
    }
}
