//! P1 tests for canonical export identities and `FileExportContribution`.
//!
//! These tests lock the data model the incremental workspace-index layer will
//! consume in P2/P3:
//!
//! - every member contribution carries a canonical `OwnerId`;
//! - every member contribution preserves its source order (overloads stay separate);
//! - the contribution carries `value_syntax` / `is_method` / `visibility`;
//! - the file cache stores the contribution as `Arc<FileExportContribution>`.

use std::path::PathBuf;
use std::sync::Arc;

use emmylua_parser::VisibilityKind;

use super::SemanticDatabase;
use super::def::{ExportKey, OwnerId, TypeScope};
use crate::{Emmyrc, FileId};

fn setup() -> SemanticDatabase {
    let mut db = SemanticDatabase::new();
    db.update_config(Arc::new(Emmyrc::default()));
    db
}

fn set_test_file(db: &mut SemanticDatabase, file_id: u32, path: &str, source: &str) -> FileId {
    let fid = FileId::new(file_id);
    db.set_file(fid, Some(PathBuf::from(path)), source.to_string());
    fid
}

#[test]
fn p1_member_contribution_carries_canonical_owner_and_flags() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        r#"
        ---@class C
        local C = {}

        ---@field public public_field number
        ---@field private hidden_field string
        function C:method() end

        M = {}
        M.global_field = 1
        local local_table = {}
        local_table.local_field = 2
        "#,
    );

    let exports = db.file_exports_of(fid);
    let find = |name: &str| {
        exports
            .members
            .iter()
            .find(|member| member.key.name() == Some(name))
            .unwrap_or_else(|| panic!("member {name} not found"))
    };

    let public_field = find("public_field");
    assert_eq!(
        public_field.owner_id,
        OwnerId::Type(TypeScope::Global, "C".into()),
        "class @field owner must be canonical Type(Global, C)"
    );
    assert_eq!(public_field.visibility, VisibilityKind::Public);
    assert!(!public_field.is_method);
    assert!(
        public_field.value_syntax.is_some(),
        "@field type syntax must be preserved"
    );

    let hidden_field = find("hidden_field");
    assert_eq!(hidden_field.visibility, VisibilityKind::Private);
    assert_eq!(
        hidden_field.owner_id, public_field.owner_id,
        "both @fields belong to the same class owner"
    );

    let method = find("method");
    assert!(method.is_method, "colon method must be marked is_method");
    assert!(
        method.value_syntax.is_some(),
        "runtime method closure syntax must be preserved"
    );

    let global_field = find("global_field");
    assert_eq!(
        global_field.owner_id,
        OwnerId::Global("M".into()),
        "global runtime member owner must be canonical Global(M)"
    );

    let local_field = find("local_field");
    assert!(
        matches!(local_field.owner_id, OwnerId::Local(..)),
        "local table member owner must stay file-local: {:?}",
        local_field.owner_id
    );

    // Source order is preserved and strictly increasing.
    let mut last_order = None;
    for member in &exports.members {
        if let Some(previous) = last_order {
            assert!(
                member.order > previous,
                "member order must strictly increase: {member:?}"
            );
        }
        last_order = Some(member.order);
    }
}

#[test]
fn p1_member_contribution_preserves_overloads_and_export_key() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/overload.lua",
        r#"
        ---@class C
        ---@field f fun(a: string): string
        ---@field f fun(a: number): number
        local C = {}
        "#,
    );

    let exports = db.file_exports_of(fid);
    let overloads: Vec<_> = exports
        .members
        .iter()
        .filter(|member| member.key.name() == Some("f"))
        .collect();

    assert_eq!(overloads.len(), 2, "overloads must stay separate entries");
    assert!(
        overloads[0].order < overloads[1].order,
        "overload order must follow source order"
    );
    assert_eq!(
        overloads[0].owner_id, overloads[1].owner_id,
        "overloads share the same canonical owner"
    );
    assert_ne!(
        overloads[0].member, overloads[1].member,
        "overloads are distinct declaration identities"
    );
    assert_eq!(
        overloads[0].export_key(),
        overloads[1].export_key(),
        "overloads share the same (owner, key) export key"
    );
    assert!(matches!(
        overloads[0].export_key(),
        ExportKey::Member(OwnerId::Type(TypeScope::Global, _), _)
    ));
}

#[test]
fn p1_surface_eq_ignores_member_value_syntax_changes() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/surface.lua", "M = {}\nM.x = 1\n");
    let before = db.file_exports_of(fid).clone();

    // Same member identity, different initializer text/range.
    set_test_file(&mut db, 1, "C:/ws/surface.lua", "M = {}\nM.x = 100\n");
    let after = db.file_exports_of(fid);

    assert!(
        before.surface_eq(after),
        "initializer value changes must not change the workspace-index surface"
    );
}

#[test]
fn p1_file_cache_stores_contribution_as_arc() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/shared.lua",
        "---@class C\nlocal C = {}\nM = {}\nM.x = 1\nreturn M\n",
    );

    let cache = db.file_cache(fid).expect("file cache");
    let exports = Arc::clone(&cache.exports);
    assert!(
        Arc::strong_count(&exports) >= 2,
        "file cache must share FileExportContribution through Arc"
    );
    assert_eq!(exports.file_id, fid);
    assert!(exports.types.iter().any(|def| def.name == "C"));
    assert!(exports.globals.iter().any(|global| global.name == "M"));
    assert_eq!(exports.module_owner(), Some(OwnerId::Module(fid)));
}
