//! Evaluate recursive comparisons with a work queue.
//!
//! A structural cycle can provisionally match. Retry its dependents whenever
//! a result changes, and settle the dependencies that support the final result.

use hashbrown::hash_map::EntryRef;
use hashbrown::{HashMap, HashSet};
use std::collections::VecDeque;

use super::{TypeCheckContext, TypeCheckFailReason, TypeCheckResult};
use crate::LuaType;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Comparison {
    source: LuaType,
    target: LuaType,
    strict_subtype: bool,
}

struct Node {
    key: Comparison,
    result: TypeCheckResult,
    parents: HashSet<usize>,
    dependencies: Vec<usize>,
    queued: bool,
}

#[derive(Default)]
pub(super) struct Comparisons {
    index: HashMap<Comparison, usize>,
    nodes: Vec<Node>,
    pending: VecDeque<usize>,
    current: Option<usize>,
    reads: Vec<(usize, bool)>,
}

impl Comparisons {
    fn intern(&mut self, key: &Comparison) -> usize {
        let id = self.nodes.len();
        match self.index.entry_ref(key) {
            EntryRef::Occupied(entry) => return *entry.get(),
            EntryRef::Vacant(entry) => {
                entry.insert(id);
            }
        }
        self.nodes.push(Node {
            key: key.clone(),
            result: Err(TypeCheckFailReason::Unresolved),
            parents: HashSet::new(),
            dependencies: Vec::new(),
            queued: true,
        });
        self.pending.push_back(id);
        id
    }

    fn read(&mut self, key: &Comparison) -> TypeCheckResult {
        let previous_len = self.nodes.len();
        let id = self.intern(key);
        let assume = id < previous_len
            && self.nodes[id].result == Err(TypeCheckFailReason::Unresolved)
            && self.guarded_cycle(id);
        let result = if assume {
            Ok(())
        } else {
            self.nodes[id].result.clone()
        };
        if let Some(parent) = self.current {
            self.nodes[id].parents.insert(parent);
            if !result.as_ref().is_err_and(|err| err.is_unresolved()) {
                self.reads.push((id, result.is_ok()));
            }
        }
        result
    }

    /// Only a cycle through data constructors can justify a recursive match.
    /// Alias forwarding and union alternatives alone provide no evidence.
    fn guarded_cycle(&self, key: usize) -> bool {
        fn is_structure(ty: &LuaType) -> bool {
            matches!(
                ty,
                LuaType::Object(_)
                    | LuaType::Array(_)
                    | LuaType::Tuple(_)
                    | LuaType::TableGeneric(_)
                    | LuaType::DocFunction(_)
            )
        }
        let mut pending: Vec<_> = self.current.iter().map(|key| (*key, false)).collect();
        let mut seen = HashSet::new();
        let mut guarded_cycle = false;
        while let Some((previous, guarded)) = pending.pop() {
            let comparison = &self.nodes[previous].key;
            let guarded =
                guarded || (is_structure(&comparison.source) && is_structure(&comparison.target));
            if !seen.insert((previous, guarded)) {
                continue;
            }
            if previous == key {
                if !guarded {
                    return false;
                }
                guarded_cycle = true;
            } else {
                pending.extend(
                    self.nodes[previous]
                        .parents
                        .iter()
                        .map(|parent| (*parent, guarded)),
                );
            }
        }
        guarded_cycle
    }

    fn finish(&mut self, id: usize, result: TypeCheckResult) {
        // Only definite reads with the same outcome support a proof.
        let dependencies = self
            .reads
            .drain(..)
            .filter_map(|(child, matched)| (result.is_ok() == matched).then_some(child))
            .collect();
        let node = &mut self.nodes[id];
        node.dependencies = dependencies;
        if node.result == result {
            return;
        }
        node.result = result;
        let parents: Vec<_> = node.parents.iter().copied().collect();
        for parent in parents {
            let node = &mut self.nodes[parent];
            if !node.queued {
                node.queued = true;
                self.pending.push_back(parent);
            }
        }
    }

    fn is_complete(&self, root: usize) -> bool {
        // Avoid allocating a traversal when the root alone decides completeness.
        let node = &self.nodes[root];
        if node.queued || node.result.as_ref().is_err_and(|err| err.is_unresolved()) {
            return false;
        }
        if node.dependencies.is_empty() {
            return true;
        }
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let node = &self.nodes[id];
            if node.queued || node.result.as_ref().is_err_and(|err| err.is_unresolved()) {
                return false;
            }
            pending.extend(&node.dependencies);
        }
        true
    }
}

pub(super) fn check(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: &LuaType,
) -> TypeCheckResult {
    let key = Comparison {
        source: source.clone(),
        target: target.clone(),
        strict_subtype: context.strict_subtype,
    };
    let comparisons = context.comparisons.clone();
    if comparisons.borrow().current.is_some() {
        return comparisons.borrow_mut().read(&key);
    }
    let Some(evaluation) =
        crate::semantic_model::cache::TypeEvaluationScope::new(&context.model.cache)
    else {
        return Err(TypeCheckFailReason::TooComplex);
    };
    let root = comparisons.borrow_mut().intern(&key);
    loop {
        let next = {
            let mut graph = comparisons.borrow_mut();
            if graph.is_complete(root) {
                return graph.nodes[root].result.clone();
            }
            graph.pending.pop_front()
        };
        let Some(next) = next else {
            return Err(if context.model.cache.borrow().evaluation_exhausted {
                TypeCheckFailReason::TooComplex
            } else {
                TypeCheckFailReason::Unresolved
            });
        };
        comparisons.borrow_mut().nodes[next].queued = false;
        let key = comparisons.borrow().nodes[next].key.clone();
        if !evaluation.visit(&key.source) || !evaluation.visit(&key.target) {
            return Err(TypeCheckFailReason::TooComplex);
        }
        let result = evaluate(context, next, &key);
        comparisons.borrow_mut().finish(next, result);
    }
}

fn evaluate(context: &TypeCheckContext, id: usize, key: &Comparison) -> TypeCheckResult {
    {
        let mut graph = context.comparisons.borrow_mut();
        graph.current = Some(id);
    }
    let mut current = TypeCheckContext {
        strict_subtype: key.strict_subtype,
        ..context.clone()
    };
    let result = super::check_comparison(&mut current, &key.source, &key.target);
    context.comparisons.borrow_mut().current = None;
    result
}
