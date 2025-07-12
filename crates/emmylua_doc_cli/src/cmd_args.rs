use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct CmdArgs {
    /// Configuration file paths.
    /// If not provided, both ".emmyrc.json" and ".luarc.json" will be searched in the workspace
    /// directory
    #[arg(short, long, value_delimiter = ',')]
    pub config: Option<Vec<PathBuf>>,

    /// Deprecated, use [WORKSPACE] instead
    #[arg(short, long, num_args = 1..)]
    pub input: Vec<PathBuf>,

    /// Path to the workspace directory
    #[arg(num_args = 1..)]
    pub workspace: Vec<PathBuf>,

    /// Comma separated list of ignore patterns.
    /// Patterns must follow glob syntax
    #[arg(long, value_delimiter = ',')]
    pub ignore: Option<Vec<String>>,

    /// Specify output format
    #[arg(
        long,
        short = 'f',
        default_value = "markdown",
        value_enum,
        ignore_case = true
    )]
    pub output_format: Format,

    /// Deprecated, use --output-format instead
    #[arg(long, value_enum, ignore_case = true)]
    pub format: Option<Format>,

    /// Specify output destination (can be stdout when output_format is json)
    #[arg(long, short, default_value = "./output")]
    pub output: OutputDestination,

    /// The path of the override template
    #[arg(long)]
    pub override_template: Option<PathBuf>,

    #[arg(long, default_value = "Docs")]
    pub site_name: Option<String>,

    /// The path of the mixin md file
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
