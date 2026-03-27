mod default_config;
mod vscode_config;

use default_config::get_client_config_default;
use serde_json::Value;
use vscode_config::get_client_config_vscode;

use crate::context::{ClientId, ServerContextSnapshot};

#[allow(unused)]
#[derive(Debug, Clone, Default, PartialEq)]
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
    supports_config_request: bool,
) -> ClientConfig {
    let mut config = ClientConfig {
        client_id,
        exclude: Vec::new(),
        extensions: Vec::new(),
        encoding: "utf-8".to_string(),
        partial_emmyrcs: None,
    };
    match client_id {
        ClientId::VSCode => {
            get_client_config_vscode(context, &mut config).await;
        }
        ClientId::Neovim => {
            get_client_config_default(context, &mut config, Some(&["Lua", "emmylua"])).await;
        }
        _ if supports_config_request => {
            get_client_config_default(context, &mut config, None).await;
        }
        _ => {}
    };

    config
}
