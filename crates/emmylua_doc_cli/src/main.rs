use crate::cmd_args::Format;
use clap::Parser;
use cmd_args::CmdArgs;
use std::process::exit;

mod cmd_args;
mod common;
mod init;
mod json_generator;
mod markdown_generator;

fn main() {
    let args = CmdArgs::parse();
    let current_path = std::env::current_dir().ok().unwrap();
    let input = args.input;
    let mut files: Vec<String> = Vec::new();
    for path in &input {
        if path.is_relative() {
            let abs_path = current_path.join(path).to_str().unwrap().to_string();
            files.push(abs_path);
        } else {
            files.push(path.to_str().unwrap().to_string());
        }
    }

    let analysis = init::load_workspace(files);
    if let Some(mut analysis) = analysis {
        let res = match args.format {
            Format::Markdown => markdown_generator::generate_markdown(
                &mut analysis,
                args.output,
                args.override_template,
                args.mixin,
            ),
            Format::Json => json_generator::generate_json(&mut analysis, args.output),
        };

        if let Err(err) = res {
            eprintln!("Error: {}", err);
            exit(1);
        }
    } else {
        eprintln!("Analysis failed.");
        exit(1);
    }
}
