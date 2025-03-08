use structopt::StructOpt;

#[allow(unused)]
#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "emmylua-ls", about = "EmmyLua Language Server")]
pub struct CmdArgs {
    /// Communication method
    #[structopt(
        long = "communication",
        short = "c",
        help = "Communication method",
        default_value = "stdio"
    )]
    pub communication: Communication,

    /// IP address to listen on (only valid when using TCP)
    #[structopt(
        long = "ip",
        help = "IP address to listen on",
        default_value = "127.0.0.1"
    )]
    pub ip: String,

    /// Port number to listen on (only valid when using TCP)
    #[structopt(
        long = "port",
        help = "Port number to listen on",
        default_value = "5007"
    )]
    pub port: u16,

    /// Path to the log file
    #[structopt(long = "log-path", help = "Path to the log file", default_value = "")]
    pub log_path: String,

    /// Logging level (e.g., "error", "warn", "info", "debug", "trace")
    #[structopt(long = "log-level", help = "Logging level", default_value = "info")]
    pub log_level: LogLevel,
}

/// Logging level enum
#[derive(Debug, StructOpt, Clone, Copy)]
#[structopt(rename_all = "lowercase")]
pub enum LogLevel {
    /// Error level
    Error,
    /// Warning level
    Warn,
    /// Info level
    Info,
    /// Debug level
    Debug,
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(input: &str) -> Result<LogLevel, Self::Err> {
        match input.to_lowercase().as_str() {
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            _ => Err(format!(
                "Invalid log level: '{}'. Please choose 'error', 'warn', 'info', 'debug'",
                input
            )),
        }
    }
}

#[derive(Debug, StructOpt, Clone, Copy)]
#[structopt(rename_all = "lowercase")]
pub enum Communication {
    Stdio,
    Tcp,
}

impl std::str::FromStr for Communication {
    type Err = String;

    fn from_str(input: &str) -> Result<Communication, Self::Err> {
        match input.to_lowercase().as_str() {
            "stdio" => Ok(Communication::Stdio),
            "tcp" => Ok(Communication::Tcp),
            _ => Err(format!(
                "Invalid communication method: '{}'. Please choose 'stdio', 'tcp'",
                input
            )),
        }
    }
}
