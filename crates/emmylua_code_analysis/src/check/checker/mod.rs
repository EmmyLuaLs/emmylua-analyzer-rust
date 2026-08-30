//! Checker collection: `Checker` trait + `CheckContext` (collect + filter) + `check_file` (registry).
//! Mirrors `diagnostic::checker`; each check covers one file.

pub mod access_invisible;
pub mod analyze_error;
pub mod assign_type_mismatch;
pub mod attribute_check;
pub mod await_in_sync;
pub mod call_non_callable;
pub mod cast_type_mismatch;
pub mod check_export;
pub mod check_field;
pub mod check_return_count;
pub mod circle_doc_class;
pub mod code_style;
pub mod deprecated;
pub mod discard_returns;
pub mod duplicate_field;
pub mod duplicate_index;
pub mod duplicate_require;
pub mod duplicate_type;
pub mod enum_value_mismatch;
pub mod generic_constraint_mismatch;
pub mod global_non_module;
pub mod incomplete_signature_doc;
pub mod local_const_reassign;
pub mod need_check_nil;
pub mod param_count;
pub mod param_type_check;
pub mod readonly;
pub mod redefined_local;
pub mod require_module_visibility;
pub mod return_type_mismatch;
pub mod syntax_error;
pub mod type_access_modifier;
pub mod unbalanced_assignments;
pub mod undefined_doc_param;
pub mod undefined_field;
pub mod undefined_global;
pub mod unknown_doc_tag;
pub mod unnecessary_assert;
pub mod unnecessary_if;
pub mod unresolved_require;
pub mod unused;

use std::sync::Arc;

use lsp_types::{DiagnosticSeverity, DiagnosticTag};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::config::CheckConfig;

/// A diagnostic result entry (decoupled from LSP via TextRange; consumers convert to LSP).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub range: TextRange,
    pub code: DiagnosticCode,
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub tags: Option<Vec<DiagnosticTag>>,
    /// Extra data (consumed by code actions; e.g. tag name for unknown_doc_tag).
    pub data: Option<serde_json::Value>,
}

/// Checker trait (mirrors `diagnostic::checker::Checker`).
pub trait Checker {
    /// Diagnostic codes handled by this check (used to skip the whole check by config).
    const CODES: &[DiagnosticCode];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>);
}

/// If all codes for this check are disabled in config, skip the whole check.
/// A file-level `---@diagnostic enable: code` can force it on (mirrors legacy `is_checker_enable_by_code`).
fn run_check<T: Checker>(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
    let file_enabled = semantic_model
        .file_facts()
        .map(|facts| &facts.file_diagnostic_enabled);
    if T::CODES.iter().any(|code| {
        context.config.is_code_enabled(code)
            || file_enabled.is_some_and(|enabled| enabled.contains(code))
    }) {
        T::check(context, semantic_model);
    }
}

/// Check context: collects candidate diagnostics, then filters through `CheckConfig` before enqueueing.
pub struct CheckContext<'a> {
    pub semantic_model: &'a SemanticModel<'a>,
    pub(crate) config: Arc<CheckConfig>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> CheckContext<'a> {
    pub fn new(semantic_model: &'a SemanticModel<'a>, config: Arc<CheckConfig>) -> Self {
        Self {
            semantic_model,
            config,
            diagnostics: Vec::new(),
        }
    }

    /// Collects a diagnostic (filtered first: disabled codes are dropped; severity/tags determined by config).
    ///
    /// Filter order mirrors legacy `DiagnosticContext::add_diagnostic`:
    /// file-level force-enable -> global config -> file-level disable -> per-line `---@diagnostic disable*`.
    pub fn add_diagnostic<T: AsRef<str>>(
        &mut self,
        code: DiagnosticCode,
        range: TextRange,
        message: T,
    ) {
        self.add_diagnostic_with_data(code, range, message.as_ref().into(), None);
    }

    /// Collects a diagnostic with extra data (consumed by code actions etc.).
    pub fn add_diagnostic_with_data(
        &mut self,
        code: DiagnosticCode,
        range: TextRange,
        message: String,
        data: Option<serde_json::Value>,
    ) {
        if let Some(facts) = self.semantic_model.file_facts() {
            let force_enable = facts.file_diagnostic_enabled.contains(&code);
            if !force_enable {
                if !self.config.is_code_enabled(&code) {
                    return;
                }
                if facts.file_diagnostic_disabled.contains(&code) {
                    return;
                }
            }
            if facts.is_range_diagnostic_disabled(&code, &range) {
                return;
            }
        } else if !self.config.is_code_enabled(&code) {
            return;
        }
        let severity = self.config.severity_of(&code);
        let tags = default_tags(&code);
        self.diagnostics.push(Diagnostic {
            range,
            code,
            message,
            severity,
            tags,
            data,
        });
    }

    /// Whether the name is in the `emmyrc.diagnostics.globals` / `globals_regex` allowlist.
    pub fn is_global_disabled(&self, name: &str) -> bool {
        self.config.is_global_disabled(name)
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

/// Runs all registered checks on a single file (filtered by config).
#[allow(dead_code)]
pub fn check_file(semantic_model: &SemanticModel<'_>, config: Arc<CheckConfig>) -> Vec<Diagnostic> {
    let mut context = CheckContext::new(semantic_model, config);
    run_check::<syntax_error::SyntaxErrorChecker>(&mut context, semantic_model);
    run_check::<analyze_error::AnalyzeErrorChecker>(&mut context, semantic_model);
    run_check::<unused::UnusedChecker>(&mut context, semantic_model);
    run_check::<deprecated::DeprecatedChecker>(&mut context, semantic_model);
    run_check::<undefined_global::UndefinedGlobal>(&mut context, semantic_model);
    run_check::<undefined_doc_param::UndefinedDocParamChecker>(&mut context, semantic_model);
    run_check::<unbalanced_assignments::UnbalancedAssignmentsChecker>(&mut context, semantic_model);
    run_check::<redefined_local::RedefinedLocalChecker>(&mut context, semantic_model);
    run_check::<duplicate_require::DuplicateRequireChecker>(&mut context, semantic_model);
    run_check::<duplicate_type::DuplicateTypeChecker>(&mut context, semantic_model);
    run_check::<type_access_modifier::InconsistentTypeAccessModifierChecker>(
        &mut context,
        semantic_model,
    );
    run_check::<circle_doc_class::CircleDocClassChecker>(&mut context, semantic_model);
    run_check::<unknown_doc_tag::UnknownDocTagChecker>(&mut context, semantic_model);
    run_check::<local_const_reassign::LocalConstReassignChecker>(&mut context, semantic_model);
    run_check::<param_type_check::ParamTypeChecker>(&mut context, semantic_model);
    run_check::<return_type_mismatch::ReturnTypeMismatchChecker>(&mut context, semantic_model);
    run_check::<assign_type_mismatch::AssignTypeMismatchChecker>(&mut context, semantic_model);
    run_check::<code_style::invert_if::InvertIfChecker>(&mut context, semantic_model);
    run_check::<code_style::non_literal_expressions_in_assert::NonLiteralExpressionsInAssertChecker>(
        &mut context,
        semantic_model,
    );
    run_check::<code_style::preferred_local_alias::PreferredLocalAliasChecker>(
        &mut context,
        semantic_model,
    );
    run_check::<generic_constraint_mismatch::GenericConstraintMismatchChecker>(
        &mut context,
        semantic_model,
    );
    run_check::<param_count::ParamCountChecker>(&mut context, semantic_model);
    run_check::<check_return_count::CheckReturnCountChecker>(&mut context, semantic_model);
    run_check::<incomplete_signature_doc::IncompleteSignatureDocChecker>(
        &mut context,
        semantic_model,
    );
    run_check::<unnecessary_assert::UnnecessaryAssertChecker>(&mut context, semantic_model);
    run_check::<unnecessary_if::UnnecessaryIfChecker>(&mut context, semantic_model);
    run_check::<await_in_sync::AwaitInSyncChecker>(&mut context, semantic_model);
    run_check::<access_invisible::AccessInvisibleChecker>(&mut context, semantic_model);
    run_check::<cast_type_mismatch::CastTypeMismatchChecker>(&mut context, semantic_model);
    run_check::<call_non_callable::CallNonCallableChecker>(&mut context, semantic_model);
    run_check::<enum_value_mismatch::EnumValueMismatchChecker>(&mut context, semantic_model);
    run_check::<attribute_check::AttributeCheckChecker>(&mut context, semantic_model);
    run_check::<need_check_nil::NeedCheckNilChecker>(&mut context, semantic_model);
    run_check::<unresolved_require::UnresolvedRequireChecker>(&mut context, semantic_model);
    run_check::<require_module_visibility::RequireModuleVisibilityChecker>(
        &mut context,
        semantic_model,
    );
    run_check::<undefined_field::UndefinedFieldChecker>(&mut context, semantic_model);
    run_check::<check_export::CheckExportChecker>(&mut context, semantic_model);
    run_check::<duplicate_field::DuplicateFieldChecker>(&mut context, semantic_model);
    run_check::<duplicate_index::DuplicateIndexChecker>(&mut context, semantic_model);
    run_check::<readonly::ReadOnlyChecker>(&mut context, semantic_model);
    run_check::<check_field::CheckFieldChecker>(&mut context, semantic_model);
    run_check::<discard_returns::DiscardReturnsChecker>(&mut context, semantic_model);
    run_check::<global_non_module::GlobalInNonModuleChecker>(&mut context, semantic_model);
    context.into_diagnostics()
}

fn default_tags(code: &DiagnosticCode) -> Option<Vec<DiagnosticTag>> {
    match code {
        DiagnosticCode::Unused | DiagnosticCode::UnreachableCode => {
            Some(vec![DiagnosticTag::UNNECESSARY])
        }
        DiagnosticCode::Deprecated => Some(vec![DiagnosticTag::DEPRECATED]),
        _ => None,
    }
}
