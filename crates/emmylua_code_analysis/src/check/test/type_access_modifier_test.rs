//! Tests for the type_access_modifier checker (new path: SalsaDatabase → check_file).
//!
//! M0 semantics: salsa has no multi-workspace split yet, so all Internal types belong to `WorkspaceId::MAIN`;
//! The old scenario of "different workspaces do not affect each other" is deferred until Phase 5 workspace scopes are mirrored.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use lsp_types::Uri;

use crate::DiagnosticCode;
use crate::{Emmyrc, SalsaDatabase, SalsaSemanticModel};

use super::{Diagnostic, check_source, count_by_code};

/// `(file) Foo` defined in another file does not affect `Foo` in the current file.
fn check_with_other_file(other_source: &str, main_source: &str) -> Vec<Diagnostic> {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SalsaDatabase::new();
    db.update_config(emmyrc.clone());

    let other_uri = Uri::from_str("file:///C:/ws/other.lua").expect("other uri");
    db.set_file_content(&other_uri, Some(other_source.to_string()));
    let main_uri = Uri::from_str("file:///C:/ws/main.lua").expect("main uri");
    let main_file = db.set_file_content(&main_uri, Some(main_source.to_string()));
    db.update_main_root(PathBuf::from("C:/ws"));

    let model = SalsaSemanticModel::new(&db, main_file).expect("semantic model");
    let config = Arc::new(crate::check::CheckConfig::new(&emmyrc));
    crate::check::check_file(&model, config)
}

/// Same-file public + internal same-name type → reported.
#[test]
fn test_explicit_public_and_internal_report_inconsistency() {
    let diags = check_source(
        "---@class (public) Foo\nlocal Foo = {}\n---@class (internal) Foo\nlocal FooInternal = {}",
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InconsistentTypeAccessModifier),
        2
    );
}

/// Implicit public + explicit public → consistent, not reported.
#[test]
fn test_implicit_and_explicit_public_stay_consistent() {
    let diags =
        check_source("---@class Foo\nlocal Foo = {}\n---@class (public) Foo\nlocal FooPublic = {}");
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InconsistentTypeAccessModifier),
        0
    );
}

/// Implicit public + internal → reported.
#[test]
fn test_implicit_public_and_internal_report_inconsistency() {
    let diags = check_source(
        "---@class Foo\nlocal Foo = {}\n---@class (internal) Foo\nlocal FooInternal = {}",
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InconsistentTypeAccessModifier),
        2
    );
}

/// File private + implicit public → reported.
#[test]
fn test_file_and_implicit_public_report_inconsistency() {
    let diags =
        check_source("---@class (file) Foo\nlocal Foo = {}\n---@class Foo\nlocal FooPublic = {}");
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InconsistentTypeAccessModifier),
        2
    );
}

/// Same-file partial internal + partial internal → consistent, not reported.
#[test]
fn test_partial_internal_stay_consistent() {
    let diags = check_source(
        "---@class (partial,internal) Foo\nlocal Foo = {}\n---@class (partial,internal) Foo\nlocal FooInternal = {}",
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InconsistentTypeAccessModifier),
        0
    );
}

/// Partial public + partial internal → reported.
#[test]
fn test_partial_public_and_internal_report_inconsistency() {
    let diags = check_source(
        "---@class (partial,public) Foo\nlocal Foo = {}\n---@class (partial,internal) Foo\nlocal FooInternal = {}",
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InconsistentTypeAccessModifier),
        2
    );
}

/// `(file) Foo` in other files does not affect the current file.
#[test]
fn test_file_types_in_other_files_do_not_affect_current_file() {
    let diags = check_with_other_file(
        "---@class (file) Foo\nlocal Foo = {}",
        "---@class Foo\nlocal Foo = {}",
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InconsistentTypeAccessModifier),
        0
    );
}
