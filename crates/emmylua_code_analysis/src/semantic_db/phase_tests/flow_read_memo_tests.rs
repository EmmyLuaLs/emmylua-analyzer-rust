//! Regression guards for two quadratic costs of reading a local through a long chain of flow
//! nodes that leave it alone or write it:
//!
//! ```lua
//! ---@param h H
//! local function f(h)
//!   if h.a then elseif h.b then end -- n times
//! end
//!
//! local function g()
//!   local x = 0
//!   x = x + 1 -- n times
//!   return x
//! end
//! ```
//!
//! A value read met the memo only where it started, so each read walked back through every
//! earlier untouched merge, or every earlier write: n reads × n nodes. 0.25.1 is linear here.

use crate::VirtualWorkspace;
use crate::semantic_model::flow::flow_metrics;
use emmylua_parser::{LuaAstNode, LuaNameExpr};

fn untouched_merge_chain(n: usize) -> String {
    let mut content = String::from(
        "---@class H\n---@field a? boolean\n---@field b? boolean\n\n---@param h H\nlocal function f(h)\n",
    );
    for _ in 0..n {
        content.push_str("  if h.a then elseif h.b then end\n");
    }
    content.push_str("end\nreturn f\n");
    content
}

fn reassigned_chain(n: usize) -> String {
    let mut content = String::from("local function g()\n  local x = 0\n");
    for _ in 0..n {
        content.push_str("  x = x + 1\n");
    }
    content.push_str("  return x\nend\nreturn g\n");
    content
}

/// Flow steps to read every `name` in `source`.
fn read_steps(source: &str, name: &str) -> u64 {
    let mut ws = VirtualWorkspace::new();
    let fid = ws.def_file("chain.lua", source);
    let model = ws.analysis.semantic_model(fid);
    flow_metrics::reset();
    for expr in model.chunk().expect("chunk").descendants::<LuaNameExpr>() {
        if expr.get_name_text().as_deref() == Some(name) {
            let _ = model.type_of_expr_at(expr.get_syntax_id(), expr.get_position());
        }
    }
    flow_metrics::trace_steps() + flow_metrics::branch_scan_steps()
}

/// Steps at n = 16, 64, 256 stay within twice a linear extrapolation from n = 16.
fn assert_linear(what: &str, steps_at: impl Fn(usize) -> u64) {
    let measurements: Vec<_> = [16usize, 64, 256]
        .into_iter()
        .map(|n| (n, steps_at(n)))
        .collect();
    let (n_first, steps_first) = measurements[0];
    for (n, steps) in &measurements {
        let bound = steps_first * (*n as u64) / (n_first as u64) * 2;
        assert!(
            *steps <= bound,
            "{steps} flow steps for {n} {what} (bound {bound}, linear from n = {n_first}): \
             each read walks back through all earlier ones. All: {measurements:?}"
        );
    }
}

#[test]
fn reads_past_untouched_merges_stay_linear() {
    assert_linear("conditionals", |n| {
        read_steps(&untouched_merge_chain(n), "h")
    });
}

#[test]
fn reads_of_a_reassigned_local_stay_linear() {
    assert_linear("writes", |n| read_steps(&reassigned_chain(n), "x"));
}
