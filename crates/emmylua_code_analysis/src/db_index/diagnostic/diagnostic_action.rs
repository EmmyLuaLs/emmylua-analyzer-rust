use rowan::TextRange;

use crate::DiagnosticCode;

#[derive(Debug)]
pub struct DiagnosticAction {
    range: TextRange,
    kind: DiagnosticActionKind,
    code: DiagnosticCode,
}

impl DiagnosticAction {
    pub fn new(range: TextRange, kind: DiagnosticActionKind, code: DiagnosticCode) -> Self {
        Self { range, kind, code }
    }

    pub fn get_range(&self) -> TextRange {
        self.range
    }

    pub fn is_enable(&self) -> bool {
        matches!(self.kind, DiagnosticActionKind::Enable)
    }

    pub fn is_disable(&self) -> bool {
        matches!(self.kind, DiagnosticActionKind::Disable)
    }

    pub fn get_code(&self) -> DiagnosticCode {
        self.code
    }
}

#[derive(Debug)]
pub enum DiagnosticActionKind {
    Disable,
    Enable, // donot use this
}
