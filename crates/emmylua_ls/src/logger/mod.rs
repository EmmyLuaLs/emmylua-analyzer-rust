use std::{env, fs, path::PathBuf};

use chrono::Local;
use emmylua_code_analysis::file_path_to_uri;
use fern::Dispatch;
use log::{info, LevelFilter};

use crate::cmd_args::{CmdArgs, LogLevel};

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn init_logger(root: Option<&str>, cmd_args: &CmdArgs) {
    let level = match cmd_args.log_level {
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
    };

    if root.is_none() {
        init_stderr_logger(level);
        return;
    }
    let root = root.unwrap();

    let filename = if root.is_empty() || root == "/" {
        "root".to_string()
    } else {
        root.trim_start_matches('/')
            .split(|c| c == '/' || c == '\\' || c == ':')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_")
    };

    let exe_path = env::current_exe().unwrap();
    let exe_dir = exe_path.parent().unwrap();
    let log_dir = if cmd_args.log_path.is_empty() {
        exe_dir.join("logs")
    } else {
        PathBuf::from(cmd_args.log_path.as_str())
    };
    if !log_dir.exists() {
        match fs::create_dir_all(&log_dir) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to create log directory: {:?}", e);
                init_stderr_logger(level);
                return;
            }
        }
    }

    let log_file_path = log_dir.join(format!("{}.log", filename));

    let log_file = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open log file: {:?}", e);
            init_stderr_logger(level);
            return;
        }
    };

    let logger = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S %:z"),
                record.level(),
                record.target(),
                message
            ))
        })
        // set level
        .level(level)
        // set output
        .chain(log_file);

    if let Err(e) = logger.apply() {
        eprintln!("Failed to apply logger: {:?}", e);
        return;
    }

    let uri = file_path_to_uri(&log_file_path).unwrap();
    eprintln!("init logger success with file: {}", uri.as_str());
    info!("{} v{}", CRATE_NAME, CRATE_VERSION);
}

fn init_stderr_logger(level: LevelFilter) {
    let logger = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S %:z"),
                record.level(),
                record.target(),
                message
            ))
        })
        // set level
        .level(level)
        // set output
        .chain(std::io::stderr());

    if let Err(e) = logger.apply() {
        eprintln!("Failed to apply logger: {:?}", e);
        return;
    }

    info!("{} v{}", CRATE_NAME, CRATE_VERSION);
}
