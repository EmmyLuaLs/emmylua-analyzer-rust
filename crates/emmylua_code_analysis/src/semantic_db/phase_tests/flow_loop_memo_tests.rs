//! Regression guards for flow backtracking in files that contain a loop.
//!
//! One loop anywhere in a file disabled trace-state memoization for every
//! function in it, so a chain of `n` guards in a loop-free function walked
//! ~2^n routes (`walking.lua` of a real game client project never finished).

use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaNameExpr};
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

/// Loops the binder knows are entered, so the code after them is reached through the loop.
const ENTERED_LOOP_KINDS: &[(&str, &str)] = &[
    ("numeric for", "for i = 1, 2 do walkLock = i end"),
    ("while", "while true do break end"),
    ("repeat", "repeat walkLock = 0 until walkLock == 0"),
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

/// `n` sequential guards in `walk`, after `loop_stat` in the same function.
fn guard_chain_after_loop(n: usize, loop_stat: &str) -> String {
    let mut content = format!("lastDir = nil\nwalkLock = 0\n\nfunction walk(dir)\n  {loop_stat}\n");
    for i in 0..n {
        content.push_str(&format!(
            "  if walkLock > {i} and lastDir == dir then\n    walkLock = walkLock + 1\n  end\n"
        ));
    }
    content.push_str("  lastDir = dir\nend\n");
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

/// The code after an entered loop is reached through it but does not lead back into it,
/// so its trace states stay memoizable. A statement behind a backward `goto` is on a cycle.
#[test]
fn flow_cycle_membership_is_tracked_per_node() {
    let mut ws = VirtualWorkspace::new();
    let source = guard_chain_after_loop(2, ENTERED_LOOP_KINDS[0].1) + BACKWARD_GOTO;
    let file_id = ws.def_file("walking.lua", &source);
    let model = ws.analysis.semantic_model(file_id);
    let tree = model.flow_tree().expect("flow tree");
    let flow_at = |needle: &str| {
        let offset = source.find(needle).expect("needle in source") as u32;
        tree.get_flow_id_at(TextSize::from(offset))
            .unwrap_or_else(|| panic!("no flow node at `{needle}` (offset {offset})"))
    };

    let after_loop = flow_at("lastDir = dir");
    assert!(
        tree.is_loop_reachable(after_loop),
        "the guards after the loop are reached through its body"
    );
    assert!(
        !tree.is_in_cycle(after_loop),
        "nothing after the loop leads back into it, so its trace states stay memoizable"
    );
    assert!(
        tree.is_in_cycle(flow_at("walkLock = 0\n  if t")),
        "a statement behind the backward `goto` must keep the uncached path"
    );
}

/// An entered loop turned the memo off for every guard after it in the same function, so the
/// walk grew as it did with the per-file gate (631 at n=4 after the numeric `for`).
#[test]
fn flow_entered_loop_keeps_later_guard_chain_linear() {
    for (kind, loop_stat) in ENTERED_LOOP_KINDS {
        let mut measurements = Vec::new();
        for n in [4usize, 8, 16, 32] {
            let mut ws = VirtualWorkspace::new();
            let source = guard_chain_after_loop(n, loop_stat);
            let file_id = ws.def_file("walking.lua", &source);
            flow_metrics::reset();
            read_every_name(&ws, file_id);
            let entries = flow_metrics::trace_decl_entries();
            measurements.push((n, entries));

            let bound = 32 * (n as u64) + 64;
            assert!(
                entries <= bound,
                "the declaration walk ran {entries} times for {n} guards (bound {bound}); \
                 a `{kind}` loop before them disabled the trace-state memo. \
                 So far: {measurements:?}"
            );
        }
    }
}

/// `n` guards on one field, each read again after its merge, with `prelude` before them.
fn member_guard_chain(n: usize, prelude: &str) -> String {
    let mut content = format!(
        "---@class Hunk\n---@field count integer?\n\n---@param hunk Hunk\nlocal function f(hunk)\n{prelude}"
    );
    for _ in 0..n {
        content.push_str("  if hunk.count then\n    print(1)\n  end\n  print(hunk.count)\n");
    }
    content.push_str("end\n");
    content
}

/// A guard on a field leaves a narrowing on the path, and the member memo used to serve
/// only an empty path, so every read walked both sides of every earlier guard. diffs.nvim's
/// `highlight_hunk` never finished. An entered loop before the guards turned the memo off for
/// all of them. The walk must stay polynomial in the number of guards.
#[test]
fn flow_member_guard_chain_keeps_the_memo() {
    for prelude in ["", "  for i = 1, 2 do print(i) end\n"] {
        let mut measurements = Vec::new();
        for n in [4usize, 8, 12, 16] {
            let mut ws = VirtualWorkspace::new();
            let file_id = ws.def_file("hunk.lua", &member_guard_chain(n, prelude));
            let model = ws.analysis.semantic_model(file_id);
            flow_metrics::reset();
            for index in model.chunk().expect("chunk").descendants::<LuaIndexExpr>() {
                let _ = model.type_of_expr_at(index.get_syntax_id(), index.get_position());
            }
            let steps = flow_metrics::trace_steps();
            measurements.push((n, steps));

            let bound = 64 * (n as u64) * (n as u64) + 256;
            assert!(
                steps <= bound,
                "the member walk took {steps} steps for {n} guards after {prelude:?} \
                 (bound {bound}). So far: {measurements:?}"
            );
        }
    }
}

#[test]
fn flow_correlated_guard_keeps_earlier_guards_cached() {
    for n in [4usize, 8, 16, 32, 64, 128] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut source = String::from(
            r#"
            ---@param flag boolean
            ---@return boolean, string?
            ---@return_overload true, string
            ---@return_overload false, nil
            local function pick(flag)
                if flag then return true, 'hello' end
                return false, nil
            end
            ---@param flag boolean
            local function read(flag)
                local ok, value = pick(flag)
            "#,
        );
        for _ in 0..n {
            source.push_str("if type(value) == 'string' then print(value) end\n");
        }
        source.push_str("if ok then print(value:upper()) end\nend\n");
        let file_id = ws.def_file("correlated.lua", &source);
        flow_metrics::reset();
        read_every_name(&ws, file_id);
        let steps = flow_metrics::trace_steps();
        let bound = 128 * (n as u64) * (n as u64) + 512;
        assert!(
            steps <= bound,
            "correlated guard took {steps} trace steps for {n} preceding guards (bound {bound})"
        );
    }
}

#[test]
fn flow_distinct_correlated_guards_keep_the_memo() {
    for n in [4usize, 8, 16, 32, 64, 128, 256] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut source = String::from("---@param n integer\n---@return integer, integer\n");
        for i in 1..=n {
            source.push_str(&format!("---@return_overload {i}, {i}\n"));
        }
        source.push_str("local function pick(n)\n");
        for i in 1..n {
            source.push_str(&format!("if n == {i} then return {i}, {i} end\n"));
        }
        source.push_str(&format!("return {n}, {n}\nend\n"));
        source
            .push_str("---@param n integer\nlocal function read(n)\nlocal tag, value = pick(n)\n");
        for i in 1..=n {
            source.push_str(&format!("if value == {i} then print(value) end\n"));
        }
        source.push_str("if tag == 1 then print(value) end\nend\n");
        let file_id = ws.def_file("distinct.lua", &source);
        flow_metrics::reset();
        read_every_name(&ws, file_id);
        let steps = flow_metrics::trace_steps();
        let rows = flow_metrics::correlated_row_work();
        let bound = 128 * (n as u64) * (n as u64) + 512;
        assert!(
            steps <= bound,
            "{n} distinct guards took {steps} trace steps (bound {bound})"
        );
        assert!(
            rows <= 16 * (n as u64) * (n as u64),
            "{n} guards inspected {rows} return rows"
        );
        assert_eq!(
            flow_metrics::return_rows_built(),
            n as u64,
            "{n} guards should build the call's return rows once"
        );
    }
}
