mod neovim_config;
mod vscode_config;

use neovim_config::get_client_config_neovim;
use serde_json::Value;
use vscode_config::get_client_config_vscode;

use crate::context::{ClientId, ServerContextSnapshot};

#[allow(unused)]
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    pub client_id: ClientId,
    pub exclude: Vec<String>,
    pub extensions: Vec<String>,
    pub encoding: String,
    pub partial_emmyrcs: Option<Vec<Value>>,
}

pub async fn get_client_config(
    context: &ServerContextSnapshot,
    client_id: ClientId,
) -> ClientConfig {
    let mut config = ClientConfig {
        client_id,
        exclude: Vec::new(),
        extensions: Vec::new(),
        encoding: "utf-8".to_string(),
        partial_emmyrcs: None,
    };
    match client_id {
        ClientId::VSCode => get_client_config_vscode(context, &mut config).await,
        ClientId::Neovim => get_client_config_neovim(context, &mut config).await,
        _ => Some(()),
    };

    config
}
