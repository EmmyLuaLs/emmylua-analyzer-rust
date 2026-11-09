use std::path::PathBuf;

use emmylua_code_analysis::{FileId, SemanticDatabase};
use lsp_types::Diagnostic;

use super::OutputWriter;
use crate::terminal_display::TerminalDisplay;

#[derive(Debug)]
pub struct TextOutputWriter {
    terminal_display: TerminalDisplay,
}

impl TextOutputWriter {
    pub fn new(workspace: PathBuf) -> Self {
        TextOutputWriter {
            terminal_display: TerminalDisplay::new(workspace),
        }
    }
}

impl OutputWriter for TextOutputWriter {
    fn write(&mut self, db: &SemanticDatabase, file_id: FileId, diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            return;
        }

        self.terminal_display
            .display_diagnostics(db, file_id, diagnostics);
    }

    fn finish(&mut self) {}
}
