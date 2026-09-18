//! P3 tests for incremental workspace-index containers.
//!
//! P3 adds per-file contributions and remove/add APIs to the four workspace
//! indexes. These tests apply a file change to a cloned incremental index and
//! compare it with a fresh full rebuild of the same final workspace.

use std::path::PathBuf;
use std::sync::Arc;

use super::super::super::query::{
    build_module_entry, workspace_module_index_for, workspace_reference_index_for,
    workspace_type_index_for,
};
use super::super::SemanticDatabase;
use super::super::def::TypeScope;
use crate::{Emmyrc, FileId, WorkspaceId};

fn setup() -> SemanticDatabase {
    let mut db = SemanticDatabase::new();
    db.update_config(Arc::new(Emmyrc::default()));
    db.add_main_workspace(PathBuf::from("C:/ws"));
    db
}

fn set_test_file(db: &mut SemanticDatabase, file_id: u32, path: &str, source: &str) -> FileId {
    let fid = FileId::new(file_id);
    db.set_file(fid, Some(PathBuf::from(path)), source.to_string());
    fid
}

#[test]
fn p3_type_index_remove_add_matches_full_rebuild() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "---@class A\nlocal A = {}");

    let mut incremental = workspace_type_index_for(&db, WorkspaceId::MAIN).clone();
    let a_defs = incremental.find_all(TypeScope::Global, "A");
    assert_eq!(a_defs.len(), 1);
    assert_eq!(a_defs[0].name.as_str(), "A");

    set_test_file(&mut db, 1, "C:/ws/a.lua", "---@class B\nlocal B = {}");
    let new_exports = db.file_exports_of(fid);
    incremental.remove_file(fid);
    incremental.add_file(WorkspaceId::MAIN, fid, new_exports);

    let mut fresh = setup();
    set_test_file(&mut fresh, 1, "C:/ws/a.lua", "---@class B\nlocal B = {}");
    let full = workspace_type_index_for(&fresh, WorkspaceId::MAIN);

    assert!(incremental.find_all(TypeScope::Global, "A").is_empty());
    assert_eq!(
        incremental
            .find_all(TypeScope::Global, "B")
            .iter()
            .map(|def| def.name.clone())
            .collect::<Vec<_>>(),
        full.find_all(TypeScope::Global, "B")
            .iter()
            .map(|def| def.name.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn p3_member_index_remove_add_preserves_overloads() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/c.lua",
        r#"
        ---@class C
        ---@field f fun(a: string): string
        ---@field f fun(a: number): number
        local C = {}
        "#,
    );

    let owner = db
        .file_exports_of(fid)
        .members
        .iter()
        .find(|member| member.key.name() == Some("f"))
        .expect("field f")
        .owner
        .clone();

    let mut incremental = db
        .workspace_index_cache()
        .members
        .get(&WorkspaceId::MAIN)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        incremental
            .members_of_owner_named(&owner, "f")
            .expect("f overloads")
            .len(),
        2
    );

    set_test_file(
        &mut db,
        1,
        "C:/ws/c.lua",
        r#"
        ---@class C
        ---@field f fun(a: string): string
        ---@field f fun(a: number): number
        ---@field g number
        local C = {}
        "#,
    );
    let new_exports = db.file_exports_of(fid);
    incremental.remove_file(fid);
    incremental.add_file(fid, new_exports);

    assert_eq!(
        incremental
            .members_of_owner_named(&owner, "f")
            .expect("f overloads")
            .len(),
        2
    );
    assert_eq!(
        incremental
            .members_of_owner_named(&owner, "g")
            .expect("g field")
            .len(),
        1
    );
}

#[test]
fn p3_reference_index_remove_add_matches_full_rebuild() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/ref.lua",
        "M = {}\nM.y = 1\nlocal x = M.y",
    );
    let ws = WorkspaceId::MAIN;
    let mut incremental = workspace_reference_index_for(&db, ws).clone();

    set_test_file(
        &mut db,
        1,
        "C:/ws/ref.lua",
        "M = {}\nM.y = 2\nlocal x = M.y\nlocal z = M.y",
    );
    let references = Arc::clone(&db.file_cache(fid).expect("cache").references);
    incremental.remove_file(fid);
    incremental.add_file(fid, references);

    let mut fresh = setup();
    set_test_file(
        &mut fresh,
        1,
        "C:/ws/ref.lua",
        "M = {}\nM.y = 2\nlocal x = M.y\nlocal z = M.y",
    );
    let full = workspace_reference_index_for(&fresh, ws);

    assert_eq!(incremental.decl_refs, full.decl_refs);
    assert_eq!(incremental.member_refs, full.member_refs);
    assert_eq!(incremental.member_defs, full.member_defs);
}

#[test]
fn p3_module_index_apply_file_change_is_workspace_local() {
    let mut db = setup();
    let fid1 = set_test_file(&mut db, 1, "C:/ws/mod.lua", "return {}");
    let fid2 = set_test_file(&mut db, 2, "C:/ws/other.lua", "return {}");
    let mut index = workspace_module_index_for(&db, WorkspaceId::MAIN).clone();

    let entry = build_module_entry(&db, fid2).expect("module entry");
    assert!(
        !index.apply_file_change(fid2, Some(entry)),
        "same module entry must not rebuild derived maps"
    );

    assert_eq!(
        index.module_info(fid1).map(|info| info.full_module_name),
        Some("mod".into())
    );
    assert_eq!(
        index.module_info(fid2).map(|info| info.full_module_name),
        Some("other".into())
    );

    index.apply_file_change(fid2, None);
    assert!(index.module_info(fid2).is_none());
    assert!(index.module_info(fid1).is_some());
}
