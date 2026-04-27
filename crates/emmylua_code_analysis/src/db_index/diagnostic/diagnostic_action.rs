use rowan::TextRange;

use crate::DiagnosticCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticAction {
    range: TextRange,
    kind: DiagnosticActionKind,
}

impl DiagnosticAction {
    pub fn new(range: TextRange, kind: DiagnosticActionKind) -> Self {
        Self { range, kind }
    }

    pub fn get_range(&self) -> TextRange {
        self.range
    }

    pub fn is_enable(&self) -> bool {
        matches!(self.kind, DiagnosticActionKind::Enable(_))
    }

    pub fn is_disable(&self) -> bool {
        matches!(
            self.kind,
            DiagnosticActionKind::Disable(_) | DiagnosticActionKind::DisableAll
        )
    }

    pub fn get_code(&self) -> Option<DiagnosticCode> {
        match &self.kind {
            DiagnosticActionKind::Disable(code) => Some(*code),
            DiagnosticActionKind::Enable(code) => Some(*code),
            DiagnosticActionKind::DisableAll => None,
        }
    }

    pub fn is_match(&self, is_disable: bool, range: &TextRange, code: &DiagnosticCode) -> bool {
        if self.range.intersect(*range).is_none() {
            return false;
        }

        match (&self.kind, is_disable) {
            (DiagnosticActionKind::Disable(disable_code), true) => disable_code == code,
            (DiagnosticActionKind::Enable(enable_code), false) => enable_code == code,
            (DiagnosticActionKind::DisableAll, true) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticActionKind {
    Disable(DiagnosticCode),
    Enable(DiagnosticCode), // donot use this
    DisableAll,
}
