#[cfg(feature = "cli")]
use clap::{Parser, ValueEnum};

use std::path::PathBuf;

#[allow(unused)]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "cli", derive(Parser))]
#[cfg_attr(feature = "cli", command(version))]
pub struct CmdArgs {
    /// Configuration file paths.
    /// If not provided, both ".emmyrc.json" and ".luarc.json" will be searched in the workspace
    /// directory
    #[cfg_attr(feature = "cli", arg(short, long, value_delimiter = ','))]
    pub config: Option<Vec<PathBuf>>,

    /// Path to the workspace directory
    pub workspace: PathBuf,

    /// Comma separated list of ignore patterns.
    /// Patterns must follow glob syntax
    #[cfg_attr(feature = "cli", arg(short, long, value_delimiter = ','))]
    pub ignore: Option<Vec<String>>,

    /// Specify output format
    #[cfg_attr(
        feature = "cli",
        arg(long, default_value = "text", value_enum, ignore_case = true)
    )]
    pub output_format: OutputFormat,

    /// Specify output destination (stdout or a file path, only used when output_format is json).
    #[cfg_attr(feature = "cli", arg(long, default_value = "stdout"))]
    pub output: OutputDestination,

    /// Treat warnings as errors
    #[cfg_attr(feature = "cli", arg(long))]
    pub warnings_as_errors: bool,

    /// Verbose output
    #[cfg_attr(feature = "cli", arg(long))]
    pub verbose: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "cli", derive(ValueEnum))]
pub enum OutputFormat {
    Json,
    Text,
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
