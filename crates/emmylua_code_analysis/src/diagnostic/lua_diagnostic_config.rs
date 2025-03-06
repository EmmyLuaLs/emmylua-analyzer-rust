use std::collections::{HashMap, HashSet};

use lsp_types::DiagnosticSeverity;
use regex::Regex;
use smol_str::SmolStr;

use crate::Emmyrc;

use super::DiagnosticCode;

#[derive(Debug, Clone, Default)]
pub struct LuaDiagnosticConfig {
    pub workspace_enabled: HashSet<DiagnosticCode>,
    pub workspace_disabled: HashSet<DiagnosticCode>,
    pub global_disable_set: HashSet<SmolStr>,
    pub global_disable_glob: Vec<Regex>,
    pub severity: HashMap<DiagnosticCode, DiagnosticSeverity>
}

impl LuaDiagnosticConfig {
    pub fn new(emmyrc: &Emmyrc) -> Self {
        let workspace_disabled = emmyrc.diagnostics.disable.iter().cloned().collect();
        let workspace_enabled = emmyrc.diagnostics.enables.iter().cloned().collect();
        let global_disable_set = emmyrc
            .diagnostics
            .globals
            .iter()
            .map(|s| SmolStr::new(s.as_str()))
            .collect();

        let global_disable_glob = emmyrc
            .diagnostics
            .globals_regex
            .iter()
            .filter_map(|s| match Regex::new(s) {
                Ok(r) => Some(r),
                Err(e) => {
                    log::error!("Invalid regex: {}, error: {}", s, e);
                    None
                }
            })
            .collect();

        let mut severity = HashMap::new();
        for (code, sev) in &emmyrc.diagnostics.severity {
            severity.insert(code.clone(), sev.clone().into());
        }
        Self {
            workspace_disabled,
            workspace_enabled,
            global_disable_set,
            global_disable_glob,
            severity
        }
    }
}
