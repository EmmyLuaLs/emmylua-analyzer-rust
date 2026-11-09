//! Regression guards for flow backtracking on a repeated nested-index guard chain.
//!
//! Shape (`n` identical blocks; the *second* index expression inside one condition
//! list is what triggers it):
//!
//! ```lua
//! V = V or {}
//! if V[k1] and V[k1][k2] then V[k1][k2][s] = ... end
//! ```
//!
//! Each block's guard assigns the traced global, so the two merge arms genuinely
//! differ and both must be walked. Before the declaration-trace states were
//! memoized (`cache::TraceStateKey`, applied only on loop-free flow graphs) the
//! same states were re-derived along ~2^n routes:
//!
//! | n | 2 | 4 | 6 | 8 | 10 | 12 | 16 | 20 |
//! |---|---:|---:|---:|---:|---:|---:|---:|---:|
//! | walk entries | 13 | 81 | 365 | 1513 | 6117 | 24545 | 393177 | 6.3M |
//!
//! With the memo the 600-block payload of
//! `test_issue_1028_i18n_semantic_tokens_repeated_prefix_guard_chain` in
//! `emmylua_ls` finishes in well under a second.

use crate::semantic_model::flow::flow_metrics;
use crate::{LuaType, VirtualWorkspace};
use emmylua_parser::{LuaAstNode, LuaIndexExpr};

/// `n` guard blocks, each repeating one nested index expression in a condition.
fn repeated_prefix_guard_chain(n: usize) -> String {
    let mut content = String::from("V_cfad19afc42b = V_cfad19afc42b or {}\n");
    for i in 0..n {
        let table_key = 3_121_212;
        let field_key = 1_111_112 + i;
        content.push_str(&format!(
            "if V_cfad19afc42b[{table_key}] and V_cfad19afc42b[{table_key}][{field_key}] then\n    V_cfad19afc42b[{table_key}][{field_key}][\"__STR_{i}__\"] = \"__STR_{}__\"\nend\n\n",
            i + 1,
        ));
    }
    content
}

/// Read every resolvable member of the file, the way the semantic-token builder
/// does for every name token it renders.
fn read_every_member(fid: crate::FileId, ws: &VirtualWorkspace) {
    let model = ws.analysis.semantic_model(fid);
    for index_expr in model.chunk().expect("chunk").descendants::<LuaIndexExpr>() {
        let Some(resolved) = model.resolve_member(&index_expr) else {
            continue;
        };
        let Some(member_id) = resolved.member_id else {
            continue;
        };
        let _ = model.type_of_member_at(&member_id, index_expr.get_range().start());
    }
}

/// The shape must resolve consistently for every size.
#[test]
fn p11_flow_nested_index_guard_chain_types_are_stable() {
    for n in [1usize, 4, 20, 60] {
        let mut ws = VirtualWorkspace::new();
        let fid = ws.def_file("probe.lua", &repeated_prefix_guard_chain(n));
        read_every_member(fid, &ws);
    }

    let mut nested = VirtualWorkspace::new();
    nested.def_file("nested.lua", &repeated_prefix_guard_chain(4));
    let nested_ty = nested.expr_ty("V_cfad19afc42b[3121212][1111115]");

    // Every index is into an undeclared global table, so the read stays unknown;
    // the point is that it resolves without error and without depending on n.
    assert!(
        matches!(nested_ty, LuaType::Unknown),
        "unexpected type for a nested-index read of an undeclared global: {nested_ty:?}"
    );
}

/// The number of uncached walks must stay linear in the number of blocks.
///
/// Distinct states per chain position are constant, so memoization should hold the
/// entry count to a small multiple of `n`; the pre-memo behaviour exceeded the
/// bound by orders of magnitude already at n=8 and never finished for n=600.
#[test]
fn p11_flow_repeated_nested_index_guard_chain_is_not_exponential() {
    let mut measurements = Vec::new();
    for n in [4usize, 16, 64, 256, 600] {
        let mut ws = VirtualWorkspace::new();
        let fid = ws.def_file("probe.lua", &repeated_prefix_guard_chain(n));
        flow_metrics::reset();
        read_every_member(fid, &ws);
        measurements.push((n, flow_metrics::trace_decl_entries()));
    }

    for (n, entries) in &measurements {
        let bound = 500 + 400 * (*n as u64);
        assert!(
            *entries <= bound,
            "the declaration walk ran {entries} times for {n} guard blocks (bound {bound}); \
             the trace-state memo is not bounding the work. All: {measurements:?}"
        );
    }

    // Growth must stay roughly linear: 150x the input must not cost more than
    // roughly 150x (plus the fixed per-file constant, already covered above).
    let (n_last, entries_last) = *measurements.last().expect("measurements");
    let (n_first, entries_first) = *measurements.first().expect("measurements");
    let (n_last, n_first) = (n_last as u64, n_first as u64);
    assert!(
        entries_last * n_first <= entries_first * n_last * 8,
        "cost grew faster than linear in the number of guard blocks: {measurements:?}"
    );
}
