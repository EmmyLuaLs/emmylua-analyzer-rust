//! P7 tests: identity-level dependency invalidation.
//!
//! A write publishes canonical `DependencyKey`s; only files whose recorded
//! dependencies intersect that set are refreshed.

use crate::{FileId, LuaType, VirtualWorkspace};

use super::def::{DependencyKey, OwnerId};
use crate::LuaMemberKey;

fn local_type(ws: &VirtualWorkspace, file_id: FileId, name: &str) -> LuaType {
    let model = ws.analysis.semantic_model(file_id);
    let facts = model.file_facts().expect("file facts");
    let decl = facts.decl_named(name).expect("local declaration");
    model.type_of_decl(&decl.id).expect("declaration type")
}

fn integer_like(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::Number
    )
}

#[test]
fn p7_dependency_index_records_identity_keys() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    let consumer = ws.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        return v
        "#,
    );
    let model = ws.analysis.semantic_model(consumer);
    let mod_file = model.module_file_of("mod").expect("mod file");

    let index = &ws.analysis.db.dependency_index;
    assert!(
        index
            .get(&DependencyKey::Module(mod_file))
            .is_some_and(|files| files.contains(&consumer)),
        "module dependency missing: {index:?}"
    );
    assert!(
        index
            .get(&DependencyKey::Member(
                OwnerId::Module(mod_file),
                LuaMemberKey::Name("x".into()),
            ))
            .is_some_and(|files| files.contains(&consumer)),
        "member dependency missing: {index:?}"
    );
}

#[test]
fn p7_value_only_member_edit_has_no_changed_keys() {
    let mut ws = VirtualWorkspace::new();
    let mod_file = ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    let before = ws.analysis.db.file_exports_of(mod_file).clone();

    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 2
        return M
        "#,
    );
    let after = ws.analysis.db.file_exports_of(mod_file).clone();

    let changed = super::query::changed_keys(Some(&before), Some(&after));
    assert!(
        changed.is_empty(),
        "value-only member edit must not publish identity keys: {changed:?}"
    );
}

#[test]
fn p7_unrelated_member_edit_does_not_refresh_dependent_files() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    let consumer = ws.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        return v
        "#,
    );
    assert!(integer_like(&local_type(&ws, consumer, "v")));

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        M.y = 2
        return M
        "#,
    );

    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_reference_refreshes(),
        0,
        "editing an unrelated member must not refresh the consumer"
    );
}

#[test]
fn p7_changed_member_refreshes_only_intersecting_files() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    let consumer = ws.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        return v
        "#,
    );
    let unrelated = ws.def_file("unrelated.lua", "local untouched = 1\nreturn untouched");
    assert!(integer_like(&local_type(&ws, consumer, "v")));

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        return M
        "#,
    );

    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_reference_refreshes(),
        1,
        "only the consumer referencing `mod.x` may be refreshed"
    );
    let _ = unrelated;

    // Incremental result must match a fresh workspace with the final contents.
    let mut fresh = VirtualWorkspace::new();
    fresh.def_file(
        "mod.lua",
        r#"
        local M = {}
        return M
        "#,
    );
    let fresh_consumer = fresh.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        return v
        "#,
    );
    assert_eq!(
        local_type(&ws, consumer, "v"),
        local_type(&fresh, fresh_consumer, "v"),
        "identity invalidation must match a full rebuild"
    );
}

#[test]
fn p7_global_dependency_refresh() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "a.lua",
        r#"
        ---@return number
        function f() end
        "#,
    );
    let consumer = ws.def_file("consumer.lua", "local v = f()\nreturn v");
    assert!(integer_like(&local_type(&ws, consumer, "v")));

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file(
        "a.lua",
        r#"
        function g() end
        "#,
    );

    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_reference_refreshes(),
        1,
        "removing global `f` must refresh its consumer"
    );
}
