//! Tests for the require_module_visibility checker (new cross-file path: SalsaDatabase → check_file).
//!
//! M0 semantics: salsa has no multi-workspace split yet, so `Internal` modules are always treated as invisible outside the current project.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use lsp_types::Uri;

use crate::DiagnosticCode;
use crate::{Emmyrc, SalsaDatabase, SalsaSemanticModel};

use super::{Diagnostic, count_by_code};

fn check_require(def_source: &str, use_source: &str) -> Vec<Diagnostic> {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SalsaDatabase::new();
    db.update_config(emmyrc.clone());

    let def_uri = Uri::from_str("file:///C:/ws/mod.lua").expect("def uri");
    db.set_file_content(&def_uri, Some(def_source.to_string()));
    let use_uri = Uri::from_str("file:///C:/ws/main.lua").expect("use uri");
    let use_file = db.set_file_content(&use_uri, Some(use_source.to_string()));
    db.update_main_root(PathBuf::from("C:/ws"));

    let model = SalsaSemanticModel::new(&db, use_file).expect("semantic model");
    let config = Arc::new(crate::check::CheckConfig::new(&emmyrc));
    crate::check::check_file(&model, config)
}

/// `---@internal return {}` → external require is reported as not visible.
#[test]
fn test_internal_return_table_is_not_visible() {
    let diags = check_require("---@internal\nreturn {}", "local a = require('mod')");
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::RequireModuleNotVisible),
        1
    );
}

/// Default returned table → public, not reported.
#[test]
fn test_public_return_table_is_visible() {
    let diags = check_require("return {}", "local a = require('mod')");
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::RequireModuleNotVisible),
        0
    );
}

/// When returning a NameExpr, visibility labels on the return statement are invalid (old semantics).
#[test]
fn test_return_statement_visibility_label_on_name_expr_is_invalid() {
    let diags = check_require(
        "local m = {}\n---@internal\nreturn m",
        "local a = require('mod')",
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::RequireModuleNotVisible),
        0
    );
}

/// When returning a NameExpr, the visibility label on the declaration is used.
#[test]
fn test_internal_return_owner_is_not_visible() {
    let diags = check_require(
        "---@internal\nlocal m = {}\nreturn m",
        "local a = require('mod')",
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::RequireModuleNotVisible),
        1
    );
}

/// `---@meta no-require` → Hide, require reports not visible.
#[test]
fn test_meta_no_require_is_not_visible() {
    let diags = check_require("---@meta no-require\nreturn {}", "local a = require('mod')");
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::RequireModuleNotVisible),
        1
    );
}
