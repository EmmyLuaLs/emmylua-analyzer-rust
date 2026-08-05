use std::path::PathBuf;

use emmylua_code_analysis::{DbIndex, FileId};
use lsp_types::{Diagnostic, DiagnosticSeverity};

use super::OutputWriter;

/// Emits diagnostics as GitHub Actions workflow commands so they surface as
/// annotations on pull requests:
///
/// ```text
/// ::error file=src/a.lua,line=10,col=5,endLine=10,endColumn=12::message
/// ::warning file=...,line=...,col=...::message
/// ::notice file=...,line=...,col=...::message
/// ```
pub struct GithubOutputWriter {
    workspace: PathBuf,
}

impl GithubOutputWriter {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl OutputWriter for GithubOutputWriter {
    fn write(&mut self, db: &DbIndex, file_id: FileId, diagnostics: Vec<Diagnostic>) {
        let Some(file_path) = db.get_vfs().get_file_path(&file_id) else {
            return;
        };
        let relative = file_path
            .strip_prefix(strip_extended_prefix(&self.workspace))
            .unwrap_or(file_path);
        let file = relative.to_string_lossy().replace('\\', "/");

        for diagnostic in diagnostics {
            let command = match diagnostic.severity {
                Some(DiagnosticSeverity::ERROR) => "error",
                Some(DiagnosticSeverity::WARNING) => "warning",
                _ => "notice",
            };

            let start = diagnostic.range.start;
            let end = diagnostic.range.end;
            // GitHub workflow commands use 1-based line/column.
            let line = start.line + 1;
            let col = start.character + 1;
            let end_line = end.line + 1;
            let end_col = end.character + 1;

            println!(
                "::{command} file={file},line={line},col={col},endLine={end_line},endColumn={end_col}::{}",
                escape_message(&diagnostic.message)
            );
        }
    }

    fn finish(&mut self) {}
}

/// Escapes a workflow command message: `%`, `\r` and `\n`.
fn escape_message(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Strips the Windows `\\?\` extended-length prefix produced by
/// `Path::canonicalize`, so it can be used with `strip_prefix`.
fn strip_extended_prefix(path: &std::path::Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
    std::path::PathBuf::from(stripped.to_string())
}
