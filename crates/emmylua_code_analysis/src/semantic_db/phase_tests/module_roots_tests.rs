//! Caller-root and alias regressions ported from the retired LuaModuleIndex tests.
use crate::semantic_db::SemanticDatabase;
use crate::{Emmyrc, EmmyrcWorkspaceModuleMap, FileId, WorkspaceFolder};
use std::{path::PathBuf, sync::Arc};

fn root() -> PathBuf {
    crate::semantic_db::tests::portable_test_root()
}
fn setup(roots: &[&str], mapping: Option<(&str, &str)>) -> SemanticDatabase {
    let mut db = SemanticDatabase::new();
    let mut config = Emmyrc::default();
    if let Some((pattern, replace)) = mapping {
        config.workspace.module_map = vec![EmmyrcWorkspaceModuleMap {
            pattern: pattern.into(),
            replace: replace.into(),
        }];
    }
    db.update_config(Arc::new(config));
    for path in roots {
        db.add_main_workspace(root().join(path));
    }
    db
}
fn file(db: &mut SemanticDatabase, id: u32, path: &str, source: &str) -> FileId {
    let id = FileId::new(id);
    db.set_file(id, Some(root().join(path)), source.into());
    id
}

#[test]
fn caller_root_is_independent_of_root_and_file_registration_order() {
    for roots in [["", "a", "b"], ["b", "a", ""]] {
        for paths in [
            ["a/helper.lua", "b/helper.lua"],
            ["b/helper.lua", "a/helper.lua"],
        ] {
            let mut db = setup(&roots, None);
            let first = file(&mut db, 1, paths[0], "return {}");
            let second = file(&mut db, 2, paths[1], "return {}");
            let a = file(&mut db, 3, "a/scripts/control.lua", "");
            let b = file(&mut db, 4, "b/scripts/control.lua", "");
            let a_helper = if paths[0].starts_with('a') {
                first
            } else {
                second
            };
            let b_helper = if paths[0].starts_with('b') {
                first
            } else {
                second
            };
            assert_eq!(db.module_file_from("helper", a), Some(a_helper));
            assert_eq!(db.module_file_from("helper", b), Some(b_helper));
            assert_eq!(db.module_names_of(a_helper), vec!["helper"]);
        }
    }
}

#[test]
fn aliases_survive_overlapping_roots_and_refresh_after_rename_remove_and_clear() {
    let mut db = setup(&["", "a", "b"], Some(("^([ab])[.](.*)$", "package_$1.$2")));
    let a = file(&mut db, 1, "a/helper.lua", "return {}");
    let b = file(&mut db, 2, "b/helper.lua", "return {}");
    assert_eq!(db.module_file_from("helper", a), Some(a));
    assert_eq!(db.module_file_from("package_b/helper", a), Some(b));
    let node = db.module_node("package_b.helper").unwrap();
    assert_eq!(db.module_node_file_ids(node), vec![b]);
    file(&mut db, 2, "b/renamed.lua", "return {}");
    assert!(db.module_node("package_b.helper").is_none());
    assert_eq!(db.module_file_from("package_b.renamed", a), Some(b));
    db.remove_file(b);
    assert!(db.module_node("package_b").is_none());
    assert_eq!(db.module_file_from("helper", a), Some(a));
    db.clear();
    assert!(db.module_file_of("helper").is_none());
}

#[test]
fn canonical_and_alias_names_are_mapped_exactly_once() {
    let mut db = setup(&["", "a"], Some(("^(.*)$", "mapped.$1")));
    let a = file(&mut db, 1, "a/helper.lua", "return {}");
    assert_eq!(
        db.module_names_of(a),
        vec!["mapped.helper", "mapped.a.helper"]
    );
    for name in ["helper", "a.helper", "mapped.helper", "mapped.a.helper"] {
        assert_eq!(db.module_file_from(name, a), Some(a), "{name}");
    }
    // Replacing a file must neither duplicate nodes nor retain stale aliases.
    file(&mut db, 1, "a/helper.lua", "return 2");
    let node = db.module_node("mapped.a.helper").unwrap();
    assert_eq!(db.module_node_file_ids(node), vec![a]);
    db.remove_file(a);
    assert!(db.module_node("mapped").is_none());
}

#[test]
fn enclosing_roots_mapping_to_same_alias_are_deduplicated() {
    let mut db = setup(
        &["", "a", "a/lib"],
        Some(("^(a[.])?lib[.](.*)$", "shared.$2")),
    );
    let a = file(&mut db, 1, "a/lib/helper.lua", "return {}");
    assert_eq!(db.module_names_of(a), vec!["helper", "shared.helper"]);
    assert_eq!(
        db.module_node_file_ids(db.module_node("shared.helper").unwrap()),
        vec![a]
    );
    db.remove_file(a);
    assert!(db.module_node("shared").is_none());
}

#[test]
fn hidden_local_candidate_shadows_visible_remote_and_is_removed_cleanly() {
    let mut db = setup(&["a", "b"], None);
    let remote = file(&mut db, 1, "b/helper.lua", "return {}");
    let local = file(&mut db, 2, "a/helper.lua", "---@meta no-require\nreturn {}");
    let caller = file(&mut db, 3, "a/control.lua", "");
    assert_eq!(db.module_file_from("helper", caller), Some(local));
    assert_eq!(db.module_file_of("helper"), Some(remote));
    db.remove_file(local);
    assert_eq!(db.module_file_from("helper", caller), Some(remote));
}

#[test]
fn fuzzy_lookup_prefers_local_root_but_exact_names_and_libraries_still_work() {
    let mut db = setup(&["a", "b"], None);
    db.add_library_workspace(&WorkspaceFolder::new(root().join("library"), true));
    let caller = file(&mut db, 1, "a/control.lua", "");
    let local = file(&mut db, 2, "a/deep/lib/helper.lua", "return {}");
    let other = file(&mut db, 3, "b/lib/helper.lua", "return {}");
    let library = file(&mut db, 4, "library/shared.lua", "return {}");
    assert_eq!(db.module_file_from("helper", caller), Some(local));
    assert_eq!(db.module_file_from("lib.helper", caller), Some(other));
    assert_eq!(db.module_file_from("shared", caller), Some(library));
    db.remove_file(local);
    assert_eq!(db.module_file_from("helper", caller), Some(other));
}

#[test]
fn callers_without_registered_roots_keep_global_resolution() {
    let mut db = setup(&["a", "b"], None);
    file(&mut db, 1, "a/helper.lua", "return {}");
    file(&mut db, 2, "b/helper.lua", "return {}");
    let outside = file(&mut db, 3, "outside/control.lua", "");
    for caller in [outside, FileId::new(12345)] {
        for name in ["helper", "missing"] {
            assert_eq!(db.module_file_from(name, caller), db.module_file_of(name));
        }
    }
}

#[test]
fn changing_roots_rebuilds_canonical_names_and_aliases() {
    let mut db = setup(&[""], Some(("^a[.](.*)$", "package_a.$1")));
    let a = file(&mut db, 1, "a/helper.lua", "return {}");
    assert_eq!(db.module_names_of(a), vec!["package_a.helper"]);
    db.add_main_workspace(root().join("a"));
    assert_eq!(db.module_names_of(a), vec!["helper", "package_a.helper"]);
    assert_eq!(db.module_file_from("helper", a), Some(a));
    assert_eq!(db.module_file_from("package_a.helper", a), Some(a));
}

#[test]
fn literal_paths_are_not_shadowed_by_fuzzy_module_names() {
    let mut db = setup(&["a", "b"], Some(("^@/", "src/")));
    let caller = file(&mut db, 1, "a/control.lua", "");
    let other = file(&mut db, 2, "b/src/helper.lua", "return 1");
    let local = file(&mut db, 3, "a/src/helper.lua", "return 2");
    file(&mut db, 4, "a/lua.lua", "return 3");
    for name in ["src/helper.lua", "@/helper.lua", "src/helper", "@/helper"] {
        assert_eq!(db.module_file_from(name, caller), Some(local), "{name}");
    }
    db.remove_file(local);
    assert_eq!(db.module_file_from("src/helper.lua", caller), Some(other));
}

#[test]
fn import_filters_exclude_roots_from_caller_scope_and_aliases() {
    let mut db = setup(&["a"], Some(("^a[.](.*)$", "package_a.$1")));
    // Neither package root accepts files under a/lib, despite containing them.
    db.add_library_workspace(&WorkspaceFolder::with_package(root(), "b".into()));
    db.add_library_workspace(&WorkspaceFolder::with_package(
        root().join("a/lib"),
        "other".into(),
    ));
    let caller = file(&mut db, 1, "a/lib/control.lua", "");
    let target = file(&mut db, 2, "a/lib/helper.lua", "return {}");
    assert_eq!(db.module_names_of(target), vec!["lib.helper"]);
    assert_eq!(db.module_file_from("lib.helper", caller), Some(target));
    assert!(db.module_node("package_a.lib.helper").is_none());
    // Registering an already known main root must not duplicate module entries.
    db.add_main_workspace(root().join("a"));
    assert_eq!(
        db.module_node_file_ids(db.module_node("lib.helper").unwrap()),
        vec![target]
    );
}
