use std::{
    fs,
    io::{self, Read, Write},
    process::exit,
};

use clap::Parser;
use emmylua_formatter::{
    cmd_args, collect_lua_files, default_config_toml, format_file, format_text_for_path,
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn read_stdin_to_string() -> io::Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn main() {
    let args = cmd_args::CliArgs::parse();

    if args.dump_default_config {
        match default_config_toml() {
            Ok(config) => {
                println!("{config}");
                exit(0);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                exit(2);
            }
        }
    }

    let mut exit_code = 0;

    let is_stdin = args.stdin || args.paths.is_empty();

    if is_stdin {
        let content = match read_stdin_to_string() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read stdin: {e}");
                exit(2);
            }
        };

        let result = match format_text_for_path(&content, None, args.config.as_deref()) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("Error: {err}");
                exit(2);
            }
        };
        let changed = result.output.changed;

        if args.check || args.list_different {
            if changed {
                exit_code = 1;
            }
        } else if let Some(out) = &args.output {
            if let Err(e) = fs::write(out, result.output.formatted) {
                eprintln!("Failed to write output to {out:?}: {e}");
                exit(2);
            }
        } else if args.write {
            eprintln!("--write with stdin requires --output <FILE>");
            exit(2);
        } else {
            let mut stdout = io::stdout();
            if let Err(e) = stdout.write_all(result.output.formatted.as_bytes()) {
                eprintln!("Failed to write to stdout: {e}");
                exit(2);
            }
        }

        exit(exit_code);
    }

    if args.output.is_some() && args.paths.len() != 1 {
        eprintln!("--output can only be used with a single input or stdin");
        exit(2);
    }

    let file_options = cmd_args::build_file_collector_options(&args);
    let files = match collect_lua_files(&args.paths, &file_options) {
        Ok(files) => files,
        Err(err) => {
            eprintln!("Error: {err}");
            exit(2);
        }
    };

    if files.len() > 1 && !(args.write || args.check || args.list_different) {
        eprintln!("Multiple matched files require --write, --check, or --list-different");
        exit(2);
    }

    if files.is_empty() {
        eprintln!("No Lua files matched the provided inputs");
        exit(2);
    }

    let mut different_paths: Vec<String> = Vec::new();

    for path in &files {
        match format_file(path, args.config.as_deref()) {
            Ok(result) => {
                let output = result.output;

                if args.check || args.list_different {
                    if output.changed {
                        exit_code = 1;
                        if args.list_different {
                            different_paths.push(path.to_string_lossy().to_string());
                        }
                    }
                } else if args.write {
                    if output.changed
                        && let Err(e) = fs::write(path, output.formatted)
                    {
                        eprintln!("Failed to write {}: {e}", path.to_string_lossy());
                        exit_code = 2;
                    }
                } else if let Some(out) = &args.output {
                    if let Err(e) = fs::write(out, output.formatted) {
                        eprintln!("Failed to write output to {out:?}: {e}");
                        exit(2);
                    }
                } else {
                    let mut stdout = io::stdout();
                    if let Err(e) = stdout.write_all(output.formatted.as_bytes()) {
                        eprintln!("Failed to write to stdout: {e}");
                        exit(2);
                    }
                }
            }
            Err(err) => {
                eprintln!("Failed to format {}: {err}", path.to_string_lossy());
                exit_code = 2;
            }
        }
    }

    if args.list_different && !different_paths.is_empty() {
        for p in different_paths {
            println!("{p}");
        }
    }

    exit(exit_code);
}
