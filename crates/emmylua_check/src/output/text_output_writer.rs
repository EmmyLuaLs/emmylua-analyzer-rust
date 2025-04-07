use std::path::PathBuf;

use emmylua_code_analysis::{DbIndex, FileId};
use lsp_types::Diagnostic;

use super::OutputWriter;
use ariadne::{Color, Label, Report, ReportKind, Source};

#[derive(Debug)]
pub struct TextOutputWriter {
    workspace: PathBuf,
}

impl TextOutputWriter {
    pub fn new(workspace: PathBuf) -> Self {
        TextOutputWriter { workspace }
    }
}

impl OutputWriter for TextOutputWriter {
    fn write(&mut self, db: &DbIndex, file_id: FileId, diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            return;
        }

        let mut file_path = db.get_vfs().get_file_path(&file_id).unwrap().clone();
        if let Ok(new_file_path) = file_path.strip_prefix(&self.workspace) {
            file_path = new_file_path.to_path_buf();
        }

        let file_path = file_path.to_str().unwrap();
        let mut out_string = String::new();
        out_string.push_str(format!("file: {} ", file_path).as_str());
        let mut error_count = 0;
        let mut warning_count = 0;
        let mut advice_count = 0;
        for diagnostic in &diagnostics {
            match diagnostic.severity {
                Some(severity) => match severity {
                    lsp_types::DiagnosticSeverity::ERROR => {
                        error_count += 1;
                    }
                    lsp_types::DiagnosticSeverity::WARNING => {
                        warning_count += 1;
                    }
                    _ => {
                        advice_count += 1;
                    }
                },
                _ => {}
            }
        }

        if error_count > 0 {
            out_string.push_str(format!(" {} error", error_count).as_str());
        }
        if warning_count > 0 {
            out_string.push_str(format!(" {} warning", warning_count).as_str());
        }
        if advice_count > 0 {
            out_string.push_str(format!(" {} advice", advice_count).as_str());
        }

        println!("{}:", &out_string);
        let out = Color::Fixed(81);
        let document = db.get_vfs().get_document(&file_id).unwrap();
        let text = document.get_text();
        for diagnostic in diagnostics {
            let range = diagnostic.range;
            let span = document.get_range_span(range).unwrap();
            let kind = match diagnostic.severity {
                Some(severity) => match severity {
                    lsp_types::DiagnosticSeverity::ERROR => ReportKind::Error,
                    lsp_types::DiagnosticSeverity::WARNING => ReportKind::Warning,
                    _ => ReportKind::Advice,
                },
                _ => ReportKind::Error,
            };

            let code = match diagnostic.code {
                Some(code) => match code {
                    lsp_types::NumberOrString::Number(code) => code.to_string(),
                    lsp_types::NumberOrString::String(code) => code,
                },
                _ => "".to_string(),
            };

            Report::build(kind, (file_path, (span.0-1)..(span.1-1)))
                .with_code(code)
                .with_label(
                    Label::new((file_path, span.0..span.1))
                        .with_message(diagnostic.message)
                        .with_color(out),
                )
                .finish()
                .print((file_path, Source::from(text)))
                .unwrap();
        }
    }

    fn finish(&mut self) {}
}
