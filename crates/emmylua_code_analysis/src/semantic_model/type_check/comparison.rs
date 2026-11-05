//! Evaluate recursive comparisons with a work queue.
//!
//! A structural cycle can provisionally match. Retry its dependents whenever
//! a result changes, and settle the dependencies that support the final result.

use hashbrown::hash_map::EntryRef;
use hashbrown::{HashMap, HashSet};
use std::{collections::VecDeque, rc::Rc};

use super::{TypeCheckContext, TypeCheckFailReason, TypeCheckResult};
use crate::semantic_model::SemanticModel;
use crate::{
    GenericTplId, LuaGenericType, LuaMemberKey, LuaType, LuaTypeDeclId, LuaTypeNode, TypeDef,
    TypeDefKind,
};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum Target {
    Type(LuaType),
    // Both presence and absence need complete proofs before they can support
    // an intersection field check.
    FieldPresent(LuaMemberKey),
    FieldAbsent(LuaMemberKey),
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Comparison {
    source: LuaType,
    target: Target,
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
    /// Cached parameter-use flags; None keeps all arguments unchanged.
    /// Missing entries have not completed analysis.
    alias_parameters: HashMap<LuaTypeDeclId, Option<Vec<bool>>>,
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
        // A cycle cannot prove whether a field exists.
        if !matches!(self.nodes[key].key.target, Target::Type(_)) {
            return false;
        }
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
            let guarded = guarded
                || (is_structure(&comparison.source)
                    && matches!(&comparison.target, Target::Type(ty) if is_structure(ty)));
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
    check_target(context, source, Target::Type(target.clone()))
}

pub(super) fn check_target(
    context: &mut TypeCheckContext,
    source: &LuaType,
    target: Target,
) -> TypeCheckResult {
    let key = Comparison {
        source: source.clone(),
        target,
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
        if !evaluation.visit(&key.source)
            || matches!(&key.target, Target::Type(target) if !evaluation.visit(target))
        {
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
    let result = match &key.target {
        Target::Type(target) => super::check_comparison(&mut current, &key.source, target),
        Target::FieldPresent(field) => {
            super::check_field_presence(&mut current, &key.source, field, true)
        }
        Target::FieldAbsent(field) => {
            super::check_field_presence(&mut current, &key.source, field, false)
        }
    };
    context.comparisons.borrow_mut().current = None;
    result
}

/// Replace unused recursive arguments only for structural comparison. The
/// original alias remains available to member queries and conditional inference.
pub(super) fn alias_for_comparison(context: &TypeCheckContext, ty: &LuaType) -> Option<LuaType> {
    let LuaType::Generic(generic) = ty else {
        return None;
    };
    let id = context
        .require_complete(|| forwarding_alias_target(context.model, generic))
        .ok()?
        .unwrap_or_else(|| generic.get_base_type_id());
    let used = used_alias_parameters(context, &id)?;
    if generic.get_params().len() != used.len() {
        return None;
    }
    Some(LuaType::Generic(Rc::new(LuaGenericType::new(
        id,
        generic
            .get_params()
            .iter()
            .zip(used)
            .map(|(argument, used)| {
                if used {
                    argument.clone()
                } else {
                    LuaType::Unknown
                }
            })
            .collect(),
    ))))
}

/// Follow one alias whose declaration passes every argument through unchanged.
/// Computations, omitted arguments, and parameter permutations stay exact.
fn forwarding_alias_target(
    model: &SemanticModel,
    generic: &LuaGenericType,
) -> Option<LuaTypeDeclId> {
    let def = model.type_def_of(&generic.get_base_type_id())?;
    if def.kind != TypeDefKind::Alias || def.generic_params.len() != generic.get_params().len() {
        return None;
    }
    let LuaType::Generic(target) = model.alias_target(&def)? else {
        return None;
    };
    (target.get_params().len() == def.generic_params.len()
        && target.get_params().iter().enumerate().all(|(index, ty)| {
            matches!(ty, LuaType::TplRef(tpl)
                if tpl.get_tpl_id() == GenericTplId::Type(index as u32))
        }))
    .then(|| target.get_base_type_id())
}

fn used_alias_parameters(context: &TypeCheckContext, id: &LuaTypeDeclId) -> Option<Vec<bool>> {
    let cached = context
        .comparisons
        .borrow()
        .alias_parameters
        .get(id)
        .cloned();
    match cached {
        Some(used) => used,
        None => {
            // Projection can run semantic queries. Do not hold a graph borrow
            // or cache an unfinished result while computing the dependencies.
            let used = context
                .require_complete(|| {
                    let def = context.model.type_def_of(id)?;
                    if def.kind != TypeDefKind::Alias || def.generic_params.is_empty() {
                        return None;
                    }
                    let target = context.model.alias_target(&def)?;
                    alias_parameter_usage(context.model, &def, &target)
                })
                .ok()?;
            context
                .comparisons
                .borrow_mut()
                .alias_parameters
                .insert(id.clone(), used.clone());
            used
        }
    }
}

/// Find parameters that can affect data, including dependencies through recursive
/// argument permutations. The comparison graph checks whether a cycle is guarded.
fn alias_parameter_usage(
    model: &SemanticModel,
    def: &TypeDef,
    target: &LuaType,
) -> Option<Vec<bool>> {
    let id = def.type_decl_id();
    let table_id = LuaTypeDeclId::global("table");
    let mut used = vec![false; def.generic_params.len()];
    // Flags only change from false to true. Each pass either marks another
    // parameter as used or returns.
    loop {
        let previous = used.clone();
        let mut pending = vec![(target, false)];
        let mut recursive = false;
        while let Some((ty, inspected)) = pending.pop() {
            if let LuaType::Generic(generic) = ty
                && (generic.get_base_type_id() == id
                    || forwarding_alias_target(model, generic).is_some_and(|target| target == id))
            {
                // Computations can inspect arguments directly. Incomplete lists
                // also keep their original default-argument behavior.
                if inspected || generic.get_params().len() != used.len() {
                    return None;
                }
                recursive = true;
                for (argument, used) in generic.get_params().iter().zip(&used) {
                    if *used {
                        pending.push((argument, inspected));
                    }
                }
                continue;
            }
            let name = match ty {
                LuaType::TplRef(tpl) => Some(tpl.get_name()),
                LuaType::StrTplRef(tpl) => Some(tpl.get_name()),
                LuaType::Ref(id) | LuaType::Def(id) => Some(id.get_name()),
                _ => None,
            };
            if let Some(index) = name.and_then(|name| {
                def.generic_params
                    .iter()
                    .position(|param| param.name == name)
            }) {
                used[index] = true;
            }
            let inspected = inspected
                || matches!(
                    ty,
                    LuaType::Conditional(_) | LuaType::Call(_) | LuaType::Mapped(_)
                )
                || matches!(ty, LuaType::Generic(generic)
                    if generic.get_base_type_id() != table_id
                        || model.type_def_of(&generic.get_base_type_id())
                            .is_some_and(|def| !model.is_std_file(def.file_id)));
            let mut children = Vec::new();
            ty.push_direct_children(&mut children);
            // Function constraints and defaults can use alias parameters too.
            if let LuaType::DocFunction(function) = ty {
                for param in function.get_generic_params() {
                    children.extend(param.get_constraint());
                    children.extend(param.get_default_type());
                }
            }
            pending.extend(children.into_iter().map(|child| (child, inspected)));
        }
        if !recursive || used.iter().all(|used| *used) {
            return None;
        }
        if used == previous {
            return Some(used);
        }
    }
}
