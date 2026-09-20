//! P7 tests: identity-level dependency invalidation.
//!
//! A write publishes canonical `DependencyKey`s; only files whose recorded
//! dependencies intersect that set are refreshed.

use crate::{FileId, LuaType, VirtualWorkspace};

use crate::LuaMemberKey;
use crate::semantic_db::def::{DependencyKey, OwnerId, TypeScope};
use crate::semantic_db::query::changed_keys;

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

    let changed = changed_keys(Some(&before), Some(&after));
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

#[test]
fn p7_type_dependency_is_recorded_and_refreshed() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file("a.lua", "---@class Hidden\nlocal x = 1\nreturn x");
    let consumer = ws.def_file("c.lua", "local y = Hidden\nreturn y");

    let has_type_dep = ws.analysis.db.dependency_index.iter().any(|(key, files)| {
        matches!(key, DependencyKey::Type(_, name) if name == "Hidden") && files.contains(&consumer)
    });
    assert!(has_type_dep, "name use must record a Type dependency");

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file("a.lua", "---@class Hidden2\nlocal x = 1\nreturn x");

    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_reference_refreshes(),
        1,
        "type surface edit must refresh the consumer reference index"
    );
}

#[test]
fn p7_unresolved_require_refreshes_when_module_added() {
    let mut ws = VirtualWorkspace::new();
    let consumer = ws.def_file(
        "consumer.lua",
        "local m = require(\"late\")\nlocal v = m.x\nreturn v",
    );
    assert_eq!(local_type(&ws, consumer, "v"), LuaType::Unknown);

    ws.def_file("late.lua", "local M = {}\nM.x = 1\nreturn M");

    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_contribution_refreshes(),
        1,
        "resolving a require alias must rebuild the consumer contribution"
    );
    assert!(
        integer_like(&local_type(&ws, consumer, "v")),
        "adding the required module must rebuild the consumer alias contribution"
    );

    let mut fresh = VirtualWorkspace::new();
    fresh.def_file("late.lua", "local M = {}\nM.x = 1\nreturn M");
    let fresh_consumer = fresh.def_file(
        "consumer.lua",
        "local m = require(\"late\")\nlocal v = m.x\nreturn v",
    );
    assert_eq!(
        local_type(&ws, consumer, "v"),
        local_type(&fresh, fresh_consumer, "v")
    );
}

#[test]
fn p7_annotation_only_member_refreshes_dependent_references() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file("a.lua", "---@class A\nA = {}");
    let consumer = ws.def_file("c.lua", "local v = A.BBB()\nreturn v");
    assert_eq!(local_type(&ws, consumer, "v"), LuaType::Unknown);

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file("a.lua", "---@class A\n---@field BBB fun(): integer\nA = {}");
    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_reference_refreshes(),
        1,
        "annotation-only member addition must refresh the consumer reference index"
    );
    assert!(
        integer_like(&local_type(&ws, consumer, "v")),
        "BBB field must be visible after the incremental update"
    );
}

#[test]
fn p7_removed_module_refreshes_consumer_alias() {
    let mut ws = VirtualWorkspace::new();
    let late = ws.def_file("late.lua", "local M = {}\nM.x = 1\nreturn M");
    let consumer = ws.def_file(
        "consumer.lua",
        "local m = require(\"late\")\nlocal v = m.x\nreturn v",
    );
    assert!(integer_like(&local_type(&ws, consumer, "v")));

    ws.analysis.db.remove_file(late);

    assert_eq!(
        local_type(&ws, consumer, "v"),
        LuaType::Unknown,
        "removing the module must rebuild the consumer alias contribution"
    );
}

#[test]
fn p7_unresolved_type_refreshes_when_type_added() {
    let mut ws = VirtualWorkspace::new();
    let consumer = ws.def_file("c.lua", "local v = LateType\nreturn v");

    assert!(
        ws.analysis
            .db
            .dependency_index
            .get(&DependencyKey::TypeName("LateType".into()))
            .is_some_and(|files| files.contains(&consumer)),
        "unresolved name must record a negative TypeName dependency"
    );

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file("a.lua", "---@class LateType");

    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_reference_refreshes(),
        1,
        "adding the type must refresh the consumer reference index"
    );
    assert!(
        ws.analysis.db.dependency_index.iter().any(|(key, files)| {
            matches!(key, DependencyKey::Type(_, name) if name == "LateType")
                && files.contains(&consumer)
        }),
        "refreshed name use must record the positive Type dependency"
    );
}

#[test]
fn p7_unresolved_member_owner_type_refreshes_when_type_added() {
    let mut ws = VirtualWorkspace::new();
    let consumer = ws.def_file("c.lua", "local v = A.BBB\nreturn v");

    assert!(
        ws.analysis
            .db
            .dependency_index
            .get(&DependencyKey::TypeName("A".into()))
            .is_some_and(|files| files.contains(&consumer)),
        "unresolved member owner must record a negative TypeName dependency"
    );

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file("a.lua", "---@class A");

    assert_eq!(
        ws.analysis
            .db
            .rebuild_metrics
            .dependent_reference_refreshes(),
        1,
        "adding the type must refresh the unresolved member owner"
    );

    let type_owner = OwnerId::Type(TypeScope::Global, "A".into());
    assert!(
        ws.analysis
            .db
            .dependency_index
            .get(&DependencyKey::Member(
                type_owner,
                LuaMemberKey::Name("BBB".into()),
            ))
            .is_some_and(|files| files.contains(&consumer)),
        "refreshed member use must record the Type owner dependency"
    );
}
