use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version)]
pub struct CmdArgs {
    /// Configuration file paths.
    /// If not provided, both ".emmyrc.json" and ".luarc.json" will be searched in the workspace
    /// directory
    #[arg(short, long, value_delimiter = ',')]
    pub config: Option<Vec<PathBuf>>,

    /// Path to the workspace directory
    #[arg(num_args = 1..)]
    pub workspace: Vec<PathBuf>,

    /// Comma separated list of exclude patterns
    /// Patterns must follow glob syntax
    /// Exclude patterns take precedence over include patterns
    #[arg(long = "exclude", alias = "ignore", value_delimiter = ',')]
    pub exclude_pattern: Option<Vec<String>>,

    /// Comma separated list of include patterns
    /// Patterns must follow glob syntax
    #[arg(long = "include", value_delimiter = ',')]
    pub include_pattern: Option<Vec<String>>,

    /// Specify output format
    #[arg(
        long,
        short = 'f',
        default_value = "html",
        value_enum,
        ignore_case = true
    )]
    pub output_format: Format,

    /// Specify output destination (can be stdout when output_format is json)
    #[arg(long, short, default_value = "./output")]
    pub output: OutputDestination,

    /// The path of the override template
    #[arg(long)]
    pub override_template: Option<PathBuf>,

    #[arg(long, default_value = "Docs")]
    pub site_name: Option<String>,

    /// A directory whose contents are merged with the generated Markdown files.
    /// For example, to override docs/index.md, create a folder called "docs" in
    /// your mixin folder and create a file called "index.md" inside it.
    #[arg(long)]
    pub mixin: Option<PathBuf>,

    /// Verbose output
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, ValueEnum)]
pub enum Format {
    Json,
    Markdown,
    Html,
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum OutputDestination {
    Stdout,
    File(PathBuf),
}

impl std::str::FromStr for OutputDestination {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdout" => Ok(OutputDestination::Stdout),
            _ => Ok(OutputDestination::File(PathBuf::from(s))),
        }
    }
}
