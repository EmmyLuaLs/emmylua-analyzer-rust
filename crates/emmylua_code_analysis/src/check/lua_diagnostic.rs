//! # LuaDiagnostic — diagnostic entry point (pure salsa path since M2)
//!
//! As of M2: `diagnose_file` only runs `analysis.diagnose_salsa` (the full `check/` checker set).
//! The old checker chain (`diagnostic/checker` + DbIndex + old SemanticModel) is disabled;
//! see `docs/SALSA_FROM_SCRATCH.md` §M2 for the decommission list.

use std::sync::Arc;

use crate::check::CheckConfig;
use crate::{DiagnosticCode, EmmyLuaAnalysis, Emmyrc, FileId};
use lsp_types::Diagnostic;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct LuaDiagnostic {
    enable: bool,
    check_config: Arc<CheckConfig>,
}

impl Default for LuaDiagnostic {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaDiagnostic {
    pub fn new() -> Self {
        Self {
            enable: true,
            check_config: Arc::new(CheckConfig::default()),
        }
    }

    pub fn update_config(&mut self, emmyrc: Arc<Emmyrc>) {
        self.enable = emmyrc.diagnostics.enable;
        self.check_config = CheckConfig::new(&emmyrc).into();
    }

    // Enable only the specified diagnostic
    pub fn enable_only(&mut self, code: DiagnosticCode) {
        let mut emmyrc = Emmyrc::default();
        emmyrc.diagnostics.enables.push(code);
        for diagnostic_code in DiagnosticCode::all().iter() {
            if *diagnostic_code != code {
                emmyrc.diagnostics.disable.push(*diagnostic_code);
            }
        }
        self.check_config = CheckConfig::new(&emmyrc).into();
    }

    pub fn diagnose_file(
        &self,
        analysis: &EmmyLuaAnalysis,
        file_id: FileId,
        cancel_token: CancellationToken,
    ) -> Option<Vec<Diagnostic>> {
        if !self.enable {
            return None;
        }
        if cancel_token.is_cancelled() {
            return None;
        }
        // Do not diagnose non-main workspace files (mirrors the old module_index.is_main semantics;
        // once salsa workspace scoping is mirrored, filter by workspace id).
        analysis.diagnose_salsa(file_id, self.check_config.clone())
    }
}
