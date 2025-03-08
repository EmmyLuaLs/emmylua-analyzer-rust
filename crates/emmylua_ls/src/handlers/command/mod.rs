mod commands;

use crate::context::ServerContextSnapshot;
use commands::get_commands_list;
#[allow(unused)]
pub use commands::*;
use lsp_types::{
    ClientCapabilities, ExecuteCommandOptions, ExecuteCommandParams, ServerCapabilities,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub async fn on_execute_command_handler(
    context: ServerContextSnapshot,
    params: ExecuteCommandParams,
    _: CancellationToken,
) -> Option<Value> {
    let args = params.arguments;
    let command_name = params.command.as_str();
    commands::dispatch_command(context, command_name, args).await;
    Some(Value::Null)
}

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    _: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.execute_command_provider = Some(ExecuteCommandOptions {
        commands: get_commands_list(),
        ..Default::default()
    });
    Some(())
}
