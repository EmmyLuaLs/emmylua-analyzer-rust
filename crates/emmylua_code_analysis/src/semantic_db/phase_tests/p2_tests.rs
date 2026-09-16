//! P2 tests for per-file shard storage.
//!
//! P2 replaces aggregate shard vectors with `FileId -> contribution` maps, so a
//! single-file edit replaces exactly one entry and shard builders never scan the
//! full workspace file list.

use std::path::PathBuf;
use std::sync::Arc;

use super::super::super::query::{deprecated_shard, module_shard, reference_shard};
use super::super::SemanticDatabase;
use super::super::exports::{export_shard, shard_of};
use crate::{Emmyrc, FileId};

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
fn p2_export_shard_is_per_file() {
    let mut db = setup();
    // FileId 1 and 65 share the same stable shard (65 % 64 == 1).
    let fid1 = set_test_file(&mut db, 1, "C:/ws/a.lua", "M = {}\nM.x = 1");
    let fid2 = set_test_file(&mut db, 65, "C:/ws/b.lua", "N = {}\nN.y = 2");
    let shard = shard_of(fid1);
    assert_eq!(shard, shard_of(fid2));

    let shard = export_shard(&db, shard);
    assert_eq!(shard.files.len(), 2);
    let first_before = Arc::clone(shard.files.get(&fid1).expect("fid1 entry"));
    let second_before = Arc::clone(shard.files.get(&fid2).expect("fid2 entry"));

    // Edit only fid1: its entry must be replaced, fid2's entry must stay the same Arc.
    set_test_file(&mut db, 1, "C:/ws/a.lua", "M = {}\nM.x = 10\nM.z = 3");

    let shard = export_shard(&db, shard_of(fid1));
    let first_after = shard.files.get(&fid1).expect("fid1 entry after edit");
    let second_after = shard.files.get(&fid2).expect("fid2 entry after edit");

    assert!(
        !Arc::ptr_eq(&first_before, first_after),
        "edited file's contribution must be replaced"
    );
    assert!(
        Arc::ptr_eq(&second_before, second_after),
        "untouched file's contribution must be reused"
    );
}

#[test]
fn p2_shard_file_lists_are_maintained_incrementally() {
    let mut db = setup();
    let fid1 = set_test_file(&mut db, 1, "C:/ws/a.lua", "M = {}");
    let fid2 = set_test_file(&mut db, 65, "C:/ws/b.lua", "N = {}");
    let shard = shard_of(fid1);

    let files = db.file_ids_in_shard(shard);
    assert!(files.contains(&fid1));
    assert!(files.contains(&fid2));

    db.remove_file(fid1);
    let files = db.file_ids_in_shard(shard);
    assert!(!files.contains(&fid1));
    assert!(files.contains(&fid2));
}

#[test]
fn p2_module_and_deprecated_shards_are_per_file() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/mod.lua",
        "---@deprecated\nM = {}\nreturn M",
    );
    let shard = shard_of(fid);

    let deprecated = deprecated_shard(&db, shard);
    assert!(
        deprecated
            .files
            .get(&fid)
            .is_some_and(|data| data.names.iter().any(|name| name == "M")),
        "deprecated global M must be stored under its file"
    );
    let module = module_shard(&db, shard);
    assert_eq!(
        module
            .files
            .get(&fid)
            .map(|entry| entry.full_module_name.as_str()),
        Some("mod")
    );

    // Remove @deprecated: only this file's deprecated entry is removed.
    set_test_file(&mut db, 1, "C:/ws/mod.lua", "M = {}\nreturn M");
    assert!(
        deprecated_shard(&db, shard).files.get(&fid).is_none(),
        "deprecated entry must be removed with the annotation"
    );
    assert!(
        module_shard(&db, shard).files.contains_key(&fid),
        "module entry must remain while the file still returns M"
    );

    // A file without a top-level return is still a module path entry.
    set_test_file(&mut db, 1, "C:/ws/mod.lua", "M = {}");
    assert!(
        module_shard(&db, shard).files.contains_key(&fid),
        "module path entry must remain even without an explicit return"
    );

    let references = reference_shard(&db, shard);
    assert!(references.files.contains_key(&fid));
}
