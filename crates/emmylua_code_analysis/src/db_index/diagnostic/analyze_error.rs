use rowan::TextRange;

use crate::DiagnosticCode;

#[derive(Debug, Clone)]
pub struct AnalyzeError {
    pub kind: DiagnosticCode,
    pub message: String,
    pub range: TextRange,
}

impl AnalyzeError {
    pub fn new(kind: DiagnosticCode, message: &str, range: TextRange) -> Self {
        Self {
            kind,
            message: message.to_string(),
            range,
        }
    }
}
