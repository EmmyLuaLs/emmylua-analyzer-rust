use structopt::StructOpt;

#[allow(unused)]
#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "emmylua-check", about = "EmmyLua Check")]
pub struct CmdArgs {
    #[structopt(short, long, parse(from_os_str), help = "Specify configuration file")]
    pub config: Option<std::path::PathBuf>,

    #[structopt(parse(from_os_str), help = "Path to the workspace directory")]
    pub workspace: std::path::PathBuf,

    #[structopt(
        short,
        long,
        help = "Comma separated list of ignore patterns",
        use_delimiter = true
    )]
    pub ignore: Option<Vec<String>>,

    #[structopt(
        long,
        help = "Specify output format (json or text)",
        default_value = "text",
        possible_values = &OutputFormat::variants(),
        case_insensitive = true
    )]
    pub output_format: OutputFormat,

    #[structopt(
        long,
        help = "Specify output destination (stdout or a file path, only used when output_format is json)",
        default_value = "stdout",
        parse(try_from_str)
    )]
    pub output: OutputDestination,

    #[structopt(long, help = "Treat warnings as errors")]
    pub warnings_as_errors: bool,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Json,
    Text,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "text" => Ok(OutputFormat::Text),
            _ => Err(format!("Invalid output format: {}", s)),
        }
    }
}

impl OutputFormat {
    pub fn variants() -> [&'static str; 2] {
        ["json", "text"]
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum OutputDestination {
    Stdout,
    File(std::path::PathBuf),
}

impl std::str::FromStr for OutputDestination {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdout" => Ok(OutputDestination::Stdout),
            _ => Ok(OutputDestination::File(std::path::PathBuf::from(s))),
        }
    }
}
