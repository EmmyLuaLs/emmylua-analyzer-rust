//! Regression guards for flow backtracking in files that contain a loop.
//!
//! One loop anywhere in a file disabled trace-state memoization for every
//! function in it, so a chain of `n` guards in a loop-free function walked
//! ~2^n routes (`walking.lua` of an OTClient project never finished).

use emmylua_parser::{LuaAstNode, LuaNameExpr};
use rowan::TextSize;

use crate::semantic_model::flow::flow_metrics;
use crate::{FileId, VirtualWorkspace};

/// Every loop statement the binder knows; each one used to turn the memo off for
/// the whole file.
const LOOP_KINDS: &[(&str, &str)] = &[
    ("for-in", "for _ in pairs(t) do end"),
    ("numeric for", "for i = 1, #t do end"),
    ("while", "while t do t = nil end"),
    ("repeat", "repeat t = nil until not t"),
];

/// `n` sequential guards in `walk`, plus an unrelated `loop_stat` in `other`.
fn guard_chain_with_loop_elsewhere(n: usize, loop_stat: &str) -> String {
    let mut content = String::from("lastDir = nil\nwalkLock = 0\n\nfunction walk(dir)\n");
    for i in 0..n {
        content.push_str(&format!(
            "  if walkLock > {i} and lastDir == dir then\n    walkLock = walkLock + 1\n  end\n"
        ));
    }
    content.push_str(&format!(
        "  lastDir = dir\nend\n\nfunction other(t)\n  {loop_stat}\nend\n"
    ));
    content
}

const BACKWARD_GOTO: &str = "
function retry(t)
  ::again::
  walkLock = 0
  if t then goto again end
end
";

/// Read the flow type of every name, the way checkers do for each reference.
fn read_every_name(ws: &VirtualWorkspace, file_id: FileId) {
    let model = ws.analysis.semantic_model(file_id);
    for name in model.chunk().expect("chunk").descendants::<LuaNameExpr>() {
        let _ = model.type_of_expr_at(name.get_syntax_id(), name.get_position());
    }
}

/// Only nodes with a real cycle behind them (a backward `goto`) lose the memo; a loop
/// or label elsewhere in the file no longer affects `walk`.
#[test]
fn flow_loop_reachability_is_tracked_per_node() {
    let mut ws = VirtualWorkspace::new();
    let source = guard_chain_with_loop_elsewhere(2, LOOP_KINDS[0].1) + BACKWARD_GOTO;
    let file_id = ws.def_file("walking.lua", &source);
    let model = ws.analysis.semantic_model(file_id);
    let tree = model.flow_tree().expect("flow tree");
    let flow_at = |needle: &str| {
        let offset = source.find(needle).expect("needle in source") as u32;
        tree.get_flow_id_at(TextSize::from(offset))
            .unwrap_or_else(|| panic!("no flow node at `{needle}` (offset {offset})"))
    };

    assert!(tree.has_complex_control());
    assert!(
        !tree.is_loop_reachable(flow_at("lastDir = dir")),
        "`walk` has no loop behind it, so its trace states stay memoizable"
    );
    assert!(
        tree.is_loop_reachable(flow_at("walkLock = 0\n  if t")),
        "a statement behind the backward `goto` must keep the uncached path"
    );
}

/// The number of uncached walks must stay linear in the number of guards for
/// every loop kind in another function: `16n + 3` (`for`) or `16n + 4` (`while`,
/// `repeat`) with the per-node gate. With the per-file gate it was already 631 at
/// n=4 and `[(8, 52455), (16, 2065146), (32, 6638442)]` after that, so the check
/// runs per size and fails on the first one instead of walking the larger chains.
#[test]
fn flow_loop_in_other_function_keeps_guard_chain_linear() {
    for (kind, loop_stat) in LOOP_KINDS {
        let mut measurements = Vec::new();
        for n in [4usize, 8, 16, 32] {
            let mut ws = VirtualWorkspace::new();
            let source = guard_chain_with_loop_elsewhere(n, loop_stat);
            let file_id = ws.def_file("walking.lua", &source);
            flow_metrics::reset();
            read_every_name(&ws, file_id);
            let entries = flow_metrics::trace_decl_entries();
            measurements.push((n, entries));

            let bound = 32 * (n as u64) + 64;
            assert!(
                entries <= bound,
                "the declaration walk ran {entries} times for {n} guards (bound {bound}); \
                 a `{kind}` loop in another function disabled the trace-state memo. \
                 So far: {measurements:?}"
            );
        }
    }
}
