use emmylua_code_analysis::EmmyrcExternalTool;
use rowan::TextSize;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::handlers::document_formatting::FormattingOptions;

pub struct FormattingRange {
    pub start_offset: TextSize,
    pub end_offset: TextSize,
    pub start_line: u32,
    pub end_line: u32,
}

pub async fn external_tool_format(
    emmyrc_external_tool: &EmmyrcExternalTool,
    text: &str,
    file_path: &str,
    range: Option<FormattingRange>,
    options: FormattingOptions,
) -> Option<String> {
    let exe_path = &emmyrc_external_tool.program;
    let args = &emmyrc_external_tool.args;
    let timeout_duration = Duration::from_millis(emmyrc_external_tool.timeout);

    let mut cmd = Command::new(exe_path);

    for arg in args {
        if let Some(processed_arg) = parse_macro_arg(arg, file_path, &range, &options) {
            cmd.arg(processed_arg);
        }
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            log::error!("Failed to spawn external formatter process: {}", e);
            return None;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(text.as_bytes()).await {
            log::error!("Failed to write to external formatter stdin: {}", e);
            return None;
        }
        if let Err(e) = stdin.shutdown().await {
            log::error!("Failed to close external formatter stdin: {}", e);
            return None;
        }
    }

    let output = match timeout(timeout_duration, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            log::error!("External formatter process error: {}", e);
            return None;
        }
        Err(_) => {
            log::error!(
                "External formatter process timed out after {}ms",
                emmyrc_external_tool.timeout
            );
            return None;
        }
    };

    if !output.status.success() {
        log::error!(
            "External formatter exited with non-zero status: {}. Stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }

    match String::from_utf8(output.stdout) {
        Ok(formatted_text) => {
            log::debug!("External formatter completed successfully");
            Some(formatted_text)
        }
        Err(e) => {
            log::error!("External formatter output is not valid UTF-8: {}", e);
            None
        }
    }
}

fn parse_macro_arg(
    arg: &str,
    file_path: &str,
    range: &Option<FormattingRange>,
    options: &FormattingOptions,
) -> Option<String> {
    let mut result = String::new();
    let mut chars = arg.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next();

            // collect ${} content
            let mut content = String::new();
            let mut brace_count = 1;

            for inner_ch in chars.by_ref() {
                if inner_ch == '{' {
                    brace_count += 1;
                    if brace_count > 1 {
                        content.push(inner_ch);
                    }
                } else if inner_ch == '}' {
                    brace_count -= 1;
                    if brace_count == 0 {
                        break;
                    }
                    content.push(inner_ch);
                } else {
                    content.push(inner_ch);
                }
            }

            // parse content
            let replacement = if content.contains('?') {
                // handle ${key:value} format
                let parts: Vec<&str> = content.splitn(2, '?').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let value = parts[1].trim();

                    let (true_value, fail_value) = if value.contains(':') {
                        let value_parts = value.splitn(2, ':').collect::<Vec<&str>>();
                        let true_value = value_parts[0].trim();
                        let fail_value = value_parts.get(1).map_or("", |s| s.trim());
                        (true_value, fail_value)
                    } else {
                        (value, "")
                    };

                    match key {
                        "use_tabs" => {
                            if options.use_tabs {
                                true_value.to_string()
                            } else {
                                fail_value.to_string()
                            }
                        }
                        "insert_final_newline" => {
                            if options.insert_final_newline {
                                true_value.to_string()
                            } else {
                                fail_value.to_string()
                            }
                        }
                        "non_standard_symbol" => {
                            if options.non_standard_symbol {
                                true_value.to_string()
                            } else {
                                fail_value.to_string()
                            }
                        }
                        _ => true_value.to_string(), // if not a predefined key, return value
                    }
                } else {
                    content.clone()
                }
            } else {
                // handle ${variable} format
                match content.trim() {
                    "file" => file_path.to_string(),
                    "indent_size" => options.indent_size.to_string(),
                    "start_offset" => {
                        if let Some(r) = range {
                            u32::from(r.start_offset).to_string()
                        } else {
                            "".to_string()
                        }
                    }
                    "end_offset" => {
                        if let Some(r) = range {
                            u32::from(r.end_offset).to_string()
                        } else {
                            "".to_string()
                        }
                    }
                    "start_line" => {
                        if let Some(r) = range {
                            r.start_line.to_string()
                        } else {
                            "".to_string()
                        }
                    }
                    "end_line" => {
                        if let Some(r) = range {
                            r.end_line.to_string()
                        } else {
                            "".to_string()
                        }
                    }
                    _ => "".to_string(),
                }
            };

            result.push_str(&replacement);
        } else {
            result.push(ch);
        }
    }

    if result.is_empty() {
        return None; // if no content was processed, return None
    }
    Some(result)
}
