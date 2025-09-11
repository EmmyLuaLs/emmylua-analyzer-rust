mod json_output_writer;
mod sarif_output_writer;
mod text_output_writer;

use std::path::PathBuf;

use emmylua_code_analysis::{DbIndex, FileId};
use lsp_types::Diagnostic;
use tokio::sync::mpsc::Receiver;

use crate::cmd_args::{OutputDestination, OutputFormat};

use crate::terminal_display::TerminalDisplay;

pub async fn output_result(
    total_count: usize,
    db: &DbIndex,
    workspace: PathBuf,
    mut receiver: Receiver<(FileId, Option<Vec<Diagnostic>>)>,
    output_format: OutputFormat,
    output: OutputDestination,
    warnings_as_errors: bool,
) -> i32 {
    let mut writer: Box<dyn OutputWriter> = match output_format {
        OutputFormat::Json => Box::new(json_output_writer::JsonOutputWriter::new(output)),
        OutputFormat::Sarif => Box::new(sarif_output_writer::SarifOutputWriter::new(output)),
        OutputFormat::Text => {
            Box::new(text_output_writer::TextOutputWriter::new(workspace.clone()))
        }
    };

    let terminal_display = TerminalDisplay::new(workspace);
    let mut has_error = false;
    let mut count = 0;
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut info_count = 0;
    let mut hint_count = 0;

    while let Some((file_id, diagnostics)) = receiver.recv().await {
        count += 1;
        if let Some(diagnostics) = diagnostics {
            for diagnostic in &diagnostics {
                match diagnostic.severity {
                    Some(lsp_types::DiagnosticSeverity::ERROR) => {
                        has_error = true;
                        error_count += 1;
                    }
                    Some(lsp_types::DiagnosticSeverity::WARNING) => {
                        if warnings_as_errors {
                            has_error = true;
                        }
                        warning_count += 1;
                    }
                    Some(lsp_types::DiagnosticSeverity::INFORMATION) => {
                        info_count += 1;
                    }
                    Some(lsp_types::DiagnosticSeverity::HINT) => {
                        hint_count += 1;
                    }
                    _ => {}
                }
            }
            writer.write(db, file_id, diagnostics);
        }

        if count == total_count {
            break;
        }
    }

    writer.finish();

    // 只在 Text 格式时显示汇总
    if output_format == OutputFormat::Text {
        terminal_display.print_summary(error_count, warning_count, info_count, hint_count);
    }

    if has_error { 1 } else { 0 }
}

trait OutputWriter {
    fn write(&mut self, db: &DbIndex, file_id: FileId, diagnostics: Vec<Diagnostic>);

    fn finish(&mut self);
}
