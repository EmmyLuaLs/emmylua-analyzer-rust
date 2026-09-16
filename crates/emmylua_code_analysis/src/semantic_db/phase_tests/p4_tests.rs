//! P4 tests for the incremental single-file update path.
//!
//! File add / remove / metadata changes must update workspace indexes from
//! old/new contributions without calling the full rebuild path.

use std::path::PathBuf;
use std::sync::Arc;

use super::super::SemanticDatabase;
use crate::{Emmyrc, FileId, LuaType, SemanticModel, WorkspaceId};

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

fn local_type(db: &SemanticDatabase, file_id: FileId, name: &str) -> LuaType {
    let model = SemanticModel::new(db, file_id);
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

#[test]
fn p4_new_file_does_not_full_rebuild() {
    let mut db = setup();
    set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = 1");
    db.rebuild_metrics.reset();

    let fid = set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1");

    assert_eq!(db.rebuild_metrics.full_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.workspace_index_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.shard_scan_builds(), 0);
    assert!(matches!(
        db.analysis().global_decl("M"),
        Some(crate::SemanticId::Decl(_))
    ));
    assert_eq!(db.analysis().module_file_of("b"), Some(fid));
}

#[test]
fn p4_remove_file_does_not_full_rebuild() {
    let mut db = setup();
    let fid_a = set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = M.x");
    let fid_b = set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1");
    assert!(is_integer_like(&local_type(&db, fid_a, "a")));

    db.rebuild_metrics.reset();
    db.remove_file(fid_b);

    assert_eq!(db.rebuild_metrics.full_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.workspace_index_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.shard_scan_builds(), 0);
    assert!(db.analysis().global_decl("M").is_none());
    assert!(db.analysis().module_file_of("b").is_none());
}

#[test]
fn p4_new_file_refreshes_dependent_references() {
    let mut db = setup();
    let fid_a = set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = M.x");
    assert_eq!(local_type(&db, fid_a, "a"), LuaType::Unknown);

    db.rebuild_metrics.reset();
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1");

    assert!(
        is_integer_like(&local_type(&db, fid_a, "a")),
        "dependent file must see the newly added global member"
    );
    assert_eq!(db.rebuild_metrics.full_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.workspace_index_rebuilds(), 0);
}

#[test]
fn p4_path_change_does_not_full_rebuild() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/old.lua", "return {}");
    assert_eq!(db.analysis().module_file_of("old"), Some(fid));

    db.rebuild_metrics.reset();
    set_test_file(&mut db, 1, "C:/ws/new.lua", "return {}");

    assert_eq!(db.rebuild_metrics.full_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.workspace_index_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.shard_scan_builds(), 0);
    assert!(db.analysis().module_file_of("old").is_none());
    assert_eq!(db.analysis().module_file_of("new"), Some(fid));

    // Workspace id stays stable for the same FileId.
    assert_eq!(db.workspace_id_of(fid), Some(WorkspaceId::MAIN));
}
