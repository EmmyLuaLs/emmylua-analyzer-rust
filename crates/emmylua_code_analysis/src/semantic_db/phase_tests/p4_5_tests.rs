//! P4.5 regression guards for keyed member lookup.
//!
//! The keyed APIs used by per-assignment diagnostics must not enumerate every
//! member of a runtime table; `full_owner_member_scans` stays at zero for name
//! lookups and would catch a regression to the old collect-then-filter path.

use crate::check::checker::redefined_local::scope_metrics;
use crate::semantic_model::flow::flow_metrics;
use crate::semantic_model::member::query_metrics;
use crate::{LuaMemberKey, LuaType, LuaTypeDeclId, VirtualWorkspace};

fn named(key: &str) -> LuaMemberKey {
    LuaMemberKey::Name(key.into())
}

#[test]
fn p4_5_keyed_member_lookup_uses_owner_name_bucket() {
    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file(
        "m.lua",
        r#"
        ---@class M
        ---@field a number
        ---@field b string
        local M = {}
        M.a = 1
        M.b = "x"
        "#,
    );
    let model = ws.analysis.semantic_model(fid);
    let ty = LuaType::Ref(LuaTypeDeclId::global("M"));

    query_metrics::reset();
    let a = model.member_infos_with_key(&ty, &named("a"));
    let b = model.member_infos_with_key(&ty, &named("b"));

    assert_eq!(a.len(), 1, "a: {a:?}");
    assert_eq!(b.len(), 1, "b: {b:?}");
    assert_eq!(
        query_metrics::full_owner_member_scans(),
        0,
        "keyed lookup must not enumerate all owner members"
    );
}

#[test]
fn p4_5_keyed_member_lookup_keeps_overloads() {
    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file(
        "m.lua",
        r#"
        ---@class M
        ---@field f fun(x: string)
        ---@field f fun(x: number)
        local M = {}
        "#,
    );
    let model = ws.analysis.semantic_model(fid);
    let ty = LuaType::Ref(LuaTypeDeclId::global("M"));

    query_metrics::reset();
    let all = model.member_infos_with_key_all(&ty, &named("f"));
    assert_eq!(all.len(), 2, "all overloads: {all:?}");
    let first = model.member_infos_with_key(&ty, &named("f"));
    assert_eq!(first.len(), 1, "deduped keyed view: {first:?}");
    assert_eq!(query_metrics::full_owner_member_scans(), 0);
}

#[test]
fn p4_5_flow_reads_use_indexed_fast_paths() {
    use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaLocalStat, LuaNameExpr};
    use std::fmt::Write;

    const N: usize = 200;
    let mut source = String::from("local M = {}\n");
    for i in 0..N {
        writeln!(source, "M.f{i} = {i}").unwrap();
    }
    for i in 0..N {
        writeln!(source, "local r{i} = M.f{i}").unwrap();
        writeln!(source, "local d{i} = M").unwrap();
    }

    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file("m.lua", &source);
    let model = ws.analysis.semantic_model(fid);
    let chunk = model.chunk().expect("chunk");

    let member_reads: Vec<_> = chunk
        .descendants::<LuaIndexExpr>()
        .filter(|expr| expr.ancestors::<LuaLocalStat>().next().is_some())
        .collect();
    let decl_reads: Vec<_> = chunk
        .descendants::<LuaNameExpr>()
        .filter(|expr| expr.get_text() == "M" && expr.ancestors::<LuaLocalStat>().next().is_some())
        .collect();
    assert_eq!(member_reads.len(), N, "member reads");
    assert!(decl_reads.len() >= N, "decl reads: {}", decl_reads.len());

    flow_metrics::reset();
    for expr in member_reads {
        let _ = model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start());
    }
    for expr in decl_reads {
        let _ = model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start());
    }

    let metrics = flow_metrics::trace_steps();
    assert!(
        flow_metrics::fast_path_hits() > 0,
        "indexed flow fast path must be used"
    );
    assert!(
        flow_metrics::fast_member_hits() > 0,
        "member fast path must be used"
    );
    assert!(
        flow_metrics::fast_decl_hits() > 0,
        "decl fast path must be used"
    );
    assert!(
        metrics < (N as u64) * 10,
        "indexed reads should not walk the whole flow chain: {metrics} steps for {N} reads"
    );
}

#[test]
fn p4_5_redefined_local_leaf_scopes_do_not_clone_parent_map() {
    use std::fmt::Write;

    const N: usize = 300;
    let mut source = String::new();
    for i in 0..N {
        writeln!(source, "local v{i} = {i}").unwrap();
    }

    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file("m.lua", &source);
    scope_metrics::reset();
    let _ = ws
        .analysis
        .diagnose_file(fid, tokio_util::sync::CancellationToken::new());
    assert_eq!(
        scope_metrics::cloned_local_entries(),
        0,
        "leaf local scopes must merge into the parent map in place"
    );
}

#[test]
fn p4_5_flow_does_not_shortcut_loop_or_branch_member_reads() {
    use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaLocalStat};

    let source = r#"
        local M = {}
        local cond = true
        while cond do
            M.x = "s"
            cond = false
        end
        if cond then
            M.y = 1
        else
            M.y = "s"
        end
        local a = M.x
        local b = M.y
    "#;

    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file("m.lua", source);
    let model = ws.analysis.semantic_model(fid);
    let chunk = model.chunk().expect("chunk");
    let reads: Vec<_> = chunk
        .descendants::<LuaIndexExpr>()
        .filter(|expr| {
            expr.ancestors::<LuaLocalStat>().next().is_some()
                && expr
                    .get_index_name_token()
                    .is_some_and(|name| matches!(name.text(), "x" | "y"))
        })
        .collect();
    assert_eq!(reads.len(), 2, "reads: {reads:?}");

    flow_metrics::reset();
    for expr in reads {
        let ty = model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start());
        assert!(!matches!(ty, LuaType::Unknown), "flow type: {ty:?}");
    }
    assert_eq!(
        flow_metrics::fast_member_hits(),
        0,
        "member fast path must not shortcut loop/branch flow"
    );
}
