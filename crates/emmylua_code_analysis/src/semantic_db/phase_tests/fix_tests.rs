//! Correctness regression tests for inference/checking issues found in real projects.
//! Each test here maps to a concrete false-positive/false-negative report.

use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

#[test]
fn fix_table_insert_accepts_two_and_three_arg_forms() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    let source = r#"
        local t = {}
        table.insert(t, 1)
        table.insert(t, "str")
        table.insert(t, {})
        table.insert(t, 1, 2)
    "#;
    assert!(
        ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, source),
        "table.insert 2-arg overload must be considered"
    );
}

fn local_type(ws: &VirtualWorkspace, file_id: crate::FileId, name: &str) -> LuaType {
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

#[test]
fn fix_global_table_field_cross_file_decl_first() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file("def.lua", "J = {}");
    ws.def_file("field.lua", "J.foo = 1");
    let consumer = ws.def_file("consumer.lua", "local v = J.foo");
    let ty = local_type(&ws, consumer, "v");
    assert!(is_integer_like(&ty), "cross-file global field type: {ty:?}");
}

#[test]
fn fix_global_table_field_cross_file_field_first() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file("field.lua", "J.foo = 1");
    ws.def_file("def.lua", "J = {}");
    let consumer = ws.def_file("consumer.lua", "local v = J.foo");
    let ty = local_type(&ws, consumer, "v");
    assert!(is_integer_like(&ty), "cross-file global field type: {ty:?}");
}

#[test]
fn fix_global_table_repeated_field_assignment_not_duplicate() {
    let mut ws = VirtualWorkspace::new();
    let source = r#"
        J = {}
        J.nAccelerateLevel = 1
        J.nAccelerateLevel = 2
    "#;
    assert!(
        ws.has_no_diagnostic(DiagnosticCode::DuplicateSetField, source),
        "repeated runtime field assignment must not be reported as duplicate"
    );
}

#[test]
fn fix_doc_type_on_member_assignment() {
    let mut ws = VirtualWorkspace::new();
    let source = r#"
        Config = {}
        ---@alias Level string
        ---@type Level
        Config.nAccelerateLevel = 1
        local v = Config.nAccelerateLevel
    "#;
    let fid = ws.def(source);
    let ty = local_type(&ws, fid, "v");
    let is_level_alias = matches!(&ty, LuaType::Ref(id) if id.get_name() == "Level");
    assert!(
        is_level_alias
            || matches!(
                ty,
                LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_)
            ),
        "---@type on member assignment must win: {ty:?}"
    );
}

#[test]
fn fix_require_strips_prefix_to_find_target_module() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file("mod.lua", "local M = {}; M.value = 42; return M");
    let consumer = ws.def_file(
        "consumer.lua",
        r#"
        local m = require("some.prefix.mod")
        local v = m.value
        "#,
    );
    let ty = local_type(&ws, consumer, "v");
    assert!(
        is_integer_like(&ty),
        "require prefix stripping must find mod.lua: {ty:?}"
    );
}

#[test]
fn fix_doc_type_trailing_member_assignment() {
    let mut ws = VirtualWorkspace::new();
    let source = r#"
        Config = {}
        ---@alias Level string
        Config.nAccelerateLevel = 1 ---@type Level
        local v = Config.nAccelerateLevel
    "#;
    let fid = ws.def(source);
    let ty = local_type(&ws, fid, "v");
    let is_level_alias = matches!(&ty, LuaType::Ref(id) if id.get_name() == "Level");
    assert!(
        is_level_alias
            || matches!(
                ty,
                LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_)
            ),
        "trailing ---@type on member assignment must win: {ty:?}"
    );
}

#[test]
fn fix_global_table_many_cross_file_fields_and_function() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file("def.lua", "J = {}");
    ws.def_file("a.lua", "J.a = 1");
    ws.def_file("b.lua", "J.b = \"s\"");
    ws.def_file("f.lua", "function J.f() return 7 end");
    let consumer = ws.def_file(
        "consumer.lua",
        r#"
        local a = J.a
        local b = J.b
        local f = J.f()
        "#,
    );
    let model = ws.analysis.semantic_model(consumer);
    let facts = model.file_facts().expect("facts");
    let decl_type = |name: &str| {
        let decl = facts.decl_named(name).expect("decl");
        model.type_of_decl(&decl.id).expect("type")
    };
    assert!(
        is_integer_like(&decl_type("a")),
        "J.a: {:?}",
        decl_type("a")
    );
    assert!(
        matches!(decl_type("b"), LuaType::String | LuaType::StringConst(_)),
        "J.b: {:?}",
        decl_type("b")
    );
    assert!(
        is_integer_like(&decl_type("f")),
        "J.f(): {:?}",
        decl_type("f")
    );
}
