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
