//! Alignment post-processing module.
//!
//! After the Printer produces plain text output, this module performs
//! trailing comment alignment on consecutive lines.

/// Align trailing comments on consecutive lines to the same column.
///
/// Groups consecutive lines that have `--` trailing comments and pads
/// their code portion so the comments start at the same column.
/// ```text
/// local a = 1 -- short          local a = 1   -- short
/// local bbb = 2 -- long var  => local bbb = 2 -- long var
/// ```
pub fn align_trailing_comments(text: &str, newline: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        // Try to find a group of consecutive lines with trailing comments
        if split_trailing_comment(lines[i]).is_some() {
            let group_start = i;
            let mut group_end = i + 1;

            // Scan forward for consecutive lines with trailing comments
            while group_end < lines.len() {
                if split_trailing_comment(lines[group_end]).is_some() {
                    group_end += 1;
                } else {
                    break;
                }
            }

            if group_end - group_start >= 2 {
                // Align only when there are at least 2 lines
                let mut max_code_width = 0;
                let mut entries: Vec<(&str, &str)> = Vec::new();

                for line in lines.iter().take(group_end).skip(group_start) {
                    let (code, comment) = split_trailing_comment(line).unwrap();
                    let code_trimmed = code.trim_end();
                    max_code_width = max_code_width.max(code_trimmed.len());
                    entries.push((code_trimmed, comment));
                }

                for (code, comment) in entries {
                    let padding = max_code_width - code.len();
                    result_lines.push(format!("{}{} {}", code, " ".repeat(padding), comment));
                }

                i = group_end;
                continue;
            }
        }

        result_lines.push(lines[i].to_string());
        i += 1;
    }

    // Preserve trailing newline
    let mut output = result_lines.join(newline);
    if text.ends_with('\n') || text.ends_with("\r\n") {
        output.push_str(newline);
    }
    output
}

/// Find a trailing comment (`--` outside of strings) in a line.
/// Returns `(code_before_comment, comment_including_dashes)`.
fn split_trailing_comment(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    // A line that starts with `--` is a standalone comment, not a trailing one
    if trimmed.starts_with("--") {
        return None;
    }

    // Scan the line, skipping string contents, to find `--`
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < len && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1; // skip escaped char
                    }
                    i += 1;
                }
                i += 1; // skip closing quote
            }
            b'[' if i + 1 < len && (bytes[i + 1] == b'[' || bytes[i + 1] == b'=') => {
                // Long string [[ ... ]] or [=[ ... ]=]
                i += 2;
                while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                    i += 1;
                }
                i += 2;
            }
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                return Some((&line[..i], &line[i..]));
            }
            _ => i += 1,
        }
    }

    None
}
