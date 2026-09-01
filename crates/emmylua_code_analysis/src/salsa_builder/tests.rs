use std::path::PathBuf;
use std::sync::Arc;

use rowan::TextSize;
use smol_str::SmolStr;

use super::SalsaDatabase;
use super::def::{
    ConstructorReturnMode, DeclKind, ModuleExport, SemanticId, TypeDefKind, TypeVisibility,
};
use super::exports::{export_shard, shard_of};
use super::facts::FileFacts;
use super::query::{deprecated_shard, module_shard};
use super::types::{PrimitiveType, TypeCandidate, TypeShell};
use crate::{Emmyrc, EmmyrcWorkspaceModuleMap, FileId, LuaType, LuaTypeDeclId};

fn setup() -> SalsaDatabase {
    let mut db = SalsaDatabase::new();
    db.update_config(Arc::new(Emmyrc::default()));
    db
}

fn set_test_file(db: &mut SalsaDatabase, file_id: u32, path: &str, source: &str) -> FileId {
    let fid = FileId::new(file_id);
    db.set_file(fid, Some(PathBuf::from(path)), source.to_string());
    fid
}

fn decl_local(facts: &FileFacts, name: &str) -> SemanticId {
    facts
        .decls
        .iter()
        .find(|decl| decl.name == name)
        .expect("decl not found")
        .id
        .clone()
}

fn assert_primitive(shell: &TypeShell, p: PrimitiveType) {
    assert_eq!(
        shell.candidates,
        vec![TypeCandidate::Primitive(p)],
        "expected {:?}, got {:?}",
        p,
        shell.candidates
    );
}

#[test]
fn test_file_facts_extracts_decls_and_scopes() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = 1\nlocal b = 2");

    let facts = db.q().file_facts(fid).expect("facts");
    assert_eq!(facts.decls.len(), 2);
    assert_eq!(facts.decls[0].name, "a");
    assert_eq!(facts.decls[1].name, "b");
    // chunk + block + 2×local stat
    assert_eq!(facts.scopes.len(), 4);
}

#[test]
fn test_decl_type_from_literal_initializer() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = 1");

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    assert_primitive(
        &db.q().decl_type(fid, a).expect("type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_decl_type_name_resolution_chain() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local x = 1\nlocal y = x");

    let facts = db.q().file_facts(fid).expect("facts");
    let y = decl_local(&facts, "y");
    assert_primitive(
        &db.q().decl_type(fid, y).expect("type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_decl_type_self_reference_cycle_fixpoint() {
    let mut db = setup();
    // `local a = a or 1`: a's type depends on itself → triggers salsa's native fixpoint.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = a or 1");

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    assert_primitive(
        &db.q().decl_type(fid, a).expect("type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_decl_type_forward_reference_chain() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local a = b or 1\nlocal b = a or 2",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    let b = decl_local(&facts, "b");
    assert_primitive(
        &db.q().decl_type(fid, a).expect("a type"),
        PrimitiveType::Number,
    );
    assert_primitive(
        &db.q().decl_type(fid, b).expect("b type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_decl_type_param_and_function() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "function foo(x)\nend");

    let facts = db.q().file_facts(fid).expect("facts");
    assert!(
        facts
            .decls
            .iter()
            .any(|d| d.name == "foo" && d.kind == DeclKind::Global)
    );
    assert!(
        facts
            .decls
            .iter()
            .any(|d| d.name == "x" && d.kind == DeclKind::Param)
    );
}

#[test]
fn test_resolve_type_def_public_cross_file() {
    let mut db = setup();
    // Default (no visibility) → Public → Global("Foo").
    set_test_file(&mut db, 1, "C:/ws/def.lua", "---@class Foo\nlocal Foo = {}");
    set_test_file(&mut db, 2, "C:/ws/other.lua", "local Bar = 1");

    let def = db
        .q()
        .resolve_type_def(FileId::new(2), "Foo")
        .expect("Foo resolves from other file");
    assert_eq!(def.name, "Foo");
    assert_eq!(def.full_name, "Foo");
    assert_eq!(def.file_id, FileId::new(1));
    assert_eq!(def.kind, TypeDefKind::Class);
    assert_eq!(def.visibility, TypeVisibility::Public);
    assert!(db.q().resolve_type_def(FileId::new(2), "Bar").is_none());
    assert!(db.q().resolve_type_def(FileId::new(2), "Missing").is_none());
}

#[test]
fn test_resolve_type_def_private_same_file_only() {
    let mut db = setup();
    // `---@class (private) Foo` → File(file1, "Foo"): only resolvable from file1.
    set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class (private) Foo\nlocal Foo = {}",
    );
    set_test_file(&mut db, 2, "C:/ws/b.lua", "local use = Foo");

    // Resolves from the same file; not from other files (scope isolation).
    let in_file = db
        .q()
        .resolve_type_def(FileId::new(1), "Foo")
        .expect("same file");
    assert_eq!(in_file.visibility, TypeVisibility::Private);
    assert!(db.q().resolve_type_def(FileId::new(2), "Foo").is_none());
}

#[test]
fn test_resolve_type_def_prefers_same_file_private_over_global() {
    let mut db = setup();
    // file1 has both `(private) Foo` and global `Foo`; same-file resolution prefers the private one.
    set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class (private) Foo\nlocal Foo = {}",
    );
    set_test_file(
        &mut db,
        2,
        "C:/ws/defs.lua",
        "---@class Foo\nlocal Foo = {}",
    );

    let in_a = db
        .q()
        .resolve_type_def(FileId::new(1), "Foo")
        .expect("file1 Foo");
    assert_eq!(in_a.file_id, FileId::new(1));
    assert_eq!(in_a.visibility, TypeVisibility::Private);

    // file2 has no private Foo → resolves to the global Foo (defined in file2).
    let in_b = db
        .q()
        .resolve_type_def(FileId::new(2), "Foo")
        .expect("file2 Foo");
    assert_eq!(in_b.file_id, FileId::new(2));
}

#[test]
fn test_resolve_type_def_namespace_qualified() {
    let mut db = setup();
    // `@namespace pkg` → full_name = "pkg.Foo" (Public → Global). Same-file lookup resolves through the namespace qualifier.
    set_test_file(
        &mut db,
        1,
        "C:/ws/pkg.lua",
        "---@namespace pkg\n---@class Foo\nlocal Foo = {}",
    );
    set_test_file(&mut db, 2, "C:/ws/use.lua", "local x = pkg.Foo");

    // In a namespace file, the bare name resolves through the qualified name to Global("pkg.Foo").
    let def = db
        .q()
        .resolve_type_def(FileId::new(1), "Foo")
        .expect("qualified");
    assert_eq!(def.full_name, "pkg.Foo");
    assert_eq!(def.file_id, FileId::new(1));

    // In a non-namespace file, the bare name does not resolve (pkg.Foo is not a bare-name global).
    assert!(db.q().resolve_type_def(FileId::new(2), "Foo").is_none());
}

#[test]
fn test_find_decl_by_offset_and_range() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local alpha = 1\nlocal beta = 2");

    let facts = db.q().file_facts(fid).expect("facts");
    let alpha = decl_local(&facts, "alpha");
    let range = db.q().decl_range(fid, alpha.clone()).expect("range");
    assert_eq!(
        db.q()
            .decls(fid)
            .unwrap()
            .iter()
            .find(|d| d.id == alpha)
            .expect("alpha")
            .name,
        "alpha"
    );

    // Offset covered by the name token → that decl.
    let hit = db.q().decl_by_offset(fid, range.start()).expect("hit");
    assert_eq!(hit, alpha);
    // Outside the name range (e.g. at a numeric literal) → no hit.
    let miss_offset = range.end() + TextSize::new(10);
    assert!(db.q().decl_by_offset(fid, miss_offset).is_none());
}

#[test]
fn test_syntax_tree_and_parse_errors() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = 1");

    assert!(db.q().syntax_tree(fid).is_some());
    assert!(db.q().chunk(fid).is_some());
    assert!(db.q().parse_errors(fid).is_none());

    let bad = set_test_file(&mut db, 2, "C:/ws/bad.lua", "local = =");
    assert!(db.q().parse_errors(bad).is_some());
}

#[test]
fn test_decl_type_doc_annotation() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "---@type string\nlocal a");

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    assert_primitive(
        &db.q().decl_type(fid, a).expect("type"),
        PrimitiveType::String,
    );
}

#[test]
fn test_decl_type_doc_annotation_wins_over_initializer() {
    let mut db = setup();
    // `---@type` takes precedence over the initializer.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "---@type string\nlocal a = 1");

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    assert_primitive(
        &db.q().decl_type(fid, a).expect("type"),
        PrimitiveType::String,
    );
}

#[test]
fn test_decl_type_doc_annotation_union() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@type number | string\nlocal a",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    let shell = db.q().decl_type(fid, a).expect("type");
    assert_eq!(
        shell.candidates,
        vec![
            TypeCandidate::Primitive(PrimitiveType::Number),
            TypeCandidate::Primitive(PrimitiveType::String)
        ]
    );
}

#[test]
fn test_decl_type_doc_annotation_named_type() {
    let mut db = setup();
    set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class Foo\nlocal Foo = {}\n---@type Foo\nlocal a",
    );
    let fid = FileId::new(1);

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    let shell = db.q().decl_type(fid, a).expect("type");
    assert_eq!(shell.candidates, vec![TypeCandidate::Named("Foo".into())]);
}

#[test]
fn test_decl_type_lua_projection() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@type string\nlocal a = 1\n---@class Bar\nlocal Bar = {}\n---@type Bar\nlocal b",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    let b = decl_local(&facts, "b");

    // Primitive type projection.
    let a_type = db.q().decl_type_lua(fid, a).expect("a lua type");
    assert_eq!(a_type, LuaType::String);

    // Named type → Ref (global).
    let b_type = db.q().decl_type_lua(fid, b).expect("b lua type");
    assert_eq!(b_type, LuaType::Ref(LuaTypeDeclId::global("Bar")));
}

#[test]
fn test_member_extraction_and_keys() {
    let mut db = setup();
    // Table field + assigned member + method.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local T = { foo = 1 }\nT.bar = 'x'\nfunction T:method() end",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let t = decl_local(&facts, "T");
    let keys = db.q().member_keys_of_decl(fid, t.clone());
    assert_eq!(keys, vec!["bar", "foo", "method"]);

    // Table field member owner = Decl(T).
    assert!(
        facts
            .members
            .iter()
            .any(|m| { m.owner == t && m.key.name() == Some("foo") })
    );
    // Method.
    let method = facts
        .members
        .iter()
        .find(|m| m.key.name() == Some("method"))
        .expect("method member");
    assert!(method.is_method);
}

#[test]
fn test_member_type() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local T = { foo = 1 }\nfunction T.bar() end\nT.baz = 'x'",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let find = |name: &str| {
        facts
            .members
            .iter()
            .position(|m| m.key.name() == Some(name))
            .map(|i| facts.members[i].id.clone())
            .expect("member")
    };

    assert_primitive(
        &db.q().member_type(fid, find("foo")).expect("foo"),
        PrimitiveType::Number,
    );
    assert_primitive(
        &db.q().member_type(fid, find("bar")).expect("bar"),
        PrimitiveType::Function,
    );
    assert_primitive(
        &db.q().member_type(fid, find("baz")).expect("baz"),
        PrimitiveType::String,
    );
}

#[test]
fn test_member_type_via_name_reference() {
    let mut db = setup();
    // Member value references a local → goes through decl_type.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local x = 1\nlocal T = {}\nT.a = x",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a_local = facts
        .members
        .iter()
        .find(|m| m.key.name() == Some("a"))
        .map(|m| m.id.clone())
        .expect("member a");
    assert_primitive(
        &db.q().member_type(fid, a_local).expect("a type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_member_global_root() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "M.x = 1");

    let facts = db.q().file_facts(fid).expect("facts");
    assert!(facts.members.iter().any(|m| {
        matches!(&m.owner, SemanticId::Name(n) if n.as_str() == "M") && m.key.name() == Some("x")
    }));
}

#[test]
fn test_member_chain_resolution() {
    let mut db = setup();
    // T.a = T.b; T.b = 1 → a resolves to Number through the IndexExpr member chain.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local T = {}\nT.a = T.b\nT.b = 1",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a_local = facts
        .members
        .iter()
        .find(|m| m.key.name() == Some("a"))
        .map(|m| m.id.clone())
        .expect("member a");
    assert_primitive(
        &db.q().member_type(fid, a_local).expect("a type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_member_phase2_cross_file_resolution() {
    let mut db = setup();
    // File B: global M + M.x.
    set_test_file(
        &mut db,
        2,
        "C:/ws/b.lua",
        "M = {}\nM.x = 1\nfunction M.f() end",
    );
    // File A: M.x reference (reading does not create members; only verifies phase 2 linking).
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local y = M.x");
    let _ = db.q().file_facts(fid).expect("facts");

    // Phase 2: Name("M") links to file B's global M.
    let owner = db.q().resolve_owner(SemanticId::name(SmolStr::new("M")));
    let decl_b = owner.expect("resolve M");
    assert!(
        matches!(&decl_b, SemanticId::Decl(_)),
        "Name(\"M\") 应解析为全局声明"
    );

    // Look up cross-file members by name (B's members all use Name("M") as the key).
    let members = db.q().members_of_owner(SemanticId::name(SmolStr::new("M")));
    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"x"), "文件 B 的成员 x: {:?}", names);
    assert!(names.contains(&"f"), "文件 B 的方法 f: {:?}", names);
    let x_member = members
        .iter()
        .find(|m| m.name == "x")
        .expect("member x")
        .clone();
    assert_eq!(x_member.file_id, FileId::new(2));
    assert_primitive(
        &db.q()
            .member_type(x_member.file_id, x_member.id.clone())
            .expect("x type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_member_phase2_name_chain_resolution() {
    let mut db = setup();
    // File B: M = {}; M.N = {}; M.N.z = 1.
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.N = {}\nM.N.z = 1");
    // File A: reads M.N.z.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local v = M.N.z");

    // Name("M.N") → global M → member N (file B).
    let owner = db.q().resolve_owner(SemanticId::name(SmolStr::new("M.N")));
    let n_member = owner.expect("resolve M.N");
    assert!(
        matches!(&n_member, SemanticId::Member(_)),
        "Name(\"M.N\") 应解析为成员"
    );
    let facts_b = db.q().file_facts(FileId::new(2)).expect("facts B");
    let _ = facts_b;
    // z is a member of N: members are declared with Name("M.N") as key and looked up by name key (cross-file).
    let zs = db
        .q()
        .members_of_owner(SemanticId::name(SmolStr::new("M.N")))
        .into_iter()
        .map(|m| m.name)
        .collect::<Vec<_>>();
    assert_eq!(zs, vec![SmolStr::new("z")]);

    // Semantic integrity: M.N accesses in a.lua are independent of b.lua ordering.
    let _ = db.q().syntax_tree(fid);
}

#[test]
fn test_member_cycle_converges() {
    let mut db = setup();
    // Real member cycle: T.a = T.b; T.b = T.a → salsa cycle_fn converges (Unknown, no panic).
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local T = {}\nT.a = T.b\nT.b = T.a",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a_local = facts
        .members
        .iter()
        .find(|m| m.key.name() == Some("a"))
        .map(|m| m.id.clone())
        .expect("member a");
    let b_local = facts
        .members
        .iter()
        .find(|m| m.key.name() == Some("b"))
        .map(|m| m.id.clone())
        .expect("member b");

    // Converges (Unknown), no panic.
    let _ = db.q().member_type(fid, a_local);
    let _ = db.q().member_type(fid, b_local);
}

#[test]
fn test_expr_logic_and_comparison() {
    let mut db = setup();
    // and → merge; comparison → boolean; .. → string.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local a = true and 'x'\nlocal b = 1 < 2\nlocal c = 'a' .. 'b'",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    let b = decl_local(&facts, "b");
    let c = decl_local(&facts, "c");

    let a_shell = db.q().decl_type(fid, a).expect("a");
    assert!(
        a_shell
            .candidates
            .contains(&TypeCandidate::Primitive(PrimitiveType::String))
    );
    assert!(
        a_shell
            .candidates
            .contains(&TypeCandidate::Primitive(PrimitiveType::Boolean))
    );
    assert_primitive(
        &db.q().decl_type(fid, b).expect("b"),
        PrimitiveType::Boolean,
    );
    assert_primitive(&db.q().decl_type(fid, c).expect("c"), PrimitiveType::String);
}

#[test]
fn test_phase2_cross_file_member_in_expr_type() {
    let mut db = setup();
    // File B: member x = 1 of global M.
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1");
    // File A: local y = M.x — cross-file member reference.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local y = M.x");

    let facts = db.q().file_facts(fid).expect("facts");
    let y = decl_local(&facts, "y");
    assert_primitive(
        &db.q().decl_type(fid, y).expect("y type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_phase2_invalidation_on_other_file_change() {
    let mut db = setup();
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1");
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local y = M.x");

    let facts = db.q().file_facts(fid).expect("facts");
    let y = decl_local(&facts, "y");
    // Editing B: M.x type changes → A's y type must invalidate and update (workspace-keyed).
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 's'");
    assert_primitive(
        &db.q().decl_type(fid, y).expect("y type"),
        PrimitiveType::String,
    );
}

#[test]
fn test_member_keys_of_owner_merges_field_and_runtime() {
    let mut db = setup();
    // B: class M has @field f; also global runtime M.x = 1 (key = Name("M")).
    set_test_file(
        &mut db,
        2,
        "C:/ws/b.lua",
        "---@class M\n---@field f number\nM.x = 1",
    );
    let _fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local v = M");

    // Completion scenario: the `M.` prefix at the cursor resolves to Name("M") (unresolved global name).
    let name_owner = SemanticId::name(SmolStr::new("M"));
    let keys = db.q().member_keys_of_owner(name_owner.clone());
    // Union of @field (resolve → TypeDef key) and runtime (Name key).
    assert!(keys.contains(&SmolStr::new("f")), "含 @field: {:?}", keys);
    assert!(
        keys.contains(&SmolStr::new("x")),
        "含运行时成员: {:?}",
        keys
    );

    // The concrete id (TypeDef) only has its own members (@field).
    let resolved = db.q().resolve_owner(name_owner).expect("resolve M");
    assert!(matches!(&resolved, SemanticId::TypeDef(_)), "类型优先");
    let concrete_keys = db.q().member_keys_of_owner(resolved);
    assert!(concrete_keys.contains(&SmolStr::new("f")));
    assert!(
        !concrete_keys.contains(&SmolStr::new("x")),
        "具体 id 不含 Name 键运行时成员"
    );
}

#[test]
fn test_doc_generic_param_binding() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@generic T\n---@param x T\nfunction id(x)\nend",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let sig = facts.signatures.first().expect("signature");
    let shell = db
        .q()
        .param_type(fid, sig.closure_syntax, 0)
        .expect("param type");
    assert_eq!(
        shell.candidates,
        vec![TypeCandidate::Generic(SmolStr::new("T"))],
        "T 应绑定为泛型参数"
    );
}

#[test]
fn test_doc_fun_type_structured() {
    let mut db = setup();
    // fun(a: number): string → structured function type candidate.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@type fun(a: number): string\nlocal f",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let f = decl_local(&facts, "f");
    let shell = db.q().decl_type(fid, f).expect("f type");
    let candidate = shell.candidates.first().expect("one candidate");
    let TypeCandidate::Function(fun) = candidate else {
        panic!("expected Function candidate, got {:?}", candidate);
    };
    assert_eq!(
        fun.params[0].candidates,
        vec![TypeCandidate::Primitive(PrimitiveType::Number)]
    );
    assert_eq!(
        fun.returns.candidates,
        vec![TypeCandidate::Primitive(PrimitiveType::String)]
    );
}

#[test]
fn test_doc_named_type_resolves_cross_file() {
    let mut db = setup();
    // B: class Foo.
    set_test_file(&mut db, 2, "C:/ws/b.lua", "---@class Foo\nlocal Foo = {}");
    // A: ---@type Foo.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "---@type Foo\nlocal a");

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    let shell = db.q().decl_type(fid, a).expect("a type");
    assert_eq!(
        shell.candidates,
        vec![TypeCandidate::Named(SmolStr::new("Foo"))],
        "Foo 解析到 TypeDef 全名"
    );
}

#[test]
fn test_cross_file_global_name_fallback() {
    let mut db = setup();
    // B: global M (table) and pure class C.
    set_test_file(
        &mut db,
        2,
        "C:/ws/b.lua",
        "M = {}\nM.x = 1\n---@class C\nlocal C = {}",
    );
    // A: local y = M (M is in B); local c = C (pure type name).
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local y = M\nlocal c = C");

    let facts = db.q().file_facts(fid).expect("facts");
    // Global M falls back → decl_type(its declaration) = table literal (synthesized Table type).
    let y = decl_local(&facts, "y");
    let y_shell = db.q().decl_type(fid, y).expect("y type");
    assert!(
        matches!(y_shell.candidates.as_slice(), [TypeCandidate::Table(_)]),
        "M 的类型应为合成 Table: {:?}",
        y_shell
    );
    // Pure type name C → Named("C").
    let c = decl_local(&facts, "c");
    let shell = db.q().decl_type(fid, c).expect("c type");
    assert_eq!(
        shell.candidates,
        vec![TypeCandidate::Named(SmolStr::new("C"))],
        "纯类型名回退到 Named"
    );
}

#[test]
fn test_require_module_resolution() {
    let mut db = setup();
    // B: module exports M (member x = 1).
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1\nreturn M");
    // A: require('b') → Named("M"); m.x → cross-file member.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local m = require('b')\nlocal v = m.x",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let m = decl_local(&facts, "m");
    let m_shell = db.q().decl_type(fid, m).expect("m type");
    assert!(
        m_shell.candidates.iter().any(
            |candidate| matches!(candidate, TypeCandidate::Table(table) if table.file_id == 2)
        ) && m_shell
            .candidates
            .iter()
            .any(|candidate| matches!(candidate, TypeCandidate::Named(name) if name == "M")),
        "require 返回模块导出的表 + 名字身份: {:?}",
        m_shell
    );
    // m.x → member x of the exported M (cross-file).
    let v = decl_local(&facts, "v");
    assert_primitive(
        &db.q().decl_type(fid, v).expect("v type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_require_module_subdir_suffix() {
    let mut db = setup();
    // B: subdirectory module returns number.
    set_test_file(&mut db, 2, "C:/ws/sub/mod.lua", "return 42");
    // A: require('sub.mod') suffix match → Number.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local n = require('sub.mod')");

    let facts = db.q().file_facts(fid).expect("facts");
    let n = decl_local(&facts, "n");
    assert_primitive(
        &db.q().decl_type(fid, n).expect("n type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_invalidation_granularity_local_decl_cached_across_edit() {
    let mut db = setup();
    // A: purely local decl (no cross-file reads); B: unrelated file.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local x = 1");
    let _fid_b = set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1");

    let facts = db.q().file_facts(fid).expect("facts");
    let x = decl_local(&facts, "x");
    assert_primitive(
        &db.q().decl_type(fid, x.clone()).expect("x"),
        PrimitiveType::Number,
    );

    // Record execution count; edit unrelated file B.
    let before = db.query_execution_count();
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 's'");
    assert_primitive(&db.q().decl_type(fid, x).expect("x"), PrimitiveType::Number);
    let after = db.query_execution_count();

    // A purely local decl's type does not depend on workspace → after editing an unrelated file, memo is reused and it is not re-executed.
    assert_eq!(
        before, after,
        "纯局部 decl_type 不应因无关文件编辑而重算（before={}, after={}）",
        before, after
    );
}

#[test]
fn test_invalidation_granularity_cross_file_decl_reexecutes() {
    let mut db = setup();
    // A: local y = M.x (cross-file) → should depend on workspace and re-execute after editing B.
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 1");
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local y = M.x");

    let facts = db.q().file_facts(fid).expect("facts");
    let y = decl_local(&facts, "y");
    assert_primitive(
        &db.q().decl_type(fid, y.clone()).expect("y"),
        PrimitiveType::Number,
    );

    let _before = db.query_execution_count();
    // Warm up the workspace member query first.
    let _ = db.q().members_of_owner(SemanticId::name(SmolStr::new("M")));
    let warmed = db.query_execution_count();
    set_test_file(&mut db, 2, "C:/ws/b.lua", "M = {}\nM.x = 's'");
    let _ = db.q().members_of_owner(SemanticId::name(SmolStr::new("M")));
    let after_members = db.query_execution_count();
    let _y_after = db.q().decl_type(fid, y.clone()).expect("y");
    let after_y = db.query_execution_count();
    // Cross-file decl depends on workspace (backdate fallback) → must re-execute after editing B and update the value correctly.
    assert_primitive(&db.q().decl_type(fid, y).expect("y"), PrimitiveType::String);
    assert!(
        after_members > warmed,
        "workspace 成员查询应随编辑重算（warmed={}, after={}）",
        warmed,
        after_members
    );
    assert!(
        after_y > after_members,
        "跨文件 decl_type 应随 workspace 变化重算（after_members={}, after_y={}）",
        after_members,
        after_y
    );
}

#[test]
fn test_file_exports_identity_and_shard_memo() {
    let mut db = setup();
    // The two files land in different shards (FileId % 64 differs).
    let fid1 = set_test_file(&mut db, 1, "C:/ws/a.lua", "M = {}\nM.x = 1");
    let fid2 = set_test_file(&mut db, 2, "C:/ws/b.lua", "N = {}");
    assert_ne!(shard_of(fid1), shard_of(fid2));

    let exports1 = db.q().file_exports(fid1).expect("exports1");
    assert_eq!(exports1.file_id, fid1);
    assert!(exports1.globals.iter().any(|g| g.name == "M"));
    assert!(exports1.members.iter().any(|m| m.key.to_path() == "x"));

    let workspace = db.workspace_input().expect("workspace");
    let config = db.config_input().expect("config");
    let shard1 = shard_of(fid1);
    let shard2 = shard_of(fid2);
    let _ = export_shard(&db, workspace, config, shard1);
    let _ = export_shard(&db, workspace, config, shard2);

    // Edit a shard2 file, then re-query shard1: should hit memo without running any tracked query.
    let before = db.query_execution_count();
    set_test_file(&mut db, 2, "C:/ws/b.lua", "N = {}\nN.y = 2");
    let _ = export_shard(&db, workspace, config, shard1);
    assert_eq!(
        before,
        db.query_execution_count(),
        "无关 shard 的导出查询应复用 memo"
    );

    // Query the shard containing the edited file: must re-execute and see the new member.
    let edited = export_shard(&db, workspace, config, shard2);
    assert!(edited.members.iter().any(|m| m.key.to_path() == "y"));
    assert!(
        db.query_execution_count() > before,
        "被编辑文件所在 shard 应重新执行"
    );
}

#[test]
fn test_module_and_deprecated_shard_memo() {
    let mut db = setup();
    let fid1 = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@deprecated\nOld = 1\nreturn {}",
    );
    let fid2 = set_test_file(
        &mut db,
        2,
        "C:/ws/b.lua",
        "---@deprecated\nNew = 1\nreturn {}",
    );
    assert_ne!(shard_of(fid1), shard_of(fid2));

    let workspace = db.workspace_input().expect("workspace");
    let config = db.config_input().expect("config");
    let shard1 = shard_of(fid1);
    let shard2 = shard_of(fid2);
    let _ = deprecated_shard(&db, workspace, config, shard1);
    let _ = deprecated_shard(&db, workspace, config, shard2);
    let _ = module_shard(&db, workspace, config, shard1);
    let _ = module_shard(&db, workspace, config, shard2);

    let before = db.query_execution_count();
    set_test_file(&mut db, 2, "C:/ws/b.lua", "return {}");
    let _ = deprecated_shard(&db, workspace, config, shard1);
    let _ = module_shard(&db, workspace, config, shard1);
    assert_eq!(
        before,
        db.query_execution_count(),
        "无关 shard 的 deprecated/module 查询应复用 memo"
    );

    let _ = deprecated_shard(&db, workspace, config, shard2);
    assert!(
        db.query_execution_count() > before,
        "被编辑文件所在 shard 应重新执行"
    );
}

#[test]
fn test_semantic_model_file_exports_and_signature_api() {
    let mut db = setup();
    let fid_impl = set_test_file(
        &mut db,
        1,
        "C:/ws/impl.lua",
        "---@param x integer\n---@return string\nfunction f(x) end\nM = {}\nM.v = 1",
    );
    let fid_use = set_test_file(&mut db, 2, "C:/ws/use.lua", "local n = 1");

    let model = crate::SalsaSemanticModel::new(&db, fid_use).expect("model");
    let exports = model.file_exports(fid_impl).expect("exports");
    assert!(exports.globals.iter().any(|g| g.name == "f"));
    assert!(exports.members.iter().any(|m| m.key.to_path() == "v"));
    assert_eq!(
        model.file_exports_current().map(|e| e.file_id),
        Some(fid_use)
    );

    let facts = db.q().file_facts(fid_impl).expect("facts");
    let f_decl = facts
        .decls
        .iter()
        .find(|d| d.name == "f")
        .expect("f decl")
        .id
        .clone();
    let signature = model.type_of_decl_signature(&f_decl).expect("signature");
    assert_eq!(signature.get_params().len(), 1);
    assert!(matches!(signature.get_ret(), LuaType::String));
}

#[test]
fn test_require_module_init_lua() {
    let mut db = setup();
    // init.lua → module name uses the parent directory.
    set_test_file(&mut db, 2, "C:/ws/pkg/init.lua", "return 7");
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local n = require('pkg')");

    let facts = db.q().file_facts(fid).expect("facts");
    let n = decl_local(&facts, "n");
    assert_primitive(
        &db.q().decl_type(fid, n).expect("n type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_require_module_map_rewrite() {
    let mut db = setup();
    // module_map: `@/` → `src/`.
    let mut emmyrc = Emmyrc::default();
    emmyrc.workspace.module_map = vec![EmmyrcWorkspaceModuleMap {
        pattern: "@/".to_string(),
        replace: "src/".to_string(),
    }];
    db.update_config(Arc::new(emmyrc));
    // File is under src/, require("@/mod") → "src/mod" → "src.mod" → match.
    set_test_file(&mut db, 2, "C:/ws/src/mod.lua", "return 9");
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local n = require('@/mod')");

    let facts = db.q().file_facts(fid).expect("facts");
    let n = decl_local(&facts, "n");
    assert_primitive(
        &db.q().decl_type(fid, n).expect("n type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_require_fuzzy_suffix_prefers_exact() {
    let mut db = setup();
    // The subdirectory b.lua module name is "sub.b": require("b") fuzzy-matches it by suffix.
    set_test_file(&mut db, 2, "C:/ws/sub/b.lua", "return 1");
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local n = require('b')\nlocal m = require('sub.b')",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    // require("b") → fuzzy → sub.b.
    let n = decl_local(&facts, "n");
    assert_primitive(&db.q().decl_type(fid, n).expect("n"), PrimitiveType::Number);
    // require("sub.b") → exact match.
    let m = decl_local(&facts, "m");
    assert_primitive(
        &db.q().decl_type(fid, m).expect("m type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_dual_identity_type_and_runtime_members() {
    let mut db = setup();
    // B: @class M + local M = {} (runtime value) + @field f + runtime M.x = 1.
    set_test_file(
        &mut db,
        2,
        "C:/ws/b.lua",
        "---@class M\n---@field f number\nlocal M = {}\nM.x = 1",
    );
    let _fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local v = M");

    // Name("M"): union of type (@field f) and runtime value (local M's member x).
    let name_owner = SemanticId::name(SmolStr::new("M"));
    let keys = db.q().member_keys_of_owner(name_owner.clone());
    assert!(keys.contains(&SmolStr::new("f")), "含 @field: {:?}", keys);
    assert!(
        keys.contains(&SmolStr::new("x")),
        "含运行时成员: {:?}",
        keys
    );

    // The concrete TypeDef id is also linked to the runtime value through dual identity.
    let type_def = db.q().resolve_owner(name_owner).expect("resolve M");
    assert!(matches!(&type_def, SemanticId::TypeDef(_)));
    let type_keys = db.q().member_keys_of_owner(type_def.clone());
    assert!(
        type_keys.contains(&SmolStr::new("f")),
        "含 @field: {:?}",
        type_keys
    );
    assert!(
        type_keys.contains(&SmolStr::new("x")),
        "TypeDef 经双重身份关联运行时成员: {:?}",
        type_keys
    );

    // Member type: `M.x` in A reads B's runtime x = 1 → Number.
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local y = M.x");
    let facts = db.q().file_facts(fid).expect("facts");
    let y = decl_local(&facts, "y");
    assert_primitive(
        &db.q().decl_type(fid, y).expect("y type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_anonymous_table_module_member() {
    let mut db = setup();
    // B: module returns anonymous table `{ x = 1 }`.
    set_test_file(&mut db, 2, "C:/ws/b.lua", "return { x = 1 }");
    // A: require('b').x → member of the anonymous table's synthesized owner.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local m = require('b')\nlocal v = m.x",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let m = decl_local(&facts, "m");
    let m_shell = db.q().decl_type(fid, m).expect("m type");
    assert!(
        matches!(m_shell.candidates.as_slice(), [TypeCandidate::Table(_)]),
        "模块导出匿名表 → Table(合成): {:?}",
        m_shell
    );
    let v = decl_local(&facts, "v");
    assert_primitive(
        &db.q().decl_type(fid, v).expect("v type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_anonymous_table_function_return_member() {
    let mut db = setup();
    // Function returns anonymous table `{ a = 's' }`.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local f = function() return { a = 's' } end\nlocal r = f()\nlocal s = r.a",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let r = decl_local(&facts, "r");
    assert!(
        matches!(
            db.q()
                .decl_type(fid, r)
                .expect("r type")
                .candidates
                .as_slice(),
            [TypeCandidate::Table(_)]
        ),
        "函数返回匿名表 → Table(合成)"
    );
    let s = decl_local(&facts, "s");
    assert_primitive(
        &db.q().decl_type(fid, s).expect("s type"),
        PrimitiveType::String,
    );
}

#[test]
fn test_generic_instantiation_member_substitution() {
    let mut db = setup();
    // Same file: Box<T> has value: T; Box<number> → value: number.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class Box<T>\n\
         ---@field value T\n\
         local Box = {}\n\
         ---@type Box<number>\n\
         local b\n\
         local v = b.value",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let v = decl_local(&facts, "v");
    assert_primitive(
        &db.q().decl_type(fid, v).expect("v type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_generic_instantiation_cross_file_and_function() {
    let mut db = setup();
    // B: Box<T>'s get(): T and value: T.
    set_test_file(
        &mut db,
        2,
        "C:/ws/b.lua",
        "---@class Box<T>\n---@field value T\n---@field get fun(): T\nlocal Box = {}",
    );
    // A: Box<string> → value: string; get() also returns string.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@type Box<string>\nlocal b\nlocal v = b.value\nlocal r = b.get()",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let v = decl_local(&facts, "v");
    assert_primitive(
        &db.q().decl_type(fid, v).expect("v type"),
        PrimitiveType::String,
    );
    let r = decl_local(&facts, "r");
    assert_primitive(
        &db.q().decl_type(fid, r).expect("r type"),
        PrimitiveType::String,
    );
}

#[test]
fn test_flow_tree_builds_cfg() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local x = 1\n\
         if type(x) == 'string' then\n\
           x = 2\n\
         end\n\
         print(x)",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let tree = db.q().flow_tree(fid).expect("flow tree");
    // The print name in call statement print(x) should bind to a flow node.
    let print_use = facts
        .name_uses
        .iter()
        .find(|u| u.name == "print")
        .expect("print use");
    let flow_id = tree.get_flow_id(print_use.syntax).expect("print 绑定 flow");
    let node = tree.get_flow_node(flow_id).expect("flow node");
    assert!(
        node.antecedent.is_some() || matches!(node.kind, super::flow::FlowNodeKind::Start),
        "flow 节点应有前驱或为 Start"
    );

    // The assignment inside the conditional if (x = 2) should also have a flow binding (via name_use syntax).
    let has_condition_node = (0..tree.node_count()).any(|i| {
        tree.get_flow_node(super::flow::FlowId(i))
            .is_some_and(|n| n.kind.is_conditional())
    });
    assert!(has_condition_node, "if 条件应产生 True/False 条件节点");
}

#[test]
fn test_lua_compilation_salsa_sync_and_check() {
    use lsp_types::Uri;
    use std::str::FromStr;

    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri = Uri::from_str("file:///C:/ws/check_test.lua").unwrap();
    let fid = db.set_file_content(
        &uri,
        Some(
            "local x = 1\n\
             local y = missing\n\
             print(x)\n\
             global_defined = 1\n\
             local z = global_defined\n\
             local w = print"
                .to_string(),
        ),
    );
    db.update_main_root(PathBuf::from("C:/ws"));

    // The salsa side can retrieve facts and types.
    let model = crate::semantic_model::SemanticModel::new(&db, fid).expect("salsa semantic model");
    let x = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "x")
        .expect("x decl");
    assert!(matches!(
        model.type_of_decl(&x.id),
        Some(LuaType::IntegerConst(1))
    ));

    // check: only missing should be reported (local x, builtin print, and global global_defined are not undefined).
    let config = Arc::new(crate::check::CheckConfig::new(&emmyrc));
    let diagnostics = crate::check::check_file(&model, config);
    let undefined: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == crate::DiagnosticCode::UndefinedGlobal)
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(undefined.len(), 1, "只报 missing: {:?}", undefined);
    assert!(
        undefined[0].contains("missing"),
        "命中 missing: {:?}",
        undefined
    );

    // Filtering: after disabling related diagnostic codes in config, no diagnostics are produced.
    let mut filtered_emmyrc = Emmyrc::default();
    filtered_emmyrc
        .diagnostics
        .disable
        .push(crate::DiagnosticCode::UndefinedGlobal);
    filtered_emmyrc
        .diagnostics
        .disable
        .push(crate::DiagnosticCode::Unused);
    let filtered_config = Arc::new(crate::check::CheckConfig::new(&filtered_emmyrc));
    let filtered = crate::check::check_file(&model, filtered_config);
    assert!(filtered.is_empty(), "禁用相关码后应无诊断: {:?}", filtered);
}

#[test]
fn test_syntax_error_checks() {
    use lsp_types::Uri;
    use std::str::FromStr;

    // (source, whether it should produce SyntaxError)
    let cases: &[(&str, bool)] = &[
        ("local = =", true),                       // error reported by parser
        ("local s = '\\u{110000}'", true),         // out-of-range unicode escape (self-check)
        ("function f() return ... end", true),     // `...` in a non-vararg function (self-check)
        ("goto missing", true),                    // goto to an undefined label (self-check)
        ("function f(...) return ... end", false), // vararg function is legal
        ("goto ok\n::ok::", false),                // goto to a declared label is legal
    ];

    for (source, expect_error) in cases {
        let emmyrc = Arc::new(Emmyrc::default());
        let mut db = SalsaDatabase::new();
        db.update_config(emmyrc.clone());
        let uri = Uri::from_str("file:///C:/ws/syntax.lua").unwrap();
        let fid = db.set_file_content(&uri, Some(source.to_string()));
        db.update_main_root(PathBuf::from("C:/ws"));
        let model =
            crate::semantic_model::SemanticModel::new(&db, fid).expect("salsa semantic model");
        let config = Arc::new(crate::check::CheckConfig::new(&emmyrc));
        let diagnostics = crate::check::check_file(&model, config);
        let has_syntax_error = diagnostics
            .iter()
            .any(|d| d.code == crate::DiagnosticCode::SyntaxError);
        assert_eq!(
            has_syntax_error,
            *expect_error,
            "source {:?}: expect SyntaxError={}, got {:?}",
            source,
            expect_error,
            diagnostics
                .iter()
                .map(|d| (d.code.get_name(), d.message.as_str()))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_salsa_snapshot_query_from_other_thread() {
    use lsp_types::Uri;
    use std::str::FromStr;

    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri = Uri::from_str("file:///C:/ws/thread_test.lua").unwrap();
    let fid = db.set_file_content(&uri, Some("local x = 1\nlocal y = x + 1".to_string()));
    db.update_main_root(PathBuf::from("C:/ws"));

    // Snapshot clone (shares memo table) → move into worker thread for querying.
    let snapshot = db.clone();
    let handle = std::thread::spawn(move || {
        let model =
            crate::semantic_model::SemanticModel::new(&snapshot, fid).expect("semantic model");
        let x = model
            .decls()
            .expect("decls")
            .iter()
            .find(|d| d.name == "x")
            .expect("x decl");
        model.type_of_decl(&x.id).expect("x type")
    });
    let ty = handle.join().expect("thread panicked");
    assert_eq!(ty, LuaType::IntegerConst(1));

    // Write operations stay on the main thread; the snapshot is unaffected.
    let uri2 = Uri::from_str("file:///C:/ws/thread_test2.lua").unwrap();
    db.set_file_content(&uri2, Some("local z = 2".to_string()));
    assert!(db.file_ids().contains(&fid), "写操作后 salsa 仍持有原文件");
}

#[test]
fn test_lua_syntax_id_disambiguates_nested_same_start() {
    let mut db = setup();
    // Value expression is the outer `(x + 1) * 2`: the inner `(x + 1)` shares the starting `(` with the outer.
    // Position alone is not unique; LuaSyntaxId (kind+range) is required to locate the outer expression precisely.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local x = 1\nlocal a = (x + 1) * 2",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    assert_primitive(
        &db.q().decl_type(fid, a).expect("type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_name_uses_and_decl_references() {
    let mut db = setup();
    // Declarations are not NameExpr (naturally excluded); a appears 3 times (write target + value + argument).
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local a = 1\na = a + 1\nprint(a)",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let a = decl_local(&facts, "a");
    let a_uses = facts.name_uses.iter().filter(|u| u.name == "a").count();
    assert_eq!(a_uses, 3);

    let refs = db.q().decl_references(fid, a);
    assert_eq!(refs.len(), 3);
}

#[test]
fn test_resolve_name_from_use() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local x = 1\nlocal y = x + 1");

    let facts = db.q().file_facts(fid).expect("facts");
    let x = decl_local(&facts, "x");

    // Find the use of `x` in `y = x + 1` and resolve it back to x's declaration.
    let x_use = facts
        .name_uses
        .iter()
        .find(|u| u.name == "x")
        .expect("x use");
    let resolved = db
        .q()
        .resolve_name(fid, x_use.syntax.get_range().start())
        .expect("resolved");
    assert_eq!(resolved, x);

    // The declaration position (local x) is not a NameExpr, so it does not resolve.
    assert!(
        db.q()
            .resolve_name(
                fid,
                facts
                    .decls
                    .iter()
                    .find(|d| d.id == x)
                    .unwrap()
                    .name_range
                    .start()
            )
            .is_none()
    );
}

#[test]
fn test_decl_references_respects_shadowing() {
    let mut db = setup();
    // Inner x shadows outer x: outer x's references are only the two visible in the outer scope (`x = x`); inner has only `return x`.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local x = 1\nlocal function f()\n  local x = 2\n  return x\nend\nx = x",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let x_decls = facts
        .decls
        .iter()
        .filter(|d| d.name == "x")
        .collect::<Vec<_>>();
    assert_eq!(x_decls.len(), 2, "expected outer + inner x");
    let outer = x_decls[0].id.clone(); // declaration order: outer first
    let inner = x_decls[1].id.clone();

    // Outer x references: the two x's in `x = x` (write + value).
    let outer_refs = db.q().decl_references(fid, outer);
    assert_eq!(outer_refs.len(), 2);
    // Inner x reference: the x in `return x`.
    let inner_refs = db.q().decl_references(fid, inner);
    assert_eq!(inner_refs.len(), 1);
}

#[test]
fn test_resolve_name_same_scope_shadowing_prefers_latest() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local x = 123
local x = 456
print(x)",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let x_decls = facts
        .decls
        .iter()
        .filter(|d| d.name == "x")
        .collect::<Vec<_>>();
    assert_eq!(x_decls.len(), 2);

    let first = x_decls[0].id.clone();
    let second = x_decls[1].id.clone();

    // The x in `print(x)` should resolve to the second `local x = 456`.
    let x_use = facts
        .name_uses
        .iter()
        .find(|u| u.name == "x" && u.syntax.get_range().start() > x_decls[1].name_range.start())
        .expect("x use after second decl");
    let resolved = db
        .q()
        .resolve_name(fid, x_use.syntax.get_range().start())
        .expect("resolved");
    assert_eq!(resolved, second);
    assert_ne!(resolved, first);
}

#[test]
fn test_call_returns_function_return_type() {
    let mut db = setup();
    // `f()`'s return type comes from f's body return.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "local f = function() return 1 end\nlocal x = f()",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let x = decl_local(&facts, "x");
    assert_primitive(
        &db.q().decl_type(fid, x).expect("x type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_signature_doc_return() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@return string\nfunction foo() end",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let sig = facts.signatures.first().expect("signature");
    let shell = db
        .q()
        .signature_return(fid, sig.closure_syntax)
        .expect("ret");
    assert_primitive(&shell, PrimitiveType::String);
}

#[test]
fn test_signature_mutual_recursion_converges() {
    let mut db = setup();
    // foo → bar → foo: salsa cycle_fn converges, no panic.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "function foo() return bar() end\nfunction bar() return foo() end",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let foo_sig = facts
        .signatures
        .iter()
        .find(|s| s.name.as_deref() == Some("foo"))
        .expect("foo sig");
    let bar_sig = facts
        .signatures
        .iter()
        .find(|s| s.name.as_deref() == Some("bar"))
        .expect("bar sig");
    let _ = db.q().signature_return(fid, foo_sig.closure_syntax);
    let _ = db.q().signature_return(fid, bar_sig.closure_syntax);
}

#[test]
fn test_func_params_not_duplicated() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "function f(x)\nend");

    let facts = db.q().file_facts(fid).expect("facts");
    let param_count = facts
        .decls
        .iter()
        .filter(|d| d.kind == DeclKind::Param)
        .count();
    assert_eq!(param_count, 1, "param 不应被重复收集");
}

#[test]
fn test_signature_doc_param_type() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@param x string\nfunction f(x)\nend",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let sig = facts.signatures.first().expect("signature");
    assert_eq!(sig.param_names, vec!["x"]);
    let docs = sig.docs.as_ref().expect("doc");
    assert_eq!(docs.param_types.len(), 1);
    assert_eq!(docs.param_types[0].0, "x", "x 应有 ---@param 类型");
}

#[test]
fn test_named_vararg_signature_is_variadic() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "function f(...args) end");
    let facts = db.q().file_facts(fid).expect("facts");
    let sig = facts.signatures.first().expect("signature");
    assert!(sig.is_variadic, "named vararg `...args` 应标记为可变参数");
    assert_eq!(sig.param_names, vec!["args"]);

    let fid2 = set_test_file(&mut db, 2, "C:/ws/b.lua", "function g(...) end");
    let facts2 = db.q().file_facts(fid2).expect("facts");
    let sig2 = facts2.signatures.first().expect("signature");
    assert!(sig2.is_variadic, "`...` 仍应标记为可变参数");
    assert_eq!(sig2.param_names, vec!["..."]);
}

#[test]
fn test_class_field_members() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class Foo\n---@field bar string\n---@field count number\nlocal Foo = {}",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    // @field member owner = Type(TypeDefLocal), pointing to the Foo definition in this file.
    let foo_id = facts.type_def_by_name("Foo").expect("Foo def").id.clone();
    assert!(
        facts
            .members
            .iter()
            .any(|m| { m.owner == foo_id && m.key.name() == Some("bar") })
    );
    assert_eq!(
        db.q().member_keys_of_type(fid, foo_id.clone()),
        vec!["bar", "count"]
    );
    assert!(
        db.q()
            .member_keys_of_decl(fid, decl_local(&facts, "Foo"))
            .is_empty()
    );
}

#[test]
fn test_type_def_generic_params() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class Box<T: Base, U = string>\nlocal Box = {}\n---@alias Pair<T, U>\nlocal Pair = 1",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let box_def = facts.type_def_by_name("Box").expect("Box def");
    assert_eq!(box_def.generic_params.len(), 2, "Box 应有 2 个泛型");
    assert_eq!(box_def.generic_params[0].name, "T");
    assert!(box_def.generic_params[0].constraint.is_some(), "T 有约束");
    assert!(box_def.generic_params[0].default.is_none());
    assert_eq!(box_def.generic_params[1].name, "U");
    assert!(box_def.generic_params[1].default.is_some(), "U 有默认类型");

    let pair_def = facts.type_def_by_name("Pair").expect("Pair def");
    assert_eq!(pair_def.generic_params.len(), 2, "alias 应有 2 个泛型");
    assert!(pair_def.generic_params[0].constraint.is_none());
}

#[test]
fn test_doc_deprecated_on_class_and_field() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@deprecated\n\
         ---@class OldThing\n\
         ---@deprecated\n\
         ---@field gone number\n\
         local OldThing = {}",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let old = facts.type_def_by_name("OldThing").expect("OldThing");
    assert!(old.deprecated, "@class 同块 @deprecated");
    assert!(
        facts
            .members
            .iter()
            .any(|m| m.key.name() == Some("gone") && m.deprecated),
        "@field 同块 @deprecated"
    );
}

#[test]
fn test_signature_doc_generic_and_flags() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@generic T\n\
         ---@deprecated\n\
         ---@async\n\
         ---@overload fun(x: string): string\n\
         ---@param x T\n\
         ---@return T\n\
         function id(x)\nend",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let sig = facts.signatures.first().expect("signature");
    let docs = sig.docs.as_ref().expect("doc");
    assert_eq!(docs.generic_params.len(), 1, "函数级 @generic");
    assert_eq!(docs.generic_params[0].name, "T");
    assert!(docs.deprecated, "@deprecated");
    assert!(docs.is_async, "@async");
    assert_eq!(docs.overloads.len(), 1, "@overload 类型节点");
    assert!(
        docs.return_overloads.is_empty(),
        "匿名 @return 不算 overload"
    );
    assert_eq!(docs.returns.len(), 1);
}

#[test]
fn test_signature_doc_return_overload() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@return number count\n---@return string name\nfunction f()\nend",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let sig = facts.signatures.first().expect("signature");
    let docs = sig.docs.as_ref().expect("doc");
    assert_eq!(
        docs.return_overloads.len(),
        0,
        "具名 @return 现在进入主返回行"
    );
    assert_eq!(docs.returns.len(), 2, "具名 @return 保留在 returns");
    assert_eq!(docs.named_returns.len(), 2, "具名信息保留");
    assert_eq!(docs.named_returns[0].0, "count");
    assert_eq!(docs.named_returns[1].0, "name");
    assert!(
        docs.return_overload_rows.is_empty(),
        "没有 @return_overload"
    );
}

#[test]
fn test_signature_doc_return_overload_unnamed() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@return_overload false, string\nfunction f()\nend",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let sig = facts.signatures.first().expect("signature");
    let docs = sig.docs.as_ref().expect("doc");
    assert_eq!(
        docs.return_overloads.len(),
        2,
        "@return_overload 是 overload 行"
    );
    assert_eq!(docs.return_overloads[0].0, None, "无名字");
    assert!(docs.returns.is_empty());
}

#[test]
fn test_class_field_inheritance() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class Foo\n---@field bar string\nlocal Foo = {}\n---@class Bar : Foo\nlocal Bar = {}",
    );

    // Bar inherits Foo's members.
    let facts = db.q().file_facts(fid).expect("facts");
    let bar_id = facts.type_def_by_name("Bar").expect("Bar def").id.clone();
    assert_eq!(db.q().member_keys_of_type(fid, bar_id), vec!["bar"]);
}

#[test]
fn test_member_access_resolves_class_field() {
    let mut db = setup();
    // x's doc type is Foo → `x.bar` resolves through type members.
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@class Foo\n---@field bar string\nlocal Foo = {}\n---@type Foo\nlocal x = {}\nlocal y = x.bar",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let y = decl_local(&facts, "y");
    assert_primitive(
        &db.q().decl_type(fid, y).expect("y type"),
        PrimitiveType::String,
    );
}

#[test]
fn test_module_export_decl() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/mod.lua",
        "local M = {}\nM.x = 1\nreturn M",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let m = decl_local(&facts, "M");
    match db.q().module_export(fid).expect("export") {
        ModuleExport::Decl { decl, name } => {
            assert_eq!(decl, &m);
            assert_eq!(name, "M");
        }
        other => panic!("expected Decl, got {:?}", other),
    }
    // Module export type = declaration identity table + name identity (both owner paths reachable).
    let shell = db.q().module_export_type(fid).expect("export type");
    assert!(
        shell.candidates.iter().any(
            |candidate| matches!(candidate, TypeCandidate::Table(table) if table.file_id == 1)
        ) && shell
            .candidates
            .iter()
            .any(|candidate| matches!(candidate, TypeCandidate::Named(name) if name == "M")),
        "expected exported table + name identity, got {:?}",
        shell
    );
}

#[test]
fn test_module_export_expr_and_none() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/mod.lua", "return { a = 1 }");
    assert!(matches!(
        db.q().module_export(fid).expect("export"),
        ModuleExport::Expr { .. }
    ));
    let shell = db.q().module_export_type(fid).expect("export type");
    assert!(
        matches!(shell.candidates.as_slice(), [TypeCandidate::Table(_)]),
        "匿名表导出 → Table(合成 owner): {:?}",
        shell
    );

    let no_ret = set_test_file(&mut db, 2, "C:/ws/no.lua", "local x = 1");
    assert!(matches!(
        db.q().module_export(no_ret).expect("export"),
        ModuleExport::None
    ));
}

#[test]
fn test_global_assignment_extracts_decl() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "foo = 1");

    let facts = db.q().file_facts(fid).expect("facts");
    let foo = decl_local(&facts, "foo");
    assert_primitive(
        &db.q().decl_type(fid, foo).expect("type"),
        PrimitiveType::Number,
    );
}

#[test]
fn test_constructor_attribute_collected_on_following_param() {
    let mut db = setup();
    let fid = set_test_file(
        &mut db,
        1,
        "C:/ws/a.lua",
        "---@generic T\n---@[constructor(\"__init\", \"Base\", false, \"doc\")]\n---@param class `T`\n---@return T\nfunction meta(class)\nend",
    );

    let facts = db.q().file_facts(fid).expect("facts");
    let meta = facts
        .decls
        .iter()
        .find(|decl| decl.name == "meta")
        .expect("meta decl");
    let signature = facts
        .signature_by_closure(meta.value_expr_syntax.expect("closure"))
        .expect("signature");
    let docs = signature.docs.as_ref().expect("docs");
    assert_eq!(docs.constructor_params.len(), 1);
    let (param, attribute) = &docs.constructor_params[0];
    assert_eq!(param, "class");
    assert_eq!(attribute.name, "__init");
    assert_eq!(attribute.root_class.as_deref(), Some("Base"));
    assert!(!attribute.strip_self);
    assert_eq!(attribute.return_mode, ConstructorReturnMode::Doc);
}

#[test]
fn test_remote_uri_stable_mapping() {
    use lsp_types::Uri;
    use std::str::FromStr;

    let mut db = setup();
    let uri = Uri::from_str("untitled:Untitled-1").unwrap();
    let fid1 = db.set_file_content(&uri, Some("local a = 1".to_string()));
    let fid2 = db.set_file_content(&uri, Some("local b = 1".to_string()));
    assert_eq!(fid1, fid2);
    assert_eq!(db.lookup_file_id(&uri), Some(fid1));
    assert_eq!(db.file_uri(fid1).as_ref(), Some(&uri));
    assert_eq!(db.document(fid1).unwrap().get_uri().as_ref(), Some(&uri));

    db.set_file_content(&uri, None);
    assert_eq!(db.lookup_file_id(&uri), None);
}

/// Micro-benchmark: database clone cost vs VFS read path / salsa query cost.
/// `cargo test -p emmylua_code_analysis bench_clone_vs_query -- --ignored --nocapture`
#[test]
#[ignore = "micro benchmark: clone vs salsa query"]
fn bench_clone_vs_query() {
    use std::mem::size_of;
    use std::time::{Duration, Instant};

    let mut db = setup();

    let empty_iters = 200_000;
    let now = Instant::now();
    for _ in 0..empty_iters {
        let _ = db.clone();
    }
    let empty_clone_time = now.elapsed();

    let file_count = 500;
    for i in 0..file_count {
        set_test_file(
            &mut db,
            i + 1,
            &format!("C:/ws/file_{i}.lua"),
            "local a = 1\nlocal b = a + 1\nreturn b",
        );
    }
    let target = FileId::new(file_count);
    let target_uri = db.file_uri(target).expect("target uri");

    // warmup
    for _ in 0..10_000 {
        let _ = db.clone();
    }
    let _ = db.q().file_facts(target);
    let _ = db.document(target);
    let _ = db.line_index(target);
    let _ = db.lookup_file_id(&target_uri);

    let clone_iters = 200_000;
    let query_iters = 20_000;
    let read_iters = 200_000;

    let now = Instant::now();
    for _ in 0..clone_iters {
        let _ = db.clone();
    }
    let clone_time = now.elapsed();

    let now = Instant::now();
    for _ in 0..query_iters {
        let _ = db.q().file_facts(target);
    }
    let facts_time = now.elapsed();

    let now = Instant::now();
    for _ in 0..read_iters {
        let _ = db.lookup_file_id(&target_uri);
    }
    let lookup_time = now.elapsed();

    let now = Instant::now();
    for _ in 0..read_iters {
        let _ = db.file_path(target);
    }
    let file_path_time = now.elapsed();

    let now = Instant::now();
    for _ in 0..read_iters {
        let _ = db.get_file_text(target);
    }
    let file_text_time = now.elapsed();

    let now = Instant::now();
    for _ in 0..query_iters {
        let _ = db.line_index(target);
    }
    let line_index_time = now.elapsed();

    let now = Instant::now();
    for _ in 0..query_iters {
        let _ = db.document(target);
    }
    let document_time = now.elapsed();

    fn ns_per(op: Duration, iters: u32) -> f64 {
        op.as_nanos() as f64 / iters as f64
    }

    eprintln!("== clone vs query micro-benchmark ==");
    eprintln!(
        "SalsaDatabase size={}B  empty clone={:.1} ns/op",
        size_of::<SalsaDatabase>(),
        ns_per(empty_clone_time, empty_iters)
    );
    eprintln!(
        "clone({} files)={:.1} ns/op",
        file_count,
        ns_per(clone_time, clone_iters)
    );
    eprintln!(
        "lookup_file_id={:.1} ns/op  file_path={:.1} ns/op  get_file_text={:.1} ns/op",
        ns_per(lookup_time, read_iters),
        ns_per(file_path_time, read_iters),
        ns_per(file_text_time, read_iters)
    );
    eprintln!(
        "file_facts(cached)={:.1} ns/op  line_index(cached)={:.1} ns/op  document(cached)={:.1} ns/op",
        ns_per(facts_time, query_iters),
        ns_per(line_index_time, query_iters),
        ns_per(document_time, query_iters)
    );
}

#[test]
fn test_vfs_snapshot_shared_between_clones_and_stable_across_metadata_change() {
    let mut db = setup();
    let fid = set_test_file(&mut db, 1, "C:/ws/a.lua", "local x = 1");

    // Clones share the same VFS snapshot
    let snapshot = db.clone();
    assert!(Arc::ptr_eq(db.vfs(), snapshot.vfs()));
    drop(snapshot);

    // Text-only update: don't replace the VFS snapshot, avoiding an O(n) VFS clone per keystroke
    let old_vfs = db.vfs().clone();
    set_test_file(&mut db, 1, "C:/ws/a.lua", "local x = 2");
    assert!(
        Arc::ptr_eq(&old_vfs, db.vfs()),
        "text edit should reuse the same VFS snapshot"
    );

    // Path/URI update: publish a new VFS snapshot; the old snapshot keeps the old mount info
    set_test_file(&mut db, 1, "C:/ws/b.lua", "local x = 2");
    assert!(!Arc::ptr_eq(&old_vfs, db.vfs()));
    assert_eq!(
        old_vfs.file_entry(fid).unwrap().path.as_deref(),
        Some(std::path::Path::new("C:/ws/a.lua"))
    );
    assert_eq!(
        db.vfs().file_entry(fid).unwrap().path.as_deref(),
        Some(std::path::Path::new("C:/ws/b.lua"))
    );
}

#[test]
fn test_multi_workspace_roots_and_module_index() {
    let mut db = setup();
    let std_root = PathBuf::from("C:/std");
    let main_root = PathBuf::from("C:/ws");
    let lib1_root = PathBuf::from("C:/libs/lib1");
    let lib2_root = PathBuf::from("C:/libs/lib2");

    db.add_std_workspace(std_root.clone());
    db.add_main_workspace(main_root.clone());
    db.add_library_workspace(&crate::WorkspaceFolder::new(lib1_root.clone(), true));
    db.add_library_workspace(&crate::WorkspaceFolder::new(lib2_root.clone(), true));

    let std_fid = set_test_file(&mut db, 1, "C:/std/string.lua", "return {}");
    let main_fid = set_test_file(&mut db, 2, "C:/ws/main.lua", "return {}");
    let lib1_fid = set_test_file(&mut db, 3, "C:/libs/lib1/mod.lua", "return {}");
    let lib2_fid = set_test_file(&mut db, 4, "C:/libs/lib2/other.lua", "return {}");

    assert_eq!(db.workspace_id_of(std_fid), Some(crate::WorkspaceId::STD));
    assert_eq!(db.workspace_id_of(main_fid), Some(crate::WorkspaceId::MAIN));
    assert_eq!(
        db.workspace_id_of(lib1_fid),
        Some(crate::WorkspaceId { id: 3 })
    );
    assert_eq!(
        db.workspace_id_of(lib2_fid),
        Some(crate::WorkspaceId { id: 4 })
    );

    assert_eq!(db.module_name_of(main_fid).as_deref(), Some("main"));
    assert_eq!(db.module_name_of(lib1_fid).as_deref(), Some("mod"));
    assert_eq!(db.module_name_of(lib2_fid).as_deref(), Some("other"));

    assert!(db.is_std_file(std_fid));
    assert!(db.is_main_file(main_fid));
    assert!(db.is_library_file(lib1_fid));
    assert!(db.is_library_file(lib2_fid));

    assert_eq!(db.main_workspace_file_ids(), vec![main_fid]);
    assert_eq!(db.std_workspace_file_ids(), vec![std_fid]);
    let mut libs = db.library_workspace_file_ids();
    libs.sort();
    assert_eq!(libs, vec![lib1_fid, lib2_fid]);

    let info = db.module_info_of(lib1_fid).expect("module info");
    assert_eq!(info.full_module_name, "mod");
    assert_eq!(info.workspace_id, crate::WorkspaceId { id: 3 });

    // Module tree node API
    let node_id = db.module_node("mod").expect("module node");
    let node = db.module_node_info(node_id).expect("module node info");
    assert_eq!(node.file_ids, vec![lib1_fid]);
    assert_eq!(db.module_node_file_ids(node_id), vec![lib1_fid]);
    assert!(db.module_node("lib1.mod").is_none());
}

#[test]
fn test_module_info_version_and_export_type() {
    let mut db = setup();
    db.add_main_workspace(PathBuf::from("C:/ws"));
    let fid = set_test_file(&mut db, 1, "C:/ws/mod.lua", "---@version 5.1\nreturn 42");

    let info = db.module_info_of(fid).expect("module info");
    assert!(
        !info.version_conds.is_empty(),
        "module version conds should be collected"
    );
    assert!(
        info.export_type.is_some(),
        "module export type should be projected"
    );
}

#[test]
fn test_vfs_file_ids_sorted_and_lookup_uses_snapshot() {
    let mut db = setup();
    let fid2 = set_test_file(&mut db, 2, "C:/ws/b.lua", "local b = 1");
    let fid1 = set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = 1");
    assert_eq!(db.file_ids(), vec![fid1, fid2]);
    assert_eq!(
        db.lookup_file_id(&crate::file_path_to_uri(&PathBuf::from("C:/ws/b.lua")).unwrap()),
        Some(fid2)
    );
}

#[test]
fn test_parallel_for_each_file_runs_on_shared_snapshots() {
    let mut db = setup();
    set_test_file(&mut db, 1, "C:/ws/a.lua", "local a = 1");
    set_test_file(&mut db, 2, "C:/ws/b.lua", "local b = 2");

    let visited = std::sync::atomic::AtomicUsize::new(0);
    db.parallel_for_each_file(|_file_id, model| {
        // Touch salsa-backed per-file facts from worker-owned database clones.
        let _ = model.file_facts();
        visited.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });

    assert_eq!(visited.load(std::sync::atomic::Ordering::Relaxed), 2);
}
