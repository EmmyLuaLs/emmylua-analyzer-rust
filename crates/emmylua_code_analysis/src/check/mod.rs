//! # check — new diagnostics (mirrors `diagnostic/`; only access via `semantic_model::SemanticModel`)
//!
//! Filtering system: `CheckConfig` (enable/disable/severity) takes effect in `CheckContext::add_diagnostic`;
//! checkers only produce candidate diagnostics and configuration decides whether they are reported.
//!
//! The old `diagnostic/` based on DbIndex + old SemanticModel is kept for reference and not removed yet.

pub mod builtin;
pub mod checker;
pub mod config;
mod diagnostic_code;
mod lua_diagnostic;
#[cfg(test)]
mod test;

/// Entry point that runs all checks on a single file (filtered by configuration).
pub use checker::check_file;
/// Diagnostic filtering configuration.
pub use config::{CheckConfig, CheckProfile};
pub use diagnostic_code::{DiagnosticCode, get_default_severity, is_code_default_enable};
pub use lua_diagnostic::LuaDiagnostic;
