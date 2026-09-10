//! P0 baseline tests for the workspace-index redesign.
//!
//! These tests split into two groups:
//!
//! - **Passing regression guards**: they must keep passing while the incremental
//!   workspace-index layer is rewritten.
//! - **Ignored known-failure baselines**: they encode the correct behavior for
//!   cross-file owner resolution and overloads. Enable them phase by phase as the
//!   corresponding redesign work lands. See `docs/WORKSPACE_INDEX_REDESIGN.md`.
//!
//! The ignored tests intentionally fail today; do not delete them just to make CI
//! green. They are the acceptance criteria for phases 4 and 6.

use crate::{FileId, LuaType, VirtualWorkspace};

fn local_type(ws: &VirtualWorkspace, file_id: FileId, name: &str) -> LuaType {
    let model = ws.analysis.semantic_model(file_id);
    let facts = model.file_facts().expect("file facts");
    let decl = facts.decl_named(name).expect("local declaration");
    model.type_of_decl(&decl.id).expect("declaration type")
}

fn is_integer_like(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::Number
    )
}

fn is_string_like(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_)
    )
}

/// Incremental edits must produce the same query result as a fresh workspace with
/// the final file contents. This is a correctness guard, not a performance test.
#[test]
fn p0_incremental_module_member_edit_matches_full_rebuild() {
    let mut incremental = VirtualWorkspace::new();
    incremental.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    let consumer_id = incremental.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        "#,
    );

    let before = local_type(&incremental, consumer_id, "v");
    assert!(
        is_integer_like(&before),
        "initial module member type: {before:?}"
    );

    // Export-changing edit: module member x changes number -> string.
    incremental.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = "s"
        return M
        "#,
    );
    let after_incremental = local_type(&incremental, consumer_id, "v");
    assert!(
        is_string_like(&after_incremental),
        "incremental edit result: {after_incremental:?}"
    );

    // Fresh workspace with the same final contents.
    let mut fresh = VirtualWorkspace::new();
    fresh.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = "s"
        return M
        "#,
    );
    let fresh_consumer_id = fresh.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        "#,
    );
    let after_full = local_type(&fresh, fresh_consumer_id, "v");

    assert_eq!(
        after_incremental, after_full,
        "incremental update must match a full rebuild"
    );
}

/// Sequential incremental edits must keep matching a fresh workspace.
#[test]
fn p0_incremental_multi_edit_matches_full_rebuild() {
    let mut incremental = VirtualWorkspace::new();
    incremental.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    let consumer_id = incremental.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local a = m.x
        "#,
    );

    let a_before = local_type(&incremental, consumer_id, "a");
    assert!(is_integer_like(&a_before), "initial a: {a_before:?}");

    // Edit 1: change M.x's value.
    incremental.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = "s"
        return M
        "#,
    );
    let a_after = local_type(&incremental, consumer_id, "a");
    assert!(is_string_like(&a_after), "after edit 1: {a_after:?}");

    // Edit 2: add M.y and a consumer that reads it.
    incremental.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = "s"
        M.y = true
        return M
        "#,
    );
    incremental.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local a = m.x
        local b = m.y
        "#,
    );
    let a_final = local_type(&incremental, consumer_id, "a");
    let b_final = local_type(&incremental, consumer_id, "b");

    // Fresh workspace with the same final contents.
    let mut fresh = VirtualWorkspace::new();
    fresh.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = "s"
        M.y = true
        return M
        "#,
    );
    let fresh_consumer_id = fresh.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local a = m.x
        local b = m.y
        "#,
    );
    let a_full = local_type(&fresh, fresh_consumer_id, "a");
    let b_full = local_type(&fresh, fresh_consumer_id, "b");

    assert_eq!(a_final, a_full, "a after sequential edits");
    assert_eq!(b_final, b_full, "b after sequential edits");
    assert!(is_string_like(&a_final), "final a: {a_final:?}");
    assert!(
        matches!(b_final, LuaType::Boolean | LuaType::BooleanConst(true)),
        "final b: {b_final:?}"
    );
}

/// Regression guard for deep `require` member chains.
///
/// This already works for the direct-expression case; it must keep working while the
/// owner identity layer is rewritten.
#[test]
fn p0_deep_require_member_chain() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.sub = { value = 42 }
        return M
        "#,
    );

    let ty = ws.expr_ty("require('mod').sub.value");
    assert!(is_integer_like(&ty), "deep require member type: {ty:?}");
}

/// Correct behavior for a module table mutated from another file.
///
/// `extra.lua` adds a member to the table returned by `mod.lua`; a consumer must see
/// that member through the module export identity, not through `extra.lua`'s local alias.
#[test]
#[ignore = "P0 known failure: cross-file module mutation is attached to the consumer-local alias"]
fn p0_cross_file_module_mutation_visible_through_require() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        return M
        "#,
    );
    ws.def_file(
        "extra.lua",
        r#"
        local M = require("mod")
        function M.extra()
            return 42
        end
        "#,
    );

    let ty = ws.expr_ty("require('mod').extra()");
    assert!(
        is_integer_like(&ty),
        "cross-file module mutation return type: {ty:?}"
    );
}

/// Correct behavior for cross-file global overloads.
///
/// Two files declare the same global function with different parameter/return types;
/// call-site resolution must consider both declarations.
#[test]
#[ignore = "P0 known failure: global overload candidates collapse to one declaration"]
fn p0_cross_file_global_overloads() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "a.lua",
        r#"
        ---@param value string
        ---@return string
        function f(value) end
        "#,
    );
    ws.def_file(
        "b.lua",
        r#"
        ---@param value number
        ---@return number
        function f(value) end
        "#,
    );

    let string_result = ws.expr_ty("f('x')");
    let number_result = ws.expr_ty("f(1)");

    assert!(
        is_string_like(&string_result),
        "string overload result: {string_result:?}"
    );
    assert!(
        is_integer_like(&number_result),
        "number overload result: {number_result:?}"
    );
}

/// A surface-preserving single-file edit must not scan all files.
///
/// Changing `M.x`'s value does not change the exported member identity, so the
/// incremental path should only update this file's references and its shard.
#[test]
fn p0_surface_preserving_edit_does_not_scan_all_files() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    ws.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        "#,
    );

    ws.analysis.db.rebuild_metrics.reset();

    // Same member key/identity; only the assigned value changes.
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 2
        return M
        "#,
    );

    let metrics = &ws.analysis.db.rebuild_metrics;
    assert_eq!(metrics.full_rebuilds(), 0, "full rebuild must not run");
    assert_eq!(
        metrics.workspace_index_rebuilds(),
        0,
        "surface-preserving edit must not rebuild workspace indexes"
    );
    assert_eq!(
        metrics.shard_scan_builds(),
        0,
        "shard builders must not scan all files"
    );
}

/// An export-changing single-file edit must update workspace indexes incrementally.
///
/// Adding `M.y` changes the file's export contribution; the incremental index
/// layer must apply that delta instead of rebuilding every workspace index.
#[test]
fn p0_export_changing_edit_does_not_rebuild_workspace_indexes() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        return M
        "#,
    );
    ws.def_file(
        "consumer.lua",
        r#"
        local m = require("mod")
        local v = m.x
        "#,
    );

    ws.analysis.db.rebuild_metrics.reset();

    // Adding M.y changes the member contribution set.
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.x = 1
        M.y = 2
        return M
        "#,
    );

    let metrics = &ws.analysis.db.rebuild_metrics;
    assert_eq!(metrics.full_rebuilds(), 0, "full rebuild must not run");
    assert_eq!(
        metrics.workspace_index_rebuilds(),
        0,
        "workspace indexes must be updated from the old/new contribution delta"
    );
    assert_eq!(
        metrics.shard_scan_builds(),
        0,
        "shard builders must not scan all files"
    );
}
