//! Resolve computations to their outer constructor; data fields remain lazy.

use super::*;

/// Resolve outer type computations, leaving fields and array elements intact.
pub(crate) fn expand_alias_head(model: &SemanticModel, ty: &LuaType) -> Option<LuaType> {
    use crate::semantic_model::cache::{CacheEntry, QueryCacheScope};

    if !needs_reduction(model, ty) {
        return Some(ty.clone());
    }

    let key = (model.file_id(), ty.clone());
    {
        let cache = model.cache.borrow();
        if let Some(entry) = cache.alias_heads.get(&key) {
            return entry.get(&cache).cloned().flatten();
        }
    }
    let _query = QueryCacheScope::new(&model.cache);
    let revision = model.cache.borrow().revision();
    model
        .cache
        .borrow_mut()
        .alias_heads
        .insert(key.clone(), CacheEntry::InProgress);
    let result = expand(model, ty).filter(|result| !needs_reduction(model, result));
    let mut cache = model.cache.borrow_mut();
    if result.is_none() {
        cache.mark_incomplete();
    }
    let complete = cache.is_complete(revision);
    cache
        .alias_heads
        .insert(key, CacheEntry::finish(result.clone(), complete));
    result
}

/// Keep an alias active while a member query consumes its outer structure.
pub(crate) fn with_expanded_alias<T>(
    model: &SemanticModel,
    ty: &LuaType,
    consume: impl FnOnce(&LuaType) -> T,
) -> Option<T> {
    let expanded = expand_alias_head(model, ty)?;
    let mut path = AliasPath::new(model)?;
    path.enter(ty)?;
    Some(consume(&expanded))
}

/// Keep exact alias instances active across nested semantic queries.
pub(super) struct AliasPath<'a, 'db> {
    model: &'a SemanticModel<'db>,
    path: Vec<LuaType>,
    evaluation: crate::semantic_model::cache::TypeEvaluationScope<'a>,
}

impl<'a, 'db> AliasPath<'a, 'db> {
    pub(super) fn new(model: &'a SemanticModel<'db>) -> Option<Self> {
        Some(Self {
            model,
            path: Vec::new(),
            evaluation: crate::semantic_model::cache::TypeEvaluationScope::new(&model.cache)?,
        })
    }

    pub(super) fn enter(&mut self, ty: &LuaType) -> Option<()> {
        if !self.evaluation.visit(ty) {
            return None;
        }
        let mut cache = self.model.cache.borrow_mut();
        if !cache.expanding_aliases.insert(ty.clone()) {
            cache.mark_incomplete();
            return None;
        }
        self.path.push(ty.clone());
        Some(())
    }

    pub(super) fn leave(&mut self) {
        let ty = self.path.pop().expect("alias leave must match an entry");
        self.model.cache.borrow_mut().expanding_aliases.remove(&ty);
    }
}

impl Drop for AliasPath<'_, '_> {
    fn drop(&mut self) {
        while !self.path.is_empty() {
            self.leave();
        }
    }
}

enum Task {
    Enter(LuaType),
    Conditional(
        LuaType,
        Rc<LuaConditionalType>,
        unify::TplBindings,
        HashMap<String, LuaType>,
    ),
    Leave,
}

/// Union and intersection branches affect the outer shape. Resolve their
/// aliases too, but stop at data constructors so recursive fields stay lazy.
pub(crate) fn expand_alias_branches(model: &SemanticModel, ty: &LuaType) -> Option<LuaType> {
    enum Task {
        Enter(LuaType),
        Rebuild(LuaType, usize),
        Leave,
    }
    let mut path = AliasPath::new(model)?;
    let mut pending = vec![Task::Enter(ty.clone())];
    let mut values = Vec::new();
    while let Some(task) = pending.pop() {
        // Do not rebuild an unbounded union after the shared budget is spent.
        if model.cache.borrow().evaluation_exhausted {
            return None;
        }
        match task {
            Task::Leave => path.leave(),
            Task::Rebuild(ty, count) => {
                let children = values.split_off(values.len() - count);
                values.push(match ty {
                    LuaType::Union(_) => LuaType::from_vec(children),
                    LuaType::Intersection(_) => {
                        let children = children
                            .into_iter()
                            .flat_map(|child| match child {
                                LuaType::Intersection(intersection) => {
                                    intersection.get_types().to_vec()
                                }
                                child => vec![child],
                            })
                            .collect();
                        LuaType::Intersection(Rc::new(LuaIntersectionType::new(children)))
                    }
                    _ => unreachable!("only unions and intersections have branches"),
                });
            }
            Task::Enter(current) => {
                let expanded = if needs_reduction(model, &current) {
                    let Some(expanded) = expand(model, &current) else {
                        values.push(current);
                        continue;
                    };
                    if matches!(expanded, LuaType::Union(_) | LuaType::Intersection(_)) {
                        if path.enter(&current).is_none() {
                            values.push(current);
                            continue;
                        }
                        pending.push(Task::Leave);
                    }
                    expanded
                } else {
                    current
                };
                if matches!(expanded, LuaType::Union(_) | LuaType::Intersection(_)) {
                    let mut children = Vec::new();
                    map_alias_children(&expanded, &mut |child| {
                        children.push(child.clone());
                        child.clone()
                    });
                    pending.push(Task::Rebuild(expanded, children.len()));
                    pending.extend(children.into_iter().rev().map(Task::Enter));
                } else {
                    values.push(expanded);
                }
            }
        }
    }
    values.pop()
}

pub(super) fn expand(model: &SemanticModel, ty: &LuaType) -> Option<LuaType> {
    let mut path = AliasPath::new(model)?;
    let mut pending = vec![Task::Enter(ty.clone())];
    let mut value = None;
    while let Some(task) = pending.pop() {
        if let Task::Enter(ty) = &task
            && !path.evaluation.visit(ty)
        {
            return None;
        }
        match task {
            Task::Leave => path.leave(),
            Task::Conditional(current, conditional, bindings, names) => {
                let bound = value.take()?;
                // Predicate queries and following the selected branch are separate
                // reductions. A branch that returns a constructor is already done.
                let result = eval_conditional_alias(model, &conditional, &bindings, &names, bound);
                path.leave();
                if needs_reduction(model, &result) {
                    path.enter(&current)?;
                    pending.push(Task::Leave);
                    pending.push(Task::Enter(result));
                } else {
                    value = Some(result);
                }
            }
            Task::Enter(current) => {
                if !needs_reduction(model, &current) {
                    value = Some(current);
                    continue;
                }
                let id = match &current {
                    LuaType::Ref(id) | LuaType::Def(id) => Some(id.clone()),
                    LuaType::Generic(generic) => Some(generic.get_base_type_id()),
                    _ => None,
                };
                let def = id
                    .filter(|id| !matches!(id.get_name(), "RawGet" | "std.RawGet" | "Merge"))
                    .and_then(|id| model.type_def_of(&id))
                    .filter(|def| def.kind == TypeDefKind::Alias);
                if let Some(def) = def {
                    let target = model.alias_target(&def)?;
                    let (bindings, names) = match &current {
                        LuaType::Generic(generic) => (
                            generic
                                .get_params()
                                .iter()
                                .enumerate()
                                .map(|(i, ty)| (GenericTplId::Type(i as u32), ty.clone()))
                                .collect(),
                            def.generic_params
                                .iter()
                                .zip(generic.get_params())
                                .map(|(param, ty)| (param.name.to_string(), ty.clone()))
                                .collect(),
                        ),
                        _ => (HashMap::new(), HashMap::new()),
                    };
                    if let LuaType::Conditional(conditional) = &target {
                        if contains_unbound_function_tpl(&current) {
                            value = Some(current);
                            continue;
                        }
                        let bound = unify::substitute(conditional.get_checked_type(), &bindings);
                        let bound = substitute_named_refs(&bound, &names);
                        path.enter(&current)?;
                        pending.push(Task::Conditional(
                            current,
                            conditional.clone(),
                            bindings,
                            names,
                        ));
                        pending.push(Task::Enter(bound));
                        continue;
                    }
                    let result =
                        unify::substitute(&substitute_named_refs(&target, &names), &bindings);
                    if needs_reduction(model, &result) {
                        path.enter(&current)?;
                        pending.push(Task::Leave);
                        pending.push(Task::Enter(result));
                    } else {
                        value = Some(result);
                    }
                } else {
                    path.enter(&current)?;
                    let result = match &current {
                        LuaType::Conditional(_) => Some(eval_conditionals(model, &current)),
                        LuaType::Mapped(mapped) => expand_mapped(model, mapped),
                        LuaType::Call(call) if call.get_call_kind() == LuaAliasCallKind::Index => {
                            expand_index_call(model, call)
                        }
                        _ => evaluate_builtin(model, &current),
                    };
                    if let Some(result) = result.filter(|result| *result != current) {
                        pending.push(Task::Leave);
                        pending.push(Task::Enter(result));
                    } else {
                        path.leave();
                        value = Some(current);
                    }
                }
            }
        }
    }
    value
}

pub(super) fn needs_reduction(model: &SemanticModel, ty: &LuaType) -> bool {
    let id = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => id.clone(),
        LuaType::Generic(generic) => {
            let id = generic.get_base_type_id();
            if matches!(id.get_name(), "RawGet" | "std.RawGet" | "Merge") {
                return true;
            }
            id
        }
        LuaType::Call(_) | LuaType::Conditional(_) | LuaType::Mapped(_) => return true,
        _ => return false,
    };
    model
        .type_def_of(&id)
        .is_some_and(|def| def.kind == TypeDefKind::Alias)
}

fn evaluate_builtin(model: &SemanticModel, ty: &LuaType) -> Option<LuaType> {
    match ty {
        LuaType::Generic(generic) => {
            let id = generic.get_base_type_id();
            let params = generic.get_params();
            if matches!(id.get_name(), "std.RawGet" | "RawGet") && params.len() == 2 {
                return eval_index_access(model, &params[0], &params[1]);
            }
            if id.get_name() == "Merge" && params.len() >= 2 {
                return eval_merge_call(
                    model,
                    &LuaAliasCallType::new(LuaAliasCallKind::Merge, params.to_vec()),
                );
            }
        }
        LuaType::Call(call) if call.get_call_kind() == LuaAliasCallKind::Merge => {
            return eval_merge_call(model, call);
        }
        _ => {}
    }
    None
}
