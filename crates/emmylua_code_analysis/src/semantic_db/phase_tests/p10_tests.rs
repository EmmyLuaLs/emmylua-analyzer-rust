//! P10 tests for semantic_db API convergence.
//!
//! The audit found two recurring "fake cheap" patterns:
//!
//! - workspace id list / lookup order were re-allocated and re-sorted on every call;
//! - type/member aggregate buckets were re-collected into a fresh `Arc` per query.
//!
//! These tests pin down the cached behavior and ensure the caches are correctly
//! invalidated by the incremental write path.

use std::path::PathBuf;
use std::sync::Arc;

use smol_str::SmolStr;

use crate::semantic_db::def::{SemanticId, TypeScope};
use crate::semantic_db::query::{self, workspace_type_index_for};
use crate::{FileId, SemanticDatabase, WorkspaceFolder, WorkspaceId};

fn setup() -> SemanticDatabase {
    let mut db = SemanticDatabase::new();
    db.update_config(Arc::new(crate::Emmyrc::default()));
    db.add_main_workspace(PathBuf::from("C:/ws"));
    db
}

fn set_test_file(db: &mut SemanticDatabase, file_id: u32, path: &str, source: &str) -> FileId {
    let fid = FileId::new(file_id);
    db.set_file(fid, Some(PathBuf::from(path)), source.to_string());
    fid
}

#[test]
fn p10_workspace_id_caches_follow_root_changes() {
    let mut db = setup();
    assert_eq!(
        db.all_workspace_ids(),
        &[WorkspaceId::MAIN, WorkspaceId::REMOTE]
    );
    assert_eq!(
        db.workspace_lookup_order(),
        &[WorkspaceId::MAIN, WorkspaceId::REMOTE]
    );

    // The query helpers return the cached slice, not a fresh Vec.
    let cached_ptr = db.all_workspace_ids().as_ptr();
    assert_eq!(query::all_workspace_ids(&db).as_ptr(), cached_ptr);
    let lookup_ptr = db.workspace_lookup_order().as_ptr();
    assert_eq!(query::workspace_lookup_order(&db).as_ptr(), lookup_ptr);

    // File exists before its library root is registered. The full rebuild that
    // follows `add_library_workspace` must rebuild facts/modules with the new
    // root rather than reusing stale FileCache workspace assignments.
    let fid = set_test_file(&mut db, 1, "C:/lib/mod.lua", "return {}");
    assert_eq!(db.workspace_id_of(fid), None);

    db.add_library_workspace(&WorkspaceFolder::new(PathBuf::from("C:/lib"), true));
    assert_eq!(db.all_workspace_ids().len(), 3);
    assert_eq!(db.workspace_lookup_order()[0], WorkspaceId::MAIN);
    assert!(db.workspace_lookup_order()[1].is_library());
    assert_eq!(db.workspace_lookup_order()[2], WorkspaceId::REMOTE);
    assert!(db.workspace_id_of(fid).is_some_and(|id| id.is_library()));
    assert_eq!(
        db.q().module_file_of("mod"),
        Some(fid),
        "module index must observe the newly registered root"
    );

    // File -> workspace is cached on FileCache and refreshed after a path change.
    set_test_file(&mut db, 1, "C:/ws/mod.lua", "return {}");
    assert_eq!(db.workspace_id_of(fid), Some(WorkspaceId::MAIN));
}

#[test]
fn p10_type_buckets_share_arc_and_invalidate_on_write() {
    let mut db = setup();
    set_test_file(&mut db, 1, "C:/ws/a.lua", "---@class A\nlocal A = {}");

    let index = workspace_type_index_for(&db, WorkspaceId::MAIN);
    let first = index.find_all(TypeScope::Global, "A");
    let second = index.find_all(TypeScope::Global, "A");
    assert!(Arc::ptr_eq(&first, &second), "type bucket must be cached");
    assert_eq!(first.len(), 1);

    // The single-workspace global query fast-path returns the same bucket Arc.
    let queried = query::type_defs_in_scope(&db, TypeScope::Global, "A".into());
    let queried_again = query::type_defs_in_scope(&db, TypeScope::Global, "A".into());
    assert!(Arc::ptr_eq(&queried, &queried_again));
    assert!(Arc::ptr_eq(&first, &queried));

    // Incremental write invalidates A and caches the new B bucket.
    set_test_file(&mut db, 1, "C:/ws/a.lua", "---@class B\nlocal B = {}");
    let index = workspace_type_index_for(&db, WorkspaceId::MAIN);
    assert!(index.find_all(TypeScope::Global, "A").is_empty());

    let b_first = index.find_all(TypeScope::Global, "B");
    let b_second = index.find_all(TypeScope::Global, "B");
    assert!(Arc::ptr_eq(&b_first, &b_second));
    assert_eq!(b_first.len(), 1);
}

#[test]
fn p10_global_type_aggregate_merges_workspaces_without_duplicates() {
    let mut db = setup();
    db.add_library_workspace(&WorkspaceFolder::new(PathBuf::from("C:/lib"), true));
    set_test_file(&mut db, 1, "C:/ws/a.lua", "---@class A\nlocal A = {}");
    set_test_file(&mut db, 2, "C:/lib/a.lua", "---@class A\nlocal A = {}");

    let defs = query::type_defs_in_scope(&db, TypeScope::Global, "A".into());
    let defs_again = query::type_defs_in_scope(&db, TypeScope::Global, "A".into());
    assert_eq!(defs.len(), 2, "each workspace contributes exactly once");
    assert!(
        Arc::ptr_eq(&defs, &defs_again),
        "cross-workspace global type aggregate must be cached"
    );
    assert_eq!(
        defs.iter().map(|def| def.file_id).collect::<Vec<_>>(),
        defs_again.iter().map(|def| def.file_id).collect::<Vec<_>>()
    );

    // The aggregate is refreshed for changed names on an incremental write.
    set_test_file(&mut db, 2, "C:/lib/a.lua", "---@class B\nlocal B = {}");
    let a_defs = query::type_defs_in_scope(&db, TypeScope::Global, "A".into());
    assert_eq!(a_defs.len(), 1);
    assert_eq!(a_defs[0].file_id, FileId::new(1));
    let b_defs = query::type_defs_in_scope(&db, TypeScope::Global, "B".into());
    let b_defs_again = query::type_defs_in_scope(&db, TypeScope::Global, "B".into());
    assert_eq!(b_defs.len(), 1);
    assert_eq!(b_defs[0].file_id, FileId::new(2));
    assert!(Arc::ptr_eq(&b_defs, &b_defs_again));

    db.remove_file(FileId::new(2));
    assert!(
        query::type_defs_in_scope(&db, TypeScope::Global, "B".into()).is_empty(),
        "removed file must drop its global type aggregate entry"
    );
    assert_eq!(
        query::type_defs_in_scope(&db, TypeScope::Global, "A".into()).len(),
        1
    );
}

#[test]
fn p10_member_aggregate_is_cached_across_workspaces() {
    let mut db = setup();
    db.add_library_workspace(&WorkspaceFolder::new(PathBuf::from("C:/lib"), true));
    set_test_file(&mut db, 1, "C:/ws/a.lua", "M.x = 1");
    set_test_file(&mut db, 2, "C:/lib/a.lua", "M.y = 2");

    let owner = SemanticId::name(SmolStr::new("M"));
    let first = query::members_of_owner(&db, owner.clone());
    let second = query::members_of_owner(&db, owner.clone());
    assert!(
        Arc::ptr_eq(&first, &second),
        "cross-workspace member aggregate must be cached"
    );
    assert_eq!(
        first
            .iter()
            .map(|member| member.name.to_string())
            .collect::<Vec<_>>(),
        vec!["x".to_string(), "y".to_string()]
    );

    let named_first = query::members_of_owner_named(&db, owner.clone(), SmolStr::new("y"));
    let named_second = query::members_of_owner_named(&db, owner.clone(), SmolStr::new("y"));
    assert!(
        Arc::ptr_eq(&named_first, &named_second),
        "cross-workspace named member aggregate must be cached"
    );
    assert_eq!(named_first.len(), 1);
    assert_eq!(named_first[0].name.as_str(), "y");

    // Replacing the library contribution refreshes the touched owner/name keys.
    set_test_file(&mut db, 2, "C:/lib/a.lua", "M.z = 3");
    let updated = query::members_of_owner(&db, owner.clone());
    let updated_again = query::members_of_owner(&db, owner.clone());
    assert!(Arc::ptr_eq(&updated, &updated_again));
    assert_eq!(
        updated
            .iter()
            .map(|member| member.name.to_string())
            .collect::<Vec<_>>(),
        vec!["x".to_string(), "z".to_string()]
    );
    assert!(
        query::members_of_owner_named(&db, owner.clone(), SmolStr::new("y")).is_empty(),
        "removed named member must be dropped from the aggregate"
    );
    let z = query::members_of_owner_named(&db, owner.clone(), SmolStr::new("z"));
    assert_eq!(z.len(), 1);
    assert_eq!(z[0].name.as_str(), "z");
}

#[test]
fn p10_member_buckets_share_arc_and_preserve_overloads() {
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

    let index = db
        .workspace_index_cache()
        .members
        .get(&WorkspaceId::MAIN)
        .expect("workspace member index");

    let f_first = index
        .members_of_owner_named(&owner, "f")
        .expect("f overloads");
    let f_second = index
        .members_of_owner_named(&owner, "f")
        .expect("f overloads");
    assert!(
        Arc::ptr_eq(&f_first, &f_second),
        "named member bucket must be cached"
    );
    assert_eq!(f_first.len(), 2);

    let all_first = index.members_of_owner(&owner).expect("owner members");
    let all_second = index.members_of_owner(&owner).expect("owner members");
    assert!(Arc::ptr_eq(&all_first, &all_second));
    assert_eq!(
        all_first
            .iter()
            .map(|member| member.name.to_string())
            .collect::<Vec<_>>(),
        vec!["f".to_string(), "f".to_string()]
    );

    // The query layer also returns the cached raw owner bucket.
    let queried_first = query::members_of_owner(&db, owner.clone());
    let queried_second = query::members_of_owner(&db, owner.clone());
    assert!(Arc::ptr_eq(&queried_first, &queried_second));
    assert_eq!(queried_first.len(), 2);

    // Adding g rebuilds the touched buckets once and keeps overload order.
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
    let index = db
        .workspace_index_cache()
        .members
        .get(&WorkspaceId::MAIN)
        .expect("workspace member index");
    let f_after = index
        .members_of_owner_named(&owner, "f")
        .expect("f overloads");
    assert_eq!(f_after.len(), 2);
    let g_after = index.members_of_owner_named(&owner, "g").expect("g field");
    assert_eq!(g_after.len(), 1);
    assert!(Arc::ptr_eq(
        &g_after,
        &index.members_of_owner_named(&owner, "g").expect("g field")
    ));
    assert_eq!(
        index
            .members_of_owner(&owner)
            .expect("owner members")
            .iter()
            .map(|member| member.name.to_string())
            .collect::<Vec<_>>(),
        vec!["f".to_string(), "f".to_string(), "g".to_string()]
    );

    // Removing the file contribution drops g and keeps the f overloads.
    set_test_file(
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
    let index = db
        .workspace_index_cache()
        .members
        .get(&WorkspaceId::MAIN)
        .expect("workspace member index");
    assert!(index.members_of_owner_named(&owner, "g").is_none());
    assert_eq!(
        index
            .members_of_owner_named(&owner, "f")
            .expect("f overloads")
            .len(),
        2
    );
}
