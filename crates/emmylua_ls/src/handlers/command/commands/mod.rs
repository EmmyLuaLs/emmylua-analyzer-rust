use std::sync::LazyLock;

use emmy_add_doc_tag::AddDocTagCommand;
use emmy_auto_require::AutoRequireCommand;
use emmy_disable_code::DisableCodeCommand;
use emmy_fix_format::FixFormatCommand;
use serde_json::Value;

use crate::context::ServerContextSnapshot;

mod emmy_add_doc_tag;
mod emmy_auto_require;
mod emmy_disable_code;
mod emmy_fix_format;

pub use emmy_add_doc_tag::make_auto_doc_tag_command;
pub use emmy_auto_require::make_auto_require;
pub use emmy_disable_code::{make_disable_code_command, DisableAction};

pub trait CommandSpec {
    const COMMAND: &str;

    async fn handle(context: ServerContextSnapshot, args: Vec<Value>) -> Option<()>;
}

static COMMANDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        AutoRequireCommand::COMMAND.to_string(),
        DisableCodeCommand::COMMAND.to_string(),
        FixFormatCommand::COMMAND.to_string(),
        AddDocTagCommand::COMMAND.to_string(),
    ]
});

pub fn get_commands_list() -> Vec<String> {
    COMMANDS.clone()
}

pub async fn dispatch_command(
    context: ServerContextSnapshot,
    command_name: &str,
    args: Vec<Value>,
) -> Option<()> {
    match command_name {
        AutoRequireCommand::COMMAND => AutoRequireCommand::handle(context, args).await,
        DisableCodeCommand::COMMAND => DisableCodeCommand::handle(context, args).await,
        FixFormatCommand::COMMAND => FixFormatCommand::handle(context, args).await,
        AddDocTagCommand::COMMAND => AddDocTagCommand::handle(context, args).await,
        _ => Some(()),
    }
}
