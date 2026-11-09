mod commands;

use crate::context::{RequestOutcome, ServerContextSnapshot};
use commands::get_commands_list;
#[allow(unused)]
pub use commands::*;
use lsp_types::{
    ClientCapabilities, ExecuteCommandOptions, ExecuteCommandParams, ServerCapabilities,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;

pub async fn on_execute_command_handler(
    context: ServerContextSnapshot,
    params: ExecuteCommandParams,
    _: CancellationToken,
) -> RequestOutcome<Value> {
    let args = params.arguments;
    let command_name = params.command.as_str();
    dispatch_command(context, command_name, args).await;
    RequestOutcome::Ready(Value::Null)
}

pub struct CommandCapabilities;

impl RegisterCapabilities for CommandCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.execute_command_provider = Some(ExecuteCommandOptions {
            commands: get_commands_list(),
            ..Default::default()
        });
    }
}
