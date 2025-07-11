pub use crate::cmd_args::Format;
use crate::init::setup_logger;
pub use clap::Parser;
pub use cmd_args::*;

mod cmd_args;
mod common;
mod init;
mod json_generator;
mod markdown_generator;

#[allow(unused)]
pub fn run_doc_cli(mut cmd_args: CmdArgs) -> Result<(), Box<dyn std::error::Error>> {
    setup_logger(cmd_args.verbose);

    if !cmd_args.input.is_empty() {
        log::warn!("`--input` is deprecated, please use `workspace` instead");
        cmd_args.workspace.append(&mut cmd_args.input);
    }

    if let Some(format) = cmd_args.format {
        log::warn!("`--format` is deprecated, please use `--output-format` instead");
        cmd_args.output_format = format;
    }

    let cwd = std::env::current_dir()?;
    let workspaces: Vec<_> = cmd_args
        .workspace
        .into_iter()
        .map(|workspace| {
            if workspace.is_absolute() {
                workspace
            } else {
                cwd.join(workspace)
            }
        })
        .collect();
    let main_path = workspaces
        .first()
        .ok_or("Failed to load workspace")?
        .clone();

    let analysis = match init::load_workspace(
        main_path.clone(),
        workspaces.clone(),
        cmd_args.config,
        cmd_args.ignore,
    ) {
        Some(analysis) => analysis,
        None => {
            return Err("Failed to load workspace".into());
        }
    };

    match cmd_args.output_format {
        Format::Markdown => markdown_generator::generate_markdown(
            &analysis,
            cmd_args.output,
            cmd_args.override_template,
            cmd_args.site_name,
            cmd_args.mixin,
        ),
        Format::Json => json_generator::generate_json(&analysis, cmd_args.output),
    }
}
