mod analyze_error;
mod diagnostic_action;

use std::collections::{HashMap, HashSet};

pub use analyze_error::AnalyzeError;
pub use diagnostic_action::{DiagnosticAction, DiagnosticActionKind};
use rowan::TextRange;

use crate::{DiagnosticCode, FileId};

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct DiagnosticIndex {
    diagnostic_actions: HashMap<FileId, Vec<DiagnosticAction>>,
    diagnostics: HashMap<FileId, Vec<AnalyzeError>>,
    file_diagnostic_disabled: HashMap<FileId, HashSet<DiagnosticCode>>,
    file_diagnostic_enabled: HashMap<FileId, HashSet<DiagnosticCode>>,
}

impl DiagnosticIndex {
    pub fn new() -> Self {
        Self {
            diagnostic_actions: HashMap::new(),
            diagnostics: HashMap::new(),
            file_diagnostic_disabled: HashMap::new(),
            file_diagnostic_enabled: HashMap::new(),
        }
    }

    pub fn add_diagnostic_action(&mut self, file_id: FileId, diagnostic: DiagnosticAction) {
        self.diagnostic_actions
            .entry(file_id)
            .or_default()
            .push(diagnostic);
    }

    pub fn add_file_diagnostic_disabled(&mut self, file_id: FileId, code: DiagnosticCode) {
        self.file_diagnostic_disabled
            .entry(file_id)
            .or_default()
            .insert(code);
    }

    pub fn add_file_diagnostic_enabled(&mut self, file_id: FileId, code: DiagnosticCode) {
        self.file_diagnostic_enabled
            .entry(file_id)
            .or_default()
            .insert(code);
    }

    pub fn get_diagnostics_actions(&self, file_id: FileId) -> Option<&Vec<DiagnosticAction>> {
        self.diagnostic_actions.get(&file_id)
    }

    pub fn add_diagnostic(&mut self, file_id: FileId, diagnostic: AnalyzeError) {
        self.diagnostics
            .entry(file_id)
            .or_default()
            .push(diagnostic);
    }

    pub fn get_diagnostics(&self, file_id: FileId) -> Option<&Vec<AnalyzeError>> {
        self.diagnostics.get(&file_id)
    }

    pub fn is_file_diagnostic_code_disabled(
        &self,
        file_id: &FileId,
        code: &DiagnosticCode,
        range: &TextRange,
    ) -> bool {
        if let Some(disabled) = self.diagnostic_actions.get(file_id) {
            for action in disabled {
                if action.get_code() == *code
                    && action.is_disable()
                    && action.get_range().intersect(*range).is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_file_disabled(&self, file_id: &FileId, code: &DiagnosticCode) -> bool {
        if let Some(disabled) = self.file_diagnostic_disabled.get(file_id) {
            disabled.contains(code)
        } else {
            false
        }
    }

    pub fn is_file_enabled(&self, file_id: &FileId, code: &DiagnosticCode) -> bool {
        if let Some(enabled) = self.file_diagnostic_enabled.get(file_id) {
            enabled.contains(code)
        } else {
            false
        }
    }
}

impl LuaIndex for DiagnosticIndex {
    fn remove(&mut self, file_id: FileId) {
        self.diagnostic_actions.remove(&file_id);
        self.diagnostics.remove(&file_id);
        self.file_diagnostic_disabled.remove(&file_id);
        self.file_diagnostic_enabled.remove(&file_id);
    }

    fn clear(&mut self) {
        self.diagnostic_actions.clear();
        self.diagnostics.clear();
        self.file_diagnostic_disabled.clear();
        self.file_diagnostic_enabled.clear();
    }
}
