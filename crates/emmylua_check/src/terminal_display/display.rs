use std::io::IsTerminal;
use std::path::PathBuf;

use ansi_term::{Color, Style};
use emmylua_code_analysis::{DocumentView, FileId, SalsaDatabase};
use lsp_types::{Diagnostic, DiagnosticSeverity};

#[derive(Debug)]
pub struct TerminalDisplay {
    workspace: PathBuf,
    supports_color: bool,
    supports_underline: bool,
}

/// LSP (line, character) → in-line byte column (for highlighting).
fn col_offset(document: &DocumentView, line: usize, character: usize) -> Option<usize> {
    let offset = document.get_offset(line, character)?;
    let line_start = document.get_line_range(line)?.start();
    Some(usize::from(offset) - usize::from(line_start))
}

impl TerminalDisplay {
    pub fn new(workspace: PathBuf) -> Self {
        let supports_color = std::io::stdout().is_terminal();
        let supports_underline = supports_color && Self::check_underline_support();

        Self {
            workspace,
            supports_color,
            supports_underline,
        }
    }

    fn check_underline_support() -> bool {
        std::env::var("TERM").is_ok() || std::env::var("WT_SESSION").is_ok()
    }

    pub fn display_diagnostics(
        &mut self,
        db: &SalsaDatabase,
        file_id: FileId,
        diagnostics: Vec<Diagnostic>,
    ) {
        if diagnostics.is_empty() {
            return;
        }

        let file_path = self.get_relative_path(db, file_id);
        let document = db.document(file_id).unwrap();
        let text = document.get_text();
        let text_lines = text.lines().collect::<Vec<&str>>();

        // Group statistics by severity level
        let mut error_count = 0;
        let mut warning_count = 0;
        let mut info_count = 0;
        let mut hint_count = 0;

        for diagnostic in &diagnostics {
            match diagnostic.severity {
                Some(DiagnosticSeverity::ERROR) => error_count += 1,
                Some(DiagnosticSeverity::WARNING) => warning_count += 1,
                Some(DiagnosticSeverity::INFORMATION) => info_count += 1,
                Some(DiagnosticSeverity::HINT) => hint_count += 1,
                _ => {}
            }
        }

        // Print file header information
        self.print_file_header(
            &file_path,
            error_count,
            warning_count,
            info_count,
            hint_count,
        );

        // Display each diagnostic individually
        for diagnostic in diagnostics {
            self.display_single_diagnostic(&file_path, &document, &text_lines, diagnostic);
        }

        println!(); // Add blank line separator
    }

    fn get_relative_path(&self, db: &SalsaDatabase, file_id: FileId) -> String {
        let mut file_path = db.file_path(file_id).unwrap().clone();
        if let Ok(new_file_path) = file_path.strip_prefix(&self.workspace) {
            file_path = new_file_path.to_path_buf();
        }
        file_path.to_string_lossy().to_string()
    }

    fn print_file_header(
        &self,
        file_path: &str,
        error_count: usize,
        warning_count: usize,
        info_count: usize,
        hint_count: usize,
    ) {
        if self.supports_color {
            print!("{}", Color::Cyan.bold().paint("--- "));
            print!("{}", Color::White.bold().paint(file_path));
            print!("{}", Color::Cyan.bold().paint(" "));
        } else {
            print!("--- {} ", file_path);
        }

        let mut parts = Vec::new();
        if error_count > 0 {
            let text = format!(
                "{} error{}",
                error_count,
                if error_count > 1 { "s" } else { "" }
            );
            if self.supports_color {
                parts.push(Color::Red.bold().paint(text).to_string());
            } else {
                parts.push(text);
            }
        }
        if warning_count > 0 {
            let text = format!(
                "{} warning{}",
                warning_count,
                if warning_count > 1 { "s" } else { "" }
            );
            if self.supports_color {
                parts.push(Color::Yellow.bold().paint(text).to_string());
            } else {
                parts.push(text);
            }
        }
        if info_count > 0 {
            let text = format!("{} info", info_count);
            if self.supports_color {
                parts.push(Color::Purple.bold().paint(text).to_string());
            } else {
                parts.push(text);
            }
        }
        if hint_count > 0 {
            let text = format!(
                "{} hint{}",
                hint_count,
                if hint_count > 1 { "s" } else { "" }
            );
            if self.supports_color {
                parts.push(Color::Cyan.bold().paint(text).to_string());
            } else {
                parts.push(text);
            }
        }

        if !parts.is_empty() {
            print!("[{}]", parts.join(", "));
        }

        println!();
    }

    fn display_single_diagnostic(
        &mut self,
        file_path: &str,
        document: &DocumentView,
        lines: &[&str],
        diagnostic: Diagnostic,
    ) {
        let range = diagnostic.range;
        // Get severity level colors and symbols
        let (level_color, level_symbol, _level_name) = match diagnostic.severity {
            Some(DiagnosticSeverity::ERROR) => (Color::Red, "error", "error"),
            Some(DiagnosticSeverity::WARNING) => (Color::Yellow, "warning", "warning"),
            Some(DiagnosticSeverity::INFORMATION) => (Color::Purple, "info", "info"),
            Some(DiagnosticSeverity::HINT) => (Color::Cyan, "hint", "hint"),
            _ => (Color::Red, "error", "error"),
        };

        // Get diagnostic code
        let code = match diagnostic.code {
            Some(code) => match code {
                lsp_types::NumberOrString::Number(code) => format!("[{}]", code),
                lsp_types::NumberOrString::String(code) => format!("[{}]", code),
            },
            _ => String::new(),
        };

        // Calculate line and column numbers
        let start_line = range.start.line as usize;
        let start_character = range.start.character as usize;
        let Some(start_col) = col_offset(document, start_line, start_character) else {
            return;
        };
        let end_line = range.end.line as usize;
        let end_character = range.end.character as usize;
        let Some(end_col) = col_offset(document, end_line, end_character) else {
            return;
        };

        if start_line >= lines.len() {
            return;
        }

        // Print diagnostic header
        if self.supports_color {
            print!("{}: ", level_color.bold().paint(level_symbol));
            print!("{}", Style::new().bold().paint(&diagnostic.message));
            if !code.is_empty() {
                print!(" {}", Color::Fixed(8).paint(&code)); // Dark gray
            }
        } else {
            print!("{}: {}", level_symbol, diagnostic.message);
            if !code.is_empty() {
                print!(" {}", code);
            }
        }
        println!();

        // Print location information
        if self.supports_color {
            println!(
                "  {}: {}:{}:{}",
                Color::Fixed(8).paint("-->"), // Dark gray
                file_path,
                start_line + 1,
                start_character + 1
            );
        } else {
            println!(
                "  --> {}:{}:{}",
                file_path,
                start_line + 1,
                start_character + 1
            );
        }

        // Calculate context range to display (one line before and after for context)
        let context_start = if start_line > 0 { start_line - 1 } else { 0 };
        let context_end = std::cmp::min(end_line + 1, lines.len() - 1);

        // Calculate maximum line number width for alignment
        let max_line_num = context_end + 1;
        let line_num_width = max_line_num.to_string().len();

        println!(); // Empty line separator

        // Display code lines
        for (i, line_text) in lines
            .iter()
            .enumerate()
            .take(context_end + 1)
            .skip(context_start)
        {
            let line_num = i + 1;

            if self.supports_color {
                print!(
                    "  {} │ ",
                    Color::Cyan.paint(format!("{:width$}", line_num, width = line_num_width))
                );
            } else {
                print!("  {:width$} | ", line_num, width = line_num_width);
            }

            if i >= start_line && i <= end_line {
                // This is an error line, needs highlighting
                if i == start_line && i == end_line {
                    // Single line error
                    let prefix = &line_text[..std::cmp::min(start_col, line_text.len())];
                    let error_part = if start_col < line_text.len() && end_col <= line_text.len() {
                        &line_text[start_col..end_col]
                    } else if start_col < line_text.len() {
                        &line_text[start_col..]
                    } else {
                        ""
                    };
                    let suffix = if end_col < line_text.len() {
                        &line_text[end_col..]
                    } else {
                        ""
                    };

                    print!("{}", prefix);
                    if self.supports_color && !error_part.is_empty() {
                        let mut style = level_color.bold();
                        if self.supports_underline {
                            style = style.underline();
                        }
                        print!("{}", style.paint(error_part));
                    } else {
                        print!("{}", error_part);
                    }
                    println!("{}", suffix);
                } else {
                    // Start or end line of multi-line error
                    if self.supports_color {
                        let mut style = level_color.bold();
                        if self.supports_underline {
                            style = style.underline();
                        }
                        println!("{}", style.paint(*line_text));
                    } else {
                        println!("{}", line_text);
                    }
                }
            } else {
                // Context line
                println!("{}", line_text);
            }
        }

        println!(); // Ending empty line
    }

    pub fn print_summary(
        &self,
        total_errors: usize,
        total_warnings: usize,
        total_info: usize,
        total_hints: usize,
    ) {
        if total_errors == 0 && total_warnings == 0 && total_info == 0 && total_hints == 0 {
            if self.supports_color {
                println!("{}", Color::Green.bold().paint("No issues found"));
            } else {
                println!("No issues found");
            }
            return;
        }

        println!();
        if self.supports_color {
            println!("{}", Color::Cyan.bold().paint("Summary"));
        } else {
            println!("Summary");
        }

        if total_errors > 0 {
            let text = format!(
                "  {} error{}",
                total_errors,
                if total_errors > 1 { "s" } else { "" }
            );
            if self.supports_color {
                println!("{}", Color::Red.bold().paint(text));
            } else {
                println!("{}", text);
            }
        }

        if total_warnings > 0 {
            let text = format!(
                "  {} warning{}",
                total_warnings,
                if total_warnings > 1 { "s" } else { "" }
            );
            if self.supports_color {
                println!("{}", Color::Yellow.bold().paint(text));
            } else {
                println!("{}", text);
            }
        }

        if total_info > 0 {
            let text = format!("  {} info", total_info);
            if self.supports_color {
                println!("{}", Color::Purple.bold().paint(text));
            } else {
                println!("{}", text);
            }
        }

        if total_hints > 0 {
            let text = format!(
                "  {} hint{}",
                total_hints,
                if total_hints > 1 { "s" } else { "" }
            );
            if self.supports_color {
                println!("{}", Color::Cyan.bold().paint(text));
            } else {
                println!("{}", text);
            }
        }

        // Final status
        if total_errors > 0 {
            if self.supports_color {
                println!("\n{}", Color::Red.bold().paint("Check failed"));
            } else {
                println!("\nCheck failed");
            }
        } else if total_warnings > 0 {
            if self.supports_color {
                println!(
                    "\n{}",
                    Color::Yellow.bold().paint("Check completed with warnings")
                );
            } else {
                println!("\nCheck completed with warnings");
            }
        } else if self.supports_color {
            println!("\n{}", Color::Green.bold().paint("Check successful"));
        } else {
            println!("\nCheck successful");
        }
    }
}
