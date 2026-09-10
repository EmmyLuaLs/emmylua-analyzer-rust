//! P6 tests: unified cross-file callable/overload candidates.
//!
//! The first slice wires global same-name declarations into the VM and
//! call-site/diagnostic candidate paths; identical duplicate declarations keep
//! the legacy single-symbol behavior.

use crate::semantic_model::infer::callable::CallableCandidateSet;
use crate::semantic_model::infer::overload::CallArg;
use crate::{DiagnosticCode, LuaMemberKey, LuaType, LuaTypeDeclId, VirtualWorkspace};
use emmylua_parser::{LuaAstNode, LuaCallExpr};

fn is_integer_like(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Integer | LuaType::IntegerConst(_) | LuaType::Number
    )
}

fn is_string_like(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_)
    )
}

fn define_global_overloads(ws: &mut VirtualWorkspace) {
    ws.def_file(
        "a.lua",
        r#"
        ---@param value string
        ---@return string
        function f(value) end
        "#,
    );
    ws.def_file(
        "b.lua",
        r#"
        ---@param value number
        ---@return number
        function f(value) end
        "#,
    );
}

#[test]
fn p6_cross_file_global_overloads_are_visible_to_call_site_analysis() {
    let mut ws = VirtualWorkspace::new();
    define_global_overloads(&mut ws);

    let caller = ws.def("local value = f('x')");
    let model = ws.analysis.semantic_model(caller);
    let chunk = model.chunk().expect("chunk");
    let call = chunk.descendants::<LuaCallExpr>().next().expect("call");
    let analysis = model.call_site_analysis(&call);

    assert!(
        analysis.candidates.len() >= 2,
        "global overloads must reach call-site analysis: {:#?}",
        analysis.candidates
    );
    let has_string = analysis
        .candidates
        .iter()
        .any(|candidate| is_string_like(&candidate.get_ret()));
    let has_number = analysis
        .candidates
        .iter()
        .any(|candidate| is_integer_like(&candidate.get_ret()));
    assert!(
        has_string,
        "string overload missing: {:#?}",
        analysis.candidates
    );
    assert!(
        has_number,
        "number overload missing: {:#?}",
        analysis.candidates
    );
}

#[test]
fn p6_cross_file_global_overloads_select_by_argument_type() {
    let mut ws = VirtualWorkspace::new();
    define_global_overloads(&mut ws);

    let string_result = ws.expr_ty("f('x')");
    let number_result = ws.expr_ty("f(1)");
    assert!(
        is_string_like(&string_result),
        "string result: {string_result:?}"
    );
    assert!(
        is_integer_like(&number_result),
        "number result: {number_result:?}"
    );
}

#[test]
fn p6_identical_duplicate_globals_keep_legacy_behavior() {
    // The same source loaded twice must not turn into overloads; this mirrors
    // diagnostics running the same virtual source twice (issue 360).
    let mut ws = VirtualWorkspace::new();
    let source = r#"
        ---@alias buz number
        ---@param a buz
        ---@overload fun(): number
        function test(a) end
        local c = test({'test'})
    "#;
    assert!(ws.has_no_diagnostic(DiagnosticCode::RedundantParameter, source));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, source));
}

#[test]
fn p6b_repeated_field_overloads_select_by_arguments() {
    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file(
        "m.lua",
        r#"
        ---@class M
        ---@field f fun(a: string): string
        ---@field f fun(a: number): number
        local M = {}

        local a = M.f("x")
        local b = M.f(1)
        "#,
    );
    let model = ws.analysis.semantic_model(fid);
    let facts = model.file_facts().expect("facts");
    let decl_type = |name: &str| {
        let decl = facts.decl_named(name).expect("decl");
        model.type_of_decl(&decl.id).expect("type")
    };
    let a = decl_type("a");
    let b = decl_type("b");
    assert!(is_string_like(&a), "string overload: {a:?}");
    assert!(is_integer_like(&b), "number overload: {b:?}");
}

fn overloaded_member_source() -> &'static str {
    r#"
    ---@class M
    ---@field f fun(a: string): string
    ---@field f fun(a: number): number
    local M = {}
    "#
}

#[test]
fn p6b_member_call_site_candidates_include_all_field_overloads() {
    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file(
        "m.lua",
        &format!("{}\nlocal value = M.f(1)", overloaded_member_source()),
    );
    let model = ws.analysis.semantic_model(fid);
    let chunk = model.chunk().expect("chunk");
    let call = chunk.descendants::<LuaCallExpr>().next().expect("call");
    let analysis = model.call_site_analysis(&call);
    assert!(
        analysis.candidates.len() >= 2,
        "repeated @field overloads must reach call-site analysis: {:#?}",
        analysis.candidates
    );
    assert!(
        analysis
            .candidates
            .iter()
            .any(|candidate| is_string_like(&candidate.get_ret())),
        "string overload missing: {:#?}",
        analysis.candidates
    );
    assert!(
        analysis
            .candidates
            .iter()
            .any(|candidate| is_integer_like(&candidate.get_ret())),
        "number overload missing: {:#?}",
        analysis.candidates
    );
}

#[test]
fn p6b_inferred_call_doc_function_selects_matching_field_overload() {
    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file(
        "m.lua",
        &format!("{}\nlocal value = M.f(1)", overloaded_member_source()),
    );
    let model = ws.analysis.semantic_model(fid);
    let chunk = model.chunk().expect("chunk");
    let call = chunk.descendants::<LuaCallExpr>().next().expect("call");
    let fun = model
        .inferred_call_doc_function(call.get_syntax_id())
        .expect("inferred function");
    assert!(
        is_integer_like(fun.get_ret()),
        "selected overload return: {:?}",
        fun.get_ret()
    );
}

#[test]
fn p6b_member_overload_diagnostics_match_by_argument_type() {
    let mut ws = VirtualWorkspace::new();
    let source = format!(
        "{}
        local value = M.f(1)
        ",
        overloaded_member_source()
    );
    assert!(
        ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, &source),
        "matching number overload must not report ParamTypeMismatch"
    );
}

#[test]
fn p6b_callable_candidate_set_selects_and_returns_all_matches() {
    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file("m.lua", overloaded_member_source());
    let model = ws.analysis.semantic_model(fid);
    let ty = LuaType::Ref(LuaTypeDeclId::global("M"));
    let key = LuaMemberKey::Name("f".into());

    let candidates = CallableCandidateSet::from_prefix_type(&model, &ty, &key);
    assert!(
        candidates.candidates().len() >= 2,
        "candidates: {:?}",
        candidates
    );

    let args = [CallArg::new(LuaType::IntegerConst(1))];
    let selected = candidates
        .select(&model, &args, false, None)
        .expect("number overload selected");
    assert!(
        is_integer_like(selected.0.get_ret()),
        "selected return: {:?}",
        selected.0.get_ret()
    );

    let all = candidates.select_all(&model, &args, false, None);
    assert_eq!(all.len(), 1, "exact-match overload set: {all:?}");
}
