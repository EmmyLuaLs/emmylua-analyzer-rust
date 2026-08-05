use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::Path,
    process::exit,
};

use clap::Parser;
use emmylua_formatter::{
    LuaFormatConfig, SourceText, check_text, cmd_args, collect_lua_files, default_config_toml,
    diff::{DiffRenderOptions, render_unified_diff},
    reformat_lua_code,
};
use emmylua_parser::LuaLanguageLevel;

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

/// Reports a syntax error to stderr and flags a non-zero exit code.
fn report_syntax_error(
    label: &str,
    error: &emmylua_formatter::LuaSyntaxErrorInfo,
    exit_code: &mut i32,
) {
    eprintln!(
        "{label}: syntax error at {}:{}: {}",
        error.line, error.column, error.message
    );
    *exit_code = 2;
}

/// When `--verify` is set, reformats the output and warns if it is not stable.
fn verify_idempotent(
    source: &str,
    formatted: &str,
    level: LuaLanguageLevel,
    config: &LuaFormatConfig,
    label: &str,
    exit_code: &mut i32,
) {
    if formatted == source {
        return;
    }
    let re_formatted = reformat_lua_code(
        &SourceText {
            text: formatted,
            level,
        },
        config,
    );
    if re_formatted != formatted {
        eprintln!("warning: {label}: formatting is not idempotent");
        *exit_code = 2;
    }
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

        let resolved = match cmd_args::resolve_style(&args, args.stdin_filename.as_deref()) {
            Ok(resolved) => resolved,
            Err(err) => {
                eprintln!("Error: {err}");
                exit(2);
            }
        };
        let level = resolved.config.syntax.level.into();
        let output = check_text(&content, level, &resolved.config);

        if let Some(error) = &output.syntax_error {
            report_syntax_error("<stdin>", error, &mut exit_code);
        }

        if args.verify {
            verify_idempotent(
                &content,
                &output.formatted,
                level,
                &resolved.config,
                "<stdin>",
                &mut exit_code,
            );
        }

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
                let config = resolved.config;
                fs::read_to_string(path)
                    .map_err(emmylua_formatter::FormatterError::from)
                    .map(|source| {
                        let level: emmylua_parser::LuaLanguageLevel = config.syntax.level.into();
                        let output = check_text(&source, level, &config);
                        (path.clone(), source, config, level, output)
                    })
            });

        match format_result {
            Ok((result_path, source, config, level, output)) => {
                if let Some(error) = &output.syntax_error {
                    report_syntax_error(&result_path.to_string_lossy(), error, &mut exit_code);
                }

                if args.verify {
                    verify_idempotent(
                        &source,
                        &output.formatted,
                        level,
                        &config,
                        &result_path.to_string_lossy(),
                        &mut exit_code,
                    );
                }

                let changed = output.changed;

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
                                    &output.formatted,
                                    &diff_render_options,
                                )
                            );
                        }
                    }
                } else if args.write {
                    if changed && let Err(e) = fs::write(path, &output.formatted) {
                        eprintln!("Failed to write {}: {e}", path.to_string_lossy());
                        exit_code = 2;
                    }
                } else if let Some(out) = &args.output {
                    if let Err(e) = fs::write(out, &output.formatted) {
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
