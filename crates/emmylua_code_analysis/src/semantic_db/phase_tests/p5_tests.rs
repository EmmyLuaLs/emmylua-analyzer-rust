//! P5 tests: canonical module owners and `require` alias contributions.
//!
//! The P0 cross-file module mutation test is enabled by this phase; these tests
//! additionally cover alias propagation and the canonical contribution identity.

use std::path::PathBuf;
use std::sync::Arc;

use crate::semantic_db::query;
use crate::{
    Emmyrc, FileId, LuaType, OwnerId, SemanticDatabase, SemanticModel, TypeScope, VirtualWorkspace,
    WorkspaceFolder,
};

fn integer_like(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::Number
    )
}

#[test]
fn p5_require_alias_contribution_maps_to_module_owner() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        return M
        "#,
    );
    let extra_id = ws.def_file(
        "extra.lua",
        r#"
        local M = require("mod")
        function M.extra()
            return 42
        end
        "#,
    );

    let model = ws.analysis.semantic_model(extra_id);
    let mod_file = model.module_file_of("mod").expect("mod module file");
    let exports = ws.analysis.db.file_exports_of(extra_id);

    assert_eq!(exports.aliases.len(), 1, "aliases: {:?}", exports.aliases);
    assert_eq!(exports.aliases[0].module_file, mod_file);
    assert_eq!(exports.aliases[0].owner_id(), OwnerId::Module(mod_file));

    let extra = exports
        .members
        .iter()
        .find(|member| member.key.name() == Some("extra"))
        .expect("extra member contribution");
    assert_eq!(
        extra.owner_id,
        OwnerId::Module(mod_file),
        "module mutation must attach to Module(owner), not the consumer-local alias"
    );
}

#[test]
fn p5_require_alias_chain_attaches_mutation_to_module_owner() {
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
        local N = M
        function N.extra()
            return 42
        end
        "#,
    );

    let ty = ws.expr_ty("require('mod').extra()");
    assert!(integer_like(&ty), "alias chain return type: {ty:?}");
}

#[test]
fn p5_require_alias_contribution_survives_batch_rebuild() {
    let mut ws = VirtualWorkspace::new();
    let ids = ws.def_files(vec![
        (
            "mod.lua",
            r#"
            local M = {}
            return M
            "#,
        ),
        (
            "extra.lua",
            r#"
            local M = require("mod")
            function M.extra()
                return 42
            end
            "#,
        ),
    ]);

    // Batch loading goes through `rebuild_all_caches`; module entries must be
    // available before export contributions resolve require aliases.
    let alias_file = ids
        .iter()
        .copied()
        .find(|file_id| !ws.analysis.db.file_exports_of(*file_id).aliases.is_empty())
        .expect("extra file alias contribution");
    let aliases = &ws.analysis.db.file_exports_of(alias_file).aliases;
    assert_eq!(aliases.len(), 1, "aliases: {aliases:?}");

    let model = ws.analysis.semantic_model(alias_file);
    let mod_file = model.module_file_of("mod").expect("mod file");
    assert_eq!(aliases[0].module_file, mod_file);
    let extra = ws
        .analysis
        .db
        .file_exports_of(alias_file)
        .members
        .iter()
        .find(|member| member.key.name() == Some("extra"))
        .expect("extra member");
    assert_eq!(extra.owner_id, OwnerId::Module(mod_file));

    let ty = ws.expr_ty("require('mod').extra()");
    assert!(integer_like(&ty), "batch rebuild alias chain: {ty:?}");
}

fn local_type(ws: &VirtualWorkspace, file_id: FileId, name: &str) -> LuaType {
    let model = ws.analysis.semantic_model(file_id);
    let facts = model.file_facts().expect("file facts");
    let decl = facts.decl_named(name).expect("local declaration");
    model.type_of_decl(&decl.id).expect("declaration type")
}

#[test]
fn p5b_resolve_owner_ids_is_deterministic() {
    let mut db = SemanticDatabase::new();
    db.update_config(Arc::new(Emmyrc::default()));
    let fid = FileId::new(1);
    db.set_file(
        fid,
        Some(PathBuf::from("C:/ws/c.lua")),
        r#"
        ---@class C
        local C = {}
        C.x = 1

        local M = {}
        return M
        "#
        .to_string(),
    );

    let facts = db.file_facts_of(fid).expect("facts");
    let def = facts.type_def_by_name("C").expect("class C");
    let ids = query::resolve_owner_ids(&db, &def.id);
    assert!(
        ids.contains(&OwnerId::Type(TypeScope::Global, "C".into())),
        "type owner missing: {ids:?}"
    );
    assert!(
        ids.iter().any(|id| matches!(id, OwnerId::Local(..))),
        "runtime value owner missing: {ids:?}"
    );

    let m = facts.decl_named("M").expect("local M");
    let m_ids = query::resolve_owner_ids(&db, &m.id);
    assert!(
        m_ids.contains(&OwnerId::Module(fid)),
        "module export owner missing: {m_ids:?}"
    );
}

#[test]
fn p5b_deep_alias_member_chain() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "mod.lua",
        r#"
        local M = {}
        M.sub = { value = 42 }
        return M
        "#,
    );
    let consumer = ws.def_file(
        "consumer.lua",
        r#"
        local M = require("mod")
        local x = M.sub.value
        return x
        "#,
    );

    let ty = local_type(&ws, consumer, "x");
    assert!(integer_like(&ty), "deep alias member type: {ty:?}");
}

#[test]
fn p5b_main_workspace_wins_over_library_module_name() {
    let mut db = SemanticDatabase::new();
    db.update_config(Arc::new(Emmyrc::default()));
    db.add_main_workspace(PathBuf::from("C:/ws"));
    db.add_library_workspace(&WorkspaceFolder::new(PathBuf::from("C:/libs/lib"), true));

    let main_id = FileId::new(1);
    db.set_file(
        main_id,
        Some(PathBuf::from("C:/ws/mod.lua")),
        "return {}".to_string(),
    );
    let lib_id = FileId::new(2);
    db.set_file(
        lib_id,
        Some(PathBuf::from("C:/libs/lib/mod.lua")),
        "return {}".to_string(),
    );

    let model = SemanticModel::new(&db, main_id);
    assert_eq!(
        model.module_file_of("mod"),
        Some(main_id),
        "main workspace must win over a same-named library module"
    );
}
