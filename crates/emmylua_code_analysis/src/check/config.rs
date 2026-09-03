//! Diagnostic filtering configuration (mirrors `diagnostic::lua_diagnostic_config`).
//!
//! Determines whether each candidate diagnostic is reported and at what severity. The checker does not emit directly — it passes through this configuration first.

use std::collections::{HashMap as StdHashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hashbrown::HashMap;
use lsp_types::DiagnosticSeverity;
use regex::Regex;
use smol_str::SmolStr;

use crate::{
    Emmyrc,
    check::{DiagnosticCode, get_default_severity, is_code_default_enable},
};
use emmylua_parser::LuaLanguageLevel;

/// Per-checker timing collector used by `--profile` runs.
#[derive(Debug, Default)]
pub struct CheckProfile {
    checker_times: Mutex<StdHashMap<&'static str, Duration>>,
}

impl CheckProfile {
    pub fn record(&self, checker: &'static str, elapsed: Duration) {
        if let Ok(mut times) = self.checker_times.lock() {
            *times.entry(checker).or_default() += elapsed;
        }
    }

    pub fn snapshot(&self) -> Vec<(&'static str, Duration)> {
        let mut times = match self.checker_times.lock() {
            Ok(times) => times.clone(),
            Err(_) => StdHashMap::new(),
        };
        let mut out: Vec<_> = times.drain().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1));
        out
    }
}

/// Diagnostic filtering and display configuration.
#[derive(Debug, Clone, Default)]
pub struct CheckConfig {
    /// Globally disabled diagnostic codes.
    disabled: HashSet<DiagnosticCode>,
    /// Globally explicitly enabled diagnostic codes (override defaults).
    enabled: HashSet<DiagnosticCode>,
    /// Override default severity.
    severity: HashMap<DiagnosticCode, DiagnosticSeverity>,
    /// `emmyrc.diagnostics.globals`: global names treated as defined (undefined_global not reported).
    global_disable_set: HashSet<SmolStr>,
    /// `emmyrc.diagnostics.globals_regex`: regular-expression whitelist for global names.
    global_disable_glob: Vec<Regex>,
    /// Lua language level (determines `is_code_default_enable`).
    level: LuaLanguageLevel,
    /// Optional per-checker timing collector for `--profile` runs.
    pub(crate) profile: Option<Arc<CheckProfile>>,
}

impl CheckConfig {
    pub fn new(emmyrc: &Emmyrc) -> Self {
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
        Self {
            disabled: emmyrc.diagnostics.disable.iter().cloned().collect(),
            enabled: emmyrc.diagnostics.enables.iter().cloned().collect(),
            severity: emmyrc
                .diagnostics
                .severity
                .iter()
                .map(|(code, setting)| (*code, (*setting).into()))
                .collect(),
            global_disable_set,
            global_disable_glob,
            level: emmyrc.get_language_level(),
            profile: None,
        }
    }

    pub fn with_profile(mut self, profile: Arc<CheckProfile>) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn profile(&self) -> Option<&Arc<CheckProfile>> {
        self.profile.as_ref()
    }

    /// Whether a diagnostic code is enabled (mirrors the old `is_checker_enable_by_code` default chain):
    /// disabled → enabled → `is_code_default_enable(code, level)`.
    pub fn is_code_enabled(&self, code: &DiagnosticCode) -> bool {
        if self.disabled.contains(code) {
            return false;
        }
        if self.enabled.contains(code) {
            return true;
        }
        is_code_default_enable(code, self.level)
    }

    /// Severity for a diagnostic code (configuration override, otherwise default).
    pub fn severity_of(&self, code: &DiagnosticCode) -> DiagnosticSeverity {
        self.severity
            .get(code)
            .copied()
            .unwrap_or_else(|| get_default_severity(*code))
    }

    /// Whether the name is in the `globals` / `globals_regex` whitelist (consumed by undefined_global).
    pub fn is_global_disabled(&self, name: &str) -> bool {
        if self.global_disable_set.contains(name) {
            return true;
        }
        self.global_disable_glob.iter().any(|re| re.is_match(name))
    }
}
