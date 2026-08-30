//! Diagnostic tests for check (mirrors the old `diagnostic/test` via the new SalsaDatabase → check_file path).

mod access_invisible_test;
mod analyze_error_test;
mod assign_type_mismatch_test;
mod attribute_check_test;
mod await_in_sync_test;
mod call_non_callable_test;
mod cast_type_mismatch_test;
mod check_export_test;
mod check_return_count_test;
mod circle_doc_class_test;
mod code_style;
mod code_style_test;
mod deprecated_test;
mod disable_line_test;
mod duplicate_field_test;
mod duplicate_index_test;
mod duplicate_require_test;
mod duplicate_type_test;
mod enum_value_mismatch_test;
mod generic_constraint_mismatch_test;
mod global_in_non_module_test;
mod incomplete_signature_doc_test;
mod inject_field_test;
mod local_const_reassign_test;
mod misc_checker_test;
mod missing_fields_test;
mod missing_parameter_test;
mod need_check_nil_test;
mod param_count_test;
mod param_type_check_test;
mod readonly_test;
mod redefined_local_test;
mod redundant_parameter_test;
mod require_module_visibility_test;
mod return_type_mismatch_test;
mod syntax_error_test;
mod type_access_modifier_test;
mod type_mismatch_test;
mod unbalanced_assignments_test;
mod undefined_doc_param_test;
mod undefined_field_test;
mod undefined_global_test;
mod unknown_doc_tag_test;
mod unnecessary_assert_test;
mod unnecessary_if_test;
mod unresolved_require_test;
mod unused_test;

use std::str::FromStr;
use std::sync::Arc;

use lsp_types::Uri;

use crate::DiagnosticCode;
use crate::Emmyrc;

use super::checker::Diagnostic;

/// Run all checks on a single source file and return diagnostics.
pub(crate) fn check_source(source: &str) -> Vec<Diagnostic> {
    check_source_with_emmyrc(source, Emmyrc::default())
}

/// Run all checks on a single source file with a custom emmyrc and return diagnostics.
pub(crate) fn check_source_with_emmyrc(source: &str, emmyrc: Emmyrc) -> Vec<Diagnostic> {
    let emmyrc = Arc::new(emmyrc);
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri = Uri::from_str("file:///C:/ws/test.lua").expect("uri");
    let fid = db.set_file_content(&uri, Some(source.to_string()));
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let model = crate::SalsaSemanticModel::new(&db, fid).expect("salsa semantic model");
    let config = Arc::new(super::CheckConfig::new(&emmyrc));
    super::check_file(&model, config)
}

/// Count diagnostics for the specified code.
pub(crate) fn count_by_code(diagnostics: &[Diagnostic], code: DiagnosticCode) -> usize {
    diagnostics.iter().filter(|d| d.code == code).count()
}

/// End-to-end: EmmyLuaAnalysis::diagnose_salsa → lsp Diagnostic (range/code/severity).
#[test]
fn test_diagnose_salsa_end_to_end() {
    let mut analysis = crate::EmmyLuaAnalysis::new();
    let uri = Uri::from_str("file:///C:/ws/test.lua").expect("uri");
    let source = "local x = undefined_global\nlocal y = x + 1";
    let fid = analysis
        .update_file_by_uri(&uri, Some(source.to_string()))
        .expect("file id");

    let emmyrc = analysis.get_emmyrc();
    let config = Arc::new(super::CheckConfig::new(&emmyrc));
    let diagnostics = analysis.diagnose_salsa(fid, config).expect("diagnostics");

    // undefined_global → code + range points at the name.
    let undefined: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            matches!(
                &d.code,
                Some(lsp_types::NumberOrString::String(code))
                    if code == DiagnosticCode::UndefinedGlobal.get_name()
            )
        })
        .collect();
    assert_eq!(undefined.len(), 1, "diagnostics: {:?}", diagnostics);
    let diagnostic = undefined[0];
    assert_eq!(diagnostic.source.as_deref(), Some("EmmyLua"));
    assert!(diagnostic.severity.is_some());
    // Line 1 (0-based) `undefined_global` starts at col 10 (`local x = ` is 10 chars).
    assert_eq!(diagnostic.range.start.line, 0);
    assert_eq!(diagnostic.range.start.character, 10);
}

#[test]
fn test_check_config_default_enable_rules() {
    // Default-enabled codes are enabled in the default config.
    let config = super::CheckConfig::new(&Emmyrc::default());
    assert!(config.is_code_enabled(&DiagnosticCode::SyntaxError));
    assert!(config.is_code_enabled(&DiagnosticCode::UndefinedGlobal));
    // Default-disabled codes (UnknownDocTag, etc.) must not be reported.
    assert!(!config.is_code_enabled(&DiagnosticCode::UnknownDocTag));
    assert!(!config.is_code_enabled(&DiagnosticCode::IncompleteSignatureDoc));
    assert!(!config.is_code_enabled(&DiagnosticCode::NonLiteralExpressionsInAssert));
    // Lua 5.5+ enables IterVariableReassign by default; Lua 5.4 disables it.
    assert!(config.is_code_enabled(&DiagnosticCode::IterVariableReassign));
    let mut emmyrc54 = Emmyrc::default();
    emmyrc54.runtime.version = crate::config::EmmyrcLuaVersion::Lua54;
    let config54 = super::CheckConfig::new(&emmyrc54);
    assert!(!config54.is_code_enabled(&DiagnosticCode::IterVariableReassign));
    // Explicit disable/enable overrides the default.
    let mut emmyrc = Emmyrc::default();
    emmyrc.diagnostics.disable.push(DiagnosticCode::SyntaxError);
    emmyrc
        .diagnostics
        .enables
        .push(DiagnosticCode::UnknownDocTag);
    let config = super::CheckConfig::new(&emmyrc);
    assert!(!config.is_code_enabled(&DiagnosticCode::SyntaxError));
    assert!(config.is_code_enabled(&DiagnosticCode::UnknownDocTag));
}
