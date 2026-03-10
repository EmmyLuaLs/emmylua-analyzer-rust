use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    process::exit,
};

use clap::Parser;
use emmylua_formatter::{LuaCodeStyle, cmd_args, reformat_lua_code};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn read_stdin_to_string() -> io::Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn format_content(content: &str, style: &LuaCodeStyle) -> String {
    reformat_lua_code(content, style)
}

#[allow(unused)]
fn process_file(
    path: &PathBuf,
    style: &LuaCodeStyle,
    write: bool,
    list_diff: bool,
) -> io::Result<(bool, Option<String>)> {
    let original = fs::read_to_string(path)?;
    let formatted = format_content(&original, style);
    let changed = formatted != original;

    if write && changed {
        fs::write(path, formatted)?;
        return Ok((true, None));
    }

    if list_diff && changed {
        return Ok((true, Some(path.to_string_lossy().to_string())));
    }

    Ok((changed, None))
}

fn main() {
    let args = cmd_args::CliArgs::parse();

    let mut exit_code = 0;

    let style = match cmd_args::resolve_style(&args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            exit(2);
        }
    };

    let is_stdin = args.stdin || args.paths.is_empty();

    if is_stdin {
        let content = match read_stdin_to_string() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read stdin: {e}");
                exit(2);
            }
        };

        let formatted = format_content(&content, &style);
        let changed = formatted != content;

        if args.check || args.list_different {
            if changed {
                exit_code = 1;
            }
        } else if let Some(out) = &args.output {
            if let Err(e) = fs::write(out, formatted) {
                eprintln!("Failed to write output to {out:?}: {e}");
                exit(2);
            }
        } else if args.write {
            eprintln!("--write with stdin requires --output <FILE>");
            exit(2);
        } else {
            let mut stdout = io::stdout();
            if let Err(e) = stdout.write_all(formatted.as_bytes()) {
                eprintln!("Failed to write to stdout: {e}");
                exit(2);
            }
        }

        exit(exit_code);
    }

    if args.paths.len() > 1 && args.output.is_some() {
        eprintln!("--output can only be used with a single input or stdin");
        exit(2);
    }

    if args.paths.len() > 1 && !(args.write || args.check || args.list_different) {
        eprintln!("Multiple inputs require --write or --check");
        exit(2);
    }

    let mut different_paths: Vec<String> = Vec::new();

    for path in &args.paths {
        match fs::metadata(path) {
            Ok(meta) => {
                if !meta.is_file() {
                    eprintln!("Skipping non-file path: {}", path.to_string_lossy());
                    continue;
                }
            }
            Err(e) => {
                eprintln!("Cannot access {}: {e}", path.to_string_lossy());
                exit_code = 2;
                continue;
            }
        }

        match fs::read_to_string(path) {
            Ok(original) => {
                let formatted = format_content(&original, &style);
                let changed = formatted != original;

                if args.check || args.list_different {
                    if changed {
                        exit_code = 1;
                        if args.list_different {
                            different_paths.push(path.to_string_lossy().to_string());
                        }
                    }
                } else if args.write {
                    if changed && let Err(e) = fs::write(path, formatted) {
                        eprintln!("Failed to write {}: {e}", path.to_string_lossy());
                        exit_code = 2;
                    }
                } else if let Some(out) = &args.output {
                    if let Err(e) = fs::write(out, formatted) {
                        eprintln!("Failed to write output to {out:?}: {e}");
                        exit(2);
                    }
                } else {
                    // Single file without write/check: print to stdout
                    let mut stdout = io::stdout();
                    if let Err(e) = stdout.write_all(formatted.as_bytes()) {
                        eprintln!("Failed to write to stdout: {e}");
                        exit(2);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read {}: {e}", path.to_string_lossy());
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
