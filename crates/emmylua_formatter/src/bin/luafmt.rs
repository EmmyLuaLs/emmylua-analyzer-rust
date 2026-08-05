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

/// Writes a fresh default config file to `target`, refusing to overwrite.
fn write_init_config(target: &Path) -> Result<(), String> {
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    let config = default_config_toml().map_err(|err| err.to_string())?;
    fs::write(target, config)
        .map_err(|err| format!("failed to write {}: {err}", target.display()))?;
    println!("Wrote {}", target.display());
    Ok(())
}

/// Prints the effective configuration for `path`: where it came from and which
/// settings differ from defaults.
fn explain_config_for_path(args: &cmd_args::CliArgs, path: &Path) -> Result<(), String> {
    let resolved = cmd_args::resolve_style(args, Some(path))?;
    match &resolved.source_path {
        Some(config_path) => println!("config file: {}", config_path.display()),
        None => println!("config file: <defaults>"),
    }
    println!("resolved for: {}", path.display());

    let diffs = resolved.config.settings_differing_from_default();
    if diffs.is_empty() {
        println!("no settings differ from defaults");
    } else {
        println!("settings differing from defaults:");
        for (key, value) in diffs {
            println!("  {key} = {value}");
        }
    }
    Ok(())
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

    if let Some(target) = &args.init {
        match write_init_config(target) {
            Ok(()) => exit(0),
            Err(message) => {
                eprintln!("Error: {message}");
                exit(2);
            }
        }
    }

    if let Some(path) = &args.explain_config {
        match explain_config_for_path(&args, path) {
            Ok(()) => exit(0),
            Err(message) => {
                eprintln!("Error: {message}");
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
            if args.json {
                let has_error = output.syntax_error.is_some();
                let changed_files: Vec<&str> = if changed { vec!["<stdin>"] } else { Vec::new() };
                let error_files: Vec<&str> = if has_error {
                    vec!["<stdin>"]
                } else {
                    Vec::new()
                };
                println!(
                    "{}",
                    serde_json::json!({
                        "changed_files": changed_files,
                        "changed_count": changed_files.len(),
                        "ok_count": usize::from(!changed && !has_error),
                        "error_files": error_files,
                        "error_count": error_files.len(),
                    })
                );
            } else if changed {
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
    let mut error_files: Vec<String> = Vec::new();
    let mut ok_count: usize = 0;

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
                    error_files.push(result_path.to_string_lossy().to_string());
                } else if output.changed {
                    different_paths.push(result_path.to_string_lossy().to_string());
                } else {
                    ok_count += 1;
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
                    if changed && !args.json {
                        exit_code = 1;
                        if args.check && !args.list_different {
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

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "changed_files": different_paths,
                "changed_count": different_paths.len(),
                "ok_count": ok_count,
                "error_files": error_files,
                "error_count": error_files.len(),
            })
        );
    } else if args.list_different && !different_paths.is_empty() {
        for p in different_paths {
            println!("{p}");
        }
    } else if args.check {
        let mut summary = format!(
            "{} file(s) would be reformatted, {} file(s) OK",
            different_paths.len(),
            ok_count
        );
        if !error_files.is_empty() {
            summary.push_str(&format!(
                ", {} file(s) could not be formatted",
                error_files.len()
            ));
        }
        eprintln!("{summary}");
    }

    exit(exit_code);
}
