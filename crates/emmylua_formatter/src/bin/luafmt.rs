use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::Path,
    process::exit,
};

use clap::Parser;
use emmylua_formatter::{
    check_text, cmd_args, collect_lua_files, default_config_toml,
    diff::{DiffRenderOptions, render_unified_diff},
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn read_stdin_to_string() -> io::Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn should_use_color(choice: cmd_args::ColorChoice) -> bool {
    match choice {
        cmd_args::ColorChoice::Auto => io::stderr().is_terminal(),
        cmd_args::ColorChoice::Always => true,
        cmd_args::ColorChoice::Never => false,
    }
}

/// Renders a path relative to the current working directory with forward
/// slashes, so the diff can be consumed by `git apply`.
fn relative_diff_path(path: &Path) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    path.strip_prefix(&cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn main() {
    let args = cmd_args::CliArgs::parse();
    let diff_render_options = DiffRenderOptions {
        use_color: should_use_color(args.color),
        inline_highlight: args.inline,
        context_lines: 3,
    };

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

        let resolved = match cmd_args::resolve_style(&args, None) {
            Ok(resolved) => resolved,
            Err(err) => {
                eprintln!("Error: {err}");
                exit(2);
            }
        };
        let output = check_text(
            &content,
            resolved.config.syntax.level.into(),
            &resolved.config,
        );
        let changed = output.changed;

        if args.check || args.list_different {
            if changed {
                exit_code = 1;
                if args.check && !args.list_different {
                    eprint!(
                        "{}",
                        render_unified_diff(
                            None,
                            &content,
                            &output.formatted,
                            &diff_render_options,
                        )
                    );
                }
            }
        } else if let Some(out) = &args.output {
            if let Err(e) = fs::write(out, output.formatted) {
                eprintln!("Failed to write output to {out:?}: {e}");
                exit(2);
            }
        } else if args.write {
            eprintln!("--write with stdin requires --output <FILE>");
            exit(2);
        } else {
            let mut stdout = io::stdout();
            if let Err(e) = stdout.write_all(output.formatted.as_bytes()) {
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
        let format_result = cmd_args::resolve_style(&args, Some(path.as_path()))
            .map_err(emmylua_formatter::FormatterError::SyntaxError)
            .and_then(|resolved| {
                fs::read_to_string(path)
                    .map_err(emmylua_formatter::FormatterError::from)
                    .map(|source| {
                        let output = check_text(
                            &source,
                            resolved.config.syntax.level.into(),
                            &resolved.config,
                        );
                        (path.clone(), source, output.formatted, output.changed)
                    })
            });

        match format_result {
            Ok(result) => {
                let (result_path, source, formatted, changed) = result;

                if args.check || args.list_different {
                    if changed {
                        exit_code = 1;
                        if args.list_different {
                            different_paths.push(result_path.to_string_lossy().to_string());
                        } else if args.check {
                            eprint!(
                                "{}",
                                render_unified_diff(
                                    Some(&relative_diff_path(&result_path)),
                                    &source,
                                    &formatted,
                                    &diff_render_options,
                                )
                            );
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
                    let mut stdout = io::stdout();
                    if let Err(e) = stdout.write_all(formatted.as_bytes()) {
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
