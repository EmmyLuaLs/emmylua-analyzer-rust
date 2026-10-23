use std::collections::HashMap;

use lsp_types::DiagnosticSeverity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::DiagnosticCode;

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
/// Represents the diagnostic configuration for Emmyrc.
pub struct EmmyrcDiagnostic {
    /// A list of diagnostic codes that are disabled.
    #[serde(default)]
    pub disable: Vec<DiagnosticCode>,
    /// A flag indicating whether diagnostics are enabled.
    #[serde(default = "default_true")]
    pub enable: bool,
    /// A list of global variables.
    #[serde(default)]
    pub globals: Vec<String>,
    /// A list of regular expressions for global variables.
    #[serde(default)]
    pub globals_regex: Vec<String>,
    /// A map of diagnostic codes to their severity settings.
    #[serde(default)]
    pub severity: HashMap<DiagnosticCode, DiagnosticSeveritySetting>,
    /// A list of diagnostic codes that are enabled.
    #[serde(default)]
    pub enables: Vec<DiagnosticCode>,
    /// Delay between opening/changing a file and scanning it for errors, in milliseconds.
    #[schemars(extend("x-vscode-setting" = true))]
    pub diagnostic_interval: Option<u64>,
    /// Also report diagnostics in `---@meta` files, which are skipped by default (to check an API library itself).
    #[serde(default)]
    pub check_meta_files: bool,
}

impl Default for EmmyrcDiagnostic {
    fn default() -> Self {
        Self {
            disable: Vec::new(),
            enable: default_true(),
            globals: Vec::new(),
            globals_regex: Vec::new(),
            severity: HashMap::new(),
            enables: Vec::new(),
            diagnostic_interval: Some(500),
            check_meta_files: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeveritySetting {
    /// Represents an error diagnostic severity.
    Error,
    /// Represents a warning diagnostic severity.
    Warning,
    /// Represents an information diagnostic severity.
    Information,
    /// Represents a hint diagnostic severity.
    Hint,
}

impl From<DiagnosticSeveritySetting> for DiagnosticSeverity {
    fn from(severity: DiagnosticSeveritySetting) -> Self {
        match severity {
            DiagnosticSeveritySetting::Error => DiagnosticSeverity::ERROR,
            DiagnosticSeveritySetting::Warning => DiagnosticSeverity::WARNING,
            DiagnosticSeveritySetting::Information => DiagnosticSeverity::INFORMATION,
            DiagnosticSeveritySetting::Hint => DiagnosticSeverity::HINT,
        }
    }
}
