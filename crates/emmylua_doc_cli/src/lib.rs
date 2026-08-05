pub use crate::cmd_args::Format;
use crate::init::setup_logger;
pub use clap::Parser;
pub use cmd_args::*;

mod cmd_args;
mod common;
mod html_generator;
mod init;
mod json_generator;
mod markdown_generator;

#[allow(unused)]
pub fn run_doc_cli(mut cmd_args: CmdArgs) -> Result<(), Box<dyn std::error::Error>> {
    setup_logger(cmd_args.verbose);

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
        cmd_args.exclude_pattern,
        cmd_args.include_pattern,
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
        Format::Html => {
            html_generator::generate_html(&analysis, cmd_args.output, cmd_args.site_name)
        }
    }
}
