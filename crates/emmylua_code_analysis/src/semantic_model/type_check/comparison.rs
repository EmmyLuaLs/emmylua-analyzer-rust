//! Evaluate a finite graph of comparisons without recursive checker calls.
//!
//! A pending comparison is not evidence. Retry its parents only when a
//! dependency provides a result; a cycle without a proof stays unresolved.

use hashbrown::{HashMap, HashSet};
use std::collections::VecDeque;

use super::{TypeCheckContext, TypeCheckFailReason, TypeCheckMode, TypeCheckResult};
use crate::LuaType;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Comparison {
    source: LuaType,
    target: LuaType,
    mode: TypeCheckMode,
    strict_union: bool,
    strict_object: bool,
    strict_generic: bool,
    parameter: bool,
}

struct Node {
    result: TypeCheckResult,
    parents: HashSet<Comparison>,
    queued: bool,
}

#[derive(Default)]
pub(super) struct Comparisons {
    nodes: HashMap<Comparison, Node>,
    pending: VecDeque<Comparison>,
    current: Option<Comparison>,
}

impl Comparisons {
    fn read(&mut self, key: &Comparison) -> TypeCheckResult {
        let node = self.nodes.entry(key.clone()).or_insert_with(|| {
            self.pending.push_back(key.clone());
            Node {
                result: Err(TypeCheckFailReason::Unresolved),
                parents: HashSet::new(),
                queued: true,
            }
        });
        if let Some(parent) = &self.current {
            node.parents.insert(parent.clone());
        }
        node.result.clone()
    }

    fn finish(&mut self, key: &Comparison, result: TypeCheckResult) {
        let node = self
            .nodes
            .get_mut(key)
            .expect("comparison must be scheduled");
        let complete = !result.as_ref().is_err_and(|err| err.is_unresolved());
        node.result = result;
        if !complete {
            return;
        }
        let parents: Vec<_> = node.parents.iter().cloned().collect();
        for parent in parents {
            let node = self
                .nodes
                .get_mut(&parent)
                .expect("parent must be scheduled");
            if node.result.as_ref().is_err_and(|err| err.is_unresolved()) && !node.queued {
                node.queued = true;
                self.pending.push_back(parent);
            }
        }
    }
}

pub(super) fn check(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
    parameter: bool,
) -> TypeCheckResult {
    let key = Comparison {
        source: source.clone(),
        target: target.clone(),
        mode: context.mode,
        strict_union: context.strict_union,
        strict_object: context.strict_object,
        strict_generic: context.strict_generic,
        parameter,
    };
    let comparisons = context.comparisons.clone();
    let result = comparisons.borrow_mut().read(&key);
    if comparisons.borrow().current.is_some() {
        return result;
    }
    loop {
        let next = {
            let mut graph = comparisons.borrow_mut();
            if !graph.nodes[&key]
                .result
                .as_ref()
                .is_err_and(|err| err.is_unresolved())
            {
                return graph.nodes[&key].result.clone();
            }
            graph.pending.pop_front()
        };
        let Some(next) = next else { break };
        {
            let mut graph = comparisons.borrow_mut();
            let node = graph
                .nodes
                .get_mut(&next)
                .expect("comparison must be scheduled");
            node.queued = false;
            if !node.result.as_ref().is_err_and(|err| err.is_unresolved()) {
                continue;
            }
        }
        let result = evaluate(context, &next);
        comparisons.borrow_mut().finish(&next, result);
    }
    let graph = comparisons.borrow();
    graph
        .nodes
        .get(&key)
        .expect("root must be scheduled")
        .result
        .clone()
}

fn evaluate(context: &TypeCheckContext, key: &Comparison) -> TypeCheckResult {
    {
        let mut graph = context.comparisons.borrow_mut();
        graph.current = Some(key.clone());
    }
    let mut current = TypeCheckContext {
        mode: key.mode,
        strict_union: key.strict_union,
        strict_object: key.strict_object,
        strict_generic: key.strict_generic,
        ..context.clone()
    };
    let result = if key.parameter {
        super::assign::check_parameter(&mut current, &key.source, &key.target)
    } else {
        super::check_comparison(&mut current, &key.source, &key.target)
    };
    context.comparisons.borrow_mut().current = None;
    result
}
