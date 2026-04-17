use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    process::exit,
};

use clap::Parser;
use emmylua_formatter::{check_text, cmd_args, collect_lua_files, default_config_toml};
use similar::{ChangeTag, TextDiff};

#[derive(Clone, Copy)]
struct DiffRenderOptions {
    use_color: bool,
    style: cmd_args::DiffStyle,
}

impl DiffRenderOptions {
    fn marker_mode(self) -> bool {
        !self.use_color && matches!(self.style, cmd_args::DiffStyle::Marker)
    }
}

fn render_diff_header_path(path: &str, is_new: bool, style: cmd_args::DiffStyle) -> String {
    if matches!(style, cmd_args::DiffStyle::Git) {
        let prefix = if is_new { "b/" } else { "a/" };
        return format!("{}{path}", prefix);
    }

    path.to_string()
}

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn read_stdin_to_string() -> io::Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn format_unified_diff(
    path: &str,
    original: &str,
    formatted: &str,
    options: DiffRenderOptions,
) -> String {
    let diff = TextDiff::from_lines(original, formatted);
    let mut out = String::new();
    out.push_str(&colorize(
        &format!(
            "--- {}",
            render_diff_header_path(path, false, options.style)
        ),
        "1;31",
        options.use_color,
    ));
    out.push('\n');
    out.push_str(&colorize(
        &format!("+++ {}", render_diff_header_path(path, true, options.style)),
        "1;32",
        options.use_color,
    ));
    out.push('\n');

    for group in diff.grouped_ops(3) {
        let mut old_start_line = None;
        let mut old_end_line = None;
        let mut new_start_line = None;
        let mut new_end_line = None;
        let mut body = String::new();

        for op in group {
            for change in diff.iter_inline_changes(&op) {
                if old_start_line.is_none() {
                    old_start_line = change.old_index().map(|index| index + 1);
                }
                if new_start_line.is_none() {
                    new_start_line = change.new_index().map(|index| index + 1);
                }
                if let Some(index) = change.old_index() {
                    old_end_line = Some(index + 1);
                }
                if let Some(index) = change.new_index() {
                    new_end_line = Some(index + 1);
                }

                body.push_str(&render_line_prefix(change.tag(), options));
                for (emphasized, value) in change.iter_strings_lossy() {
                    if emphasized {
                        body.push_str(&render_emphasized_segment(
                            change.tag(),
                            value.as_ref(),
                            options,
                        ));
                    } else {
                        body.push_str(&render_plain_segment(change.tag(), value.as_ref(), options));
                    }
                }
                if !body.ends_with('\n') {
                    body.push('\n');
                }
            }
        }

        out.push_str(&colorize(
            &format!(
                "@@ -{} +{} @@",
                format_hunk_range(old_start_line, old_end_line),
                format_hunk_range(new_start_line, new_end_line)
            ),
            "1;36",
            options.use_color,
        ));
        out.push('\n');
        out.push_str(&body);
    }

    out
}

fn render_line_prefix(tag: ChangeTag, options: DiffRenderOptions) -> String {
    let (prefix, color) = match tag {
        ChangeTag::Equal => (" ", "0"),
        ChangeTag::Delete => ("-", "31"),
        ChangeTag::Insert => ("+", "32"),
    };
    colorize(prefix, color, options.use_color)
}

fn render_plain_segment(tag: ChangeTag, text: &str, options: DiffRenderOptions) -> String {
    if !options.use_color {
        return text.to_string();
    }

    let color = match tag {
        ChangeTag::Equal => return text.to_string(),
        ChangeTag::Delete => "31",
        ChangeTag::Insert => "32",
    };

    colorize(text, color, true)
}

fn render_emphasized_segment(tag: ChangeTag, text: &str, options: DiffRenderOptions) -> String {
    if options.marker_mode() {
        return match tag {
            ChangeTag::Delete => format!("[-{}-]", text),
            ChangeTag::Insert => format!("{{+{}+}}", text),
            ChangeTag::Equal => text.to_string(),
        };
    }

    let color = match tag {
        ChangeTag::Delete => "1;91",
        ChangeTag::Insert => "1;92",
        ChangeTag::Equal => return text.to_string(),
    };

    colorize(text, color, true)
}

fn colorize(text: &str, ansi_code: &str, enabled: bool) -> String {
    if !enabled || text.is_empty() {
        return text.to_string();
    }

    format!("\x1b[{ansi_code}m{text}\x1b[0m")
}

fn should_use_color(choice: cmd_args::ColorChoice) -> bool {
    match choice {
        cmd_args::ColorChoice::Auto => io::stderr().is_terminal(),
        cmd_args::ColorChoice::Always => true,
        cmd_args::ColorChoice::Never => false,
    }
}

fn format_hunk_range(start: Option<usize>, end: Option<usize>) -> String {
    match (start, end) {
        (Some(start_line), Some(end_line)) => {
            let count = end_line.saturating_sub(start_line) + 1;
            format!("{},{}", start_line, count)
        }
        (Some(start_line), None) => format!("{},0", start_line),
        (None, Some(end_line)) => format!("0,{}", end_line),
        (None, None) => "0,0".to_string(),
    }
}

fn main() {
    let args = cmd_args::CliArgs::parse();
    let diff_render_options = DiffRenderOptions {
        use_color: should_use_color(args.color),
        style: args.diff_style,
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
                        format_unified_diff(
                            "<stdin>",
                            &content,
                            &output.formatted,
                            diff_render_options,
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
                                format_unified_diff(
                                    &result_path.to_string_lossy(),
                                    &source,
                                    &formatted,
                                    diff_render_options,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_diff_keeps_inline_markers() {
        let rendered = format_unified_diff(
            "<stdin>",
            "local x=1\n",
            "local x = 1\n",
            DiffRenderOptions {
                use_color: false,
                style: cmd_args::DiffStyle::Marker,
            },
        );

        assert!(rendered.contains("[-x=1-]") || rendered.contains("{+x = 1+}"));
        assert!(!rendered.contains("\x1b["));
    }

    #[test]
    fn test_color_diff_uses_ansi_without_inline_markers() {
        let rendered = format_unified_diff(
            "<stdin>",
            "local x=1\n",
            "local x = 1\n",
            DiffRenderOptions {
                use_color: true,
                style: cmd_args::DiffStyle::Marker,
            },
        );

        assert!(rendered.contains("\x1b["));
        assert!(!rendered.contains("[-"));
        assert!(!rendered.contains("{+"));
    }

    #[test]
    fn test_git_diff_style_uses_prefixed_headers_without_inline_markers() {
        let rendered = format_unified_diff(
            "src/test.lua",
            "local x=1\n",
            "local x = 1\n",
            DiffRenderOptions {
                use_color: false,
                style: cmd_args::DiffStyle::Git,
            },
        );

        assert!(rendered.contains("--- a/src/test.lua"));
        assert!(rendered.contains("+++ b/src/test.lua"));
        assert!(!rendered.contains("[-"));
        assert!(!rendered.contains("{+"));
    }
}
