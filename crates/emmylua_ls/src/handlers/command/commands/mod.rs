use serde_json::Value;

use crate::context::ServerContextSnapshot;

mod emmy_auto_require;
mod emmy_disable_code;
mod emmy_fix_format;

pub use emmy_auto_require::make_auto_require;
pub use emmy_disable_code::{make_disable_code_command, DisableAction};

pub fn get_commands_list() -> Vec<String> {
    let mut commands = Vec::new();
    macro_rules! command_from {
        ($($module:ident),*) => {
            $(
                let command_str = $module::COMMAND.to_string();
                commands.push(command_str);
            )*
        };
    }

    command_from!(emmy_auto_require);
    command_from!(emmy_disable_code);
    command_from!(emmy_fix_format);

    commands
}

macro_rules! command_dispatch {
    ($cmd_name:expr, $context:expr, $args:expr, [ $( $module:ident ),+ ]) => {
        match $cmd_name {
            $(
                $module::COMMAND => {
                    $module::handle($context, $args).await;
                }
            )+
            _ => {}
        }
    };
}

pub async fn dispatch_command(
    context: ServerContextSnapshot,
    command_name: &str,
    args: Vec<Value>,
) -> Option<()> {
    command_dispatch!(
        command_name,
        context,
        args,
        [emmy_auto_require, emmy_disable_code, emmy_fix_format]
    );

    Some(())
}
