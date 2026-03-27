use lsp_types::{ClientCapabilities, DidChangeConfigurationParams, ServerCapabilities};

use crate::{context::ServerContextSnapshot, handlers::initialized::get_client_config};

use super::RegisterCapabilities;

pub async fn on_did_change_configuration(
    context: ServerContextSnapshot,
    params: DidChangeConfigurationParams,
) -> Option<()> {
    let pretty_json = serde_json::to_string_pretty(&params).ok()?;
    log::info!("on_did_change_configuration: {}", pretty_json);

    // Check initialization status and get client config
    let (client_id, supports_config_request) = {
        let workspace_manager = context.workspace_manager().read().await;
        let client_id = workspace_manager.client_config.client_id;
        let supports_config_request = context.lsp_features().supports_config_request();
        (client_id, supports_config_request)
    };

    if client_id.is_vscode() {
        return Some(());
    }

    log::info!("change config client_id: {:?}", client_id);

    // Get new config without holding any locks
    let new_client_config = get_client_config(&context, client_id, supports_config_request).await;

    // Update config and reload - acquire write lock only when necessary
    {
        let mut workspace_manager = context.workspace_manager().write().await;
        if workspace_manager.client_config == new_client_config {
            log::info!("skip workspace reload; client config unchanged");
            return Some(());
        }

        workspace_manager.client_config = new_client_config;
        log::info!("reloading workspace folders");
        workspace_manager.add_reload_workspace_task(context.clone());
    }

    Some(())
}

pub struct ConfigurationCapabilities;

impl RegisterCapabilities for ConfigurationCapabilities {
    fn register_capabilities(_: &mut ServerCapabilities, _: &ClientCapabilities) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use crate::{
        context::{ClientId, ServerContext},
        handlers::ClientConfig,
    };
    use emmylua_code_analysis::WorkspaceFolder;
    use lsp_server::{Connection, Message};
    use lsp_types::{DidChangeWatchedFilesClientCapabilities, WorkspaceClientCapabilities};

    static TEST_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn dynamic_watch_capabilities() -> ClientCapabilities {
        ClientCapabilities {
            workspace: Some(WorkspaceClientCapabilities {
                did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                    dynamic_registration: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn collect_watch_registration_methods(client: &Connection, timeout: Duration) -> Vec<String> {
        let mut methods = Vec::new();
        let deadline = Instant::now() + timeout;

        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match client.receiver.recv_timeout(remaining) {
                Ok(Message::Request(request))
                    if request.method == "client/registerCapability"
                        || request.method == "client/unregisterCapability" =>
                {
                    methods.push(request.method);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }

        methods
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unchanged_config_notification_does_not_reload_workspace() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = TEST_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let workspace_root = std::env::temp_dir().join(format!(
            "emmylua-config-handler-{}-{}-{}",
            std::process::id(),
            unique,
            counter,
        ));
        fs::create_dir_all(&workspace_root).unwrap();

        let (server, client) = Connection::memory();
        let context = ServerContext::new(server, dynamic_watch_capabilities());
        let snapshot = context.snapshot();
        {
            let mut workspace_manager = snapshot.workspace_manager().write().await;
            workspace_manager.workspace_folders =
                vec![WorkspaceFolder::new(workspace_root.clone(), false)];
            workspace_manager.client_config = ClientConfig {
                client_id: ClientId::Other,
                encoding: "utf-8".to_string(),
                ..Default::default()
            };
        }

        on_did_change_configuration(
            snapshot.clone(),
            DidChangeConfigurationParams {
                settings: serde_json::Value::Null,
            },
        )
        .await;

        assert!(collect_watch_registration_methods(&client, Duration::from_millis(250)).is_empty());

        snapshot
            .file_diagnostic()
            .cancel_workspace_diagnostic()
            .await;
        context.close().await;
        fs::remove_dir_all(workspace_root).unwrap();
    }
}
