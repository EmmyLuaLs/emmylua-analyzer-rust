//! P2 tests for FileCache-backed per-file facts.
//!
//! The shard layer was removed; workspace indexes now read FileCache
//! directly. These tests guard the per-file source of truth and the
//! incremental-update invariants that replaced the old shard tests.

use std::path::PathBuf;
use std::sync::Arc;

use crate::{Emmyrc, FileId};

use super::super::SemanticDatabase;

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
fn p2_file_cache_stores_module_and_deprecated_entries() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/pkg/mod.lua",
        "---@deprecated\nG = 1\nreturn {}",
    );
    let entry = db.file_module_entry_of(fid).expect("module entry");
    assert_eq!(entry.full_module_name, "pkg.mod");
    assert_eq!(entry.name, "mod");
    assert!(
        db.file_deprecated_of(fid)
            .is_some_and(|data| data.names.iter().any(|name| name == "G"))
    );
    assert_eq!(db.analysis().module_file_of("pkg.mod"), Some(fid));
}

#[test]
fn p2_module_entry_follows_path_change() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/old.lua", "return {}");
    assert_eq!(
        db.file_module_entry_of(fid)
            .map(|entry| entry.full_module_name.clone()),
        Some("old".into())
    );
    db.set_file(
        fid,
        Some(PathBuf::from("C:/ws/new.lua")),
        "return {}".to_string(),
    );
    assert_eq!(
        db.file_module_entry_of(fid)
            .map(|entry| entry.full_module_name.clone()),
        Some("new".into())
    );
    assert_eq!(db.analysis().module_file_of("old"), None);
    assert_eq!(db.analysis().module_file_of("new"), Some(fid));
}

#[test]
fn p2_incremental_write_does_not_rebuild_workspace_indexes() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/mod.lua", "return 1");
    db.rebuild_metrics.reset();
    db.set_file(
        fid,
        Some(PathBuf::from("C:/ws/mod.lua")),
        "---@deprecated\nG = 1\nreturn 2".to_string(),
    );
    assert_eq!(db.rebuild_metrics.full_rebuilds(), 0);
    assert_eq!(db.rebuild_metrics.full_index_source_scans(), 0);
    assert!(
        db.file_deprecated_of(fid)
            .is_some_and(|data| data.names.iter().any(|name| name == "G"))
    );
}
