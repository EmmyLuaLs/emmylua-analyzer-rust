use lsp_types::{
    ClientCapabilities, DidChangeConfigurationParams, ServerCapabilities
};

use crate::{context::ServerContextSnapshot, handlers::initialized::get_client_config};

pub async fn on_did_change_configuration(
    context: ServerContextSnapshot,
    params: DidChangeConfigurationParams,
) -> Option<()> {
    let pretty_json = serde_json::to_string_pretty(&params).ok()?;
    log::info!("on_did_change_configuration: {}", pretty_json);

    let config_manager = context.workspace_manager.read().await;
    if config_manager.client_config.client_id.is_vscode() {
        return Some(());
    }
    let client_id = config_manager.client_config.client_id;
    drop(config_manager);

    let new_client_config = get_client_config(&context, client_id).await;
    let mut config_manager = context.workspace_manager.write().await;
    config_manager.client_config = new_client_config;

    config_manager.reload_workspace().await;
    Some(())
}

pub fn register_capabilities(_: &mut ServerCapabilities, _: &ClientCapabilities) -> Option<()> {
    Some(())
}
