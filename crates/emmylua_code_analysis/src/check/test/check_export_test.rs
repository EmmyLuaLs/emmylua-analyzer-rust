//! Tests for the check_export checker (mirrors the test_export family from the old `diagnostic/test/inject_field_test.rs`).

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use lsp_types::Uri;

use crate::DiagnosticCode;
use crate::{Emmyrc, SalsaDatabase, SalsaSemanticModel};

use super::{Diagnostic, check_source, count_by_code};

fn check_export(def_source: &str, use_source: &str) -> Vec<Diagnostic> {
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

/// `return { a = 1 }`: imported tables may only read/write fields on the export surface.
#[test]
fn test_export_table_literal() {
    let diags = check_export(
        "return { a = 1 }",
        r#"
        local a = require("mod")
        a.newField = 1
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::InjectField), 1);

    let diags = check_export(
        "return { a = 1 }",
        r#"
        local a = require("mod")
        a.a = 2
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::InjectField), 0);

    let diags = check_export(
        "return { a = 1 }",
        r#"
        local a = require("mod")
        local v = a.newField
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedField), 1);
}

/// `local export = {}; export.a = 1; return export`: runtime members are also part of the export surface.
#[test]
fn test_export_runtime_members() {
    let diags = check_export(
        r#"
        local export = {}
        export.a = 1
        return export
        "#,
        r#"
        local a = require("mod")
        a.a = 2
        "#,
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::InjectField),
        0,
        "diagnostics: {:?}",
        diags
    );

    let diags = check_export(
        r#"
        local export = {}
        export.a = 1
        return export
        "#,
        r#"
        local a = require("mod")
        a.newField = 1
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::InjectField), 1);
}

/// Direct `require("mod").newField = 1`.
#[test]
fn test_export_direct_require() {
    let diags = check_export(
        "return { a = 1 }",
        r#"
        require("mod").newField = 1
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::InjectField), 1);
}

/// Non-require local tables are unrestricted.
#[test]
fn test_local_table_not_limited() {
    let diags = check_source(
        r#"
        local t = {}
        t.newField = 1
        local v = t.another
        "#,
    );
    assert_eq!(count_by_code(&diags, DiagnosticCode::InjectField), 0);
    assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedField), 0);
}
