use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct CmdArgs {
    /// The path of the lua project
    #[arg(long, short, num_args = 1..)]
    pub input: Vec<PathBuf>,

    /// Format of the output, default is Markdown
    #[arg(long, short, default_value = "markdown")]
    pub format: Format,

    /// The output path of the docs file
    #[arg(long, short, default_value = "./output")]
    pub output: PathBuf,

    /// The path of the override template
    #[arg(long)]
    pub override_template: Option<PathBuf>,

    /// The path of the mixin md file
    #[arg(long)]
    pub mixin: Option<PathBuf>,
}

#[derive(Debug, Clone, Eq, PartialEq, ValueEnum)]
pub enum Format {
    Markdown,
    Json,
}

impl std::str::FromStr for Format {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            _ => Err("Invalid format, must be one of markdown, json"),
        }
    }
}
