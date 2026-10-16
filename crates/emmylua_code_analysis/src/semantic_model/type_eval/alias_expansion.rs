//! Alias instances form a finite graph. Declaration checks reject argument
//! growth on recursive cycles; active instances retain recursive references.

use super::*;

/// Resolve only outer aliases, leaving fields and array elements intact.
pub(crate) fn expand_alias_head(model: &SemanticModel, ty: &LuaType) -> Option<LuaType> {
    use crate::semantic_model::cache::{CacheEntry, QueryCacheScope};

    {
        let cache = model.cache.borrow();
        if let Some(entry) = cache.alias_heads.get(ty) {
            return entry.get(&cache).cloned().flatten();
        }
    }
    let _query = QueryCacheScope::new(&model.cache);
    let revision = model.cache.borrow().revision();
    model
        .cache
        .borrow_mut()
        .alias_heads
        .insert(ty.clone(), CacheEntry::InProgress);
    let result = expand(model, ty, ExpansionMode::Head);
    let mut cache = model.cache.borrow_mut();
    if result.is_none() {
        cache.mark_incomplete();
    }
    let complete = cache.is_complete(revision);
    cache
        .alias_heads
        .insert(ty.clone(), CacheEntry::finish(result.clone(), complete));
    result
}

/// Keep the source alias active while a member query consumes its expansion.
/// Recursive edges may be retained in structures, so expansion alone does not
/// protect a consumer that follows those edges after the expansion returns.
pub(crate) fn with_expanded_alias<T>(
    model: &SemanticModel,
    ty: &LuaType,
    consume: impl FnOnce(&LuaType) -> T,
) -> Option<T> {
    let (id, mode) = match ty {
        LuaType::Ref(id) | LuaType::Def(id) => (id.clone(), ExpansionMode::Head),
        LuaType::Generic(generic) => (generic.get_base_type_id(), ExpansionMode::Structure),
        _ => return None,
    };
    model.type_def_of(&id)?;
    let Some(expanded) = expand(model, ty, mode) else {
        model.cache.borrow().mark_incomplete();
        return None;
    };
    let mut path = AliasPath::new(model);
    if path.enter(ty).is_none() {
        model.cache.borrow().mark_incomplete();
        return None;
    }
    Some(consume(&expanded))
}

/// Track active alias instances and restore the shared path when this guard ends.
pub(super) struct AliasPath<'a, 'db> {
    model: &'a SemanticModel<'db>,
    // Share active instances with public queries called during expansion.
    path: Vec<LuaType>,
}

impl<'a, 'db> AliasPath<'a, 'db> {
    pub(super) fn new(model: &'a SemanticModel<'db>) -> Self {
        Self {
            model,
            path: Vec::new(),
        }
    }

    pub(super) fn enter(&mut self, ty: &LuaType) -> Option<()> {
        if !self
            .model
            .cache
            .borrow_mut()
            .expanding_aliases
            .insert(ty.clone())
        {
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ExpansionMode {
    /// Follow outer aliases, leaving fields and array elements intact.
    Head,
    /// Expand generic aliases in structures, keeping named references lazy.
    Structure,
    /// Follow outer aliases, then expand generic aliases in the resulting structure.
    HeadAndStructure,
}

enum Task {
    Enter(LuaType, ExpansionMode),
    Alias(LuaType, ExpansionMode),
    Leave,
    Rebuild {
        original: LuaType,
        child_count: usize,
        mode: ExpansionMode,
    },
    Builtin(Rc<LuaGenericType>, ExpansionMode),
    Conditional(
        Rc<LuaConditionalType>,
        unify::TplBindings,
        HashMap<String, LuaType>,
        ExpansionMode,
    ),
}

pub(super) fn expand(model: &SemanticModel, ty: &LuaType, mode: ExpansionMode) -> Option<LuaType> {
    // Declaration checks ensure a finite set of instances. The work list
    // follows each active instance once and retains recursive child edges.
    let mut path = AliasPath::new(model);
    let mut pending = vec![Task::Enter(ty.clone(), mode)];
    let mut values: Vec<Option<LuaType>> = Vec::new();
    while let Some(task) = pending.pop() {
        match task {
            Task::Leave => path.leave(),
            Task::Rebuild {
                original,
                child_count,
                mode,
            } => {
                let children = values.split_off(values.len() - child_count);
                // Keep unresolved children so other fields remain usable.
                let mut children = children.into_iter();
                let rebuilt = map_alias_children(&original, &mut |child| {
                    children
                        .next()
                        .expect("each alias child must have a result")
                        .unwrap_or_else(|| child.clone())
                });
                let evaluated = evaluate_rebuilt(model, rebuilt.clone());
                if evaluated != rebuilt {
                    pending.push(Task::Enter(evaluated, mode));
                } else {
                    values.push(Some(rebuilt));
                }
            }
            Task::Builtin(generic, mode) => {
                let arguments = values.split_off(values.len() - generic.get_params().len());
                let Some(arguments) = arguments.into_iter().collect::<Option<Vec<_>>>() else {
                    values.push(None);
                    continue;
                };
                let rebuilt = LuaType::Generic(Rc::new(LuaGenericType::new(
                    generic.get_base_type_id(),
                    arguments,
                )));
                if path.enter(&rebuilt).is_none() {
                    values.push(None);
                    continue;
                }
                if let Some(result) = evaluate_builtin(model, &rebuilt) {
                    pending.push(Task::Leave);
                    pending.push(Task::Enter(result, mode));
                } else {
                    path.leave();
                    pending.push(Task::Alias(rebuilt, mode));
                }
            }
            Task::Conditional(conditional, bindings, names, mode) => {
                assert!(
                    !values.is_empty(),
                    "conditional must have an argument result"
                );
                let Some(bound) = values.pop().flatten() else {
                    // An unresolved argument cannot select a conditional
                    // branch. Propagate failure to its containing structure.
                    values.push(None);
                    continue;
                };
                let result =
                    eval_conditional_alias(model, &conditional, &bindings, &names, Some(bound));
                pending.push(Task::Enter(result, mode));
            }
            Task::Enter(current, mode) => {
                if mode != ExpansionMode::Head
                    && let LuaType::Generic(generic) = &current
                    && matches!(
                        generic.get_base_type_id().get_name(),
                        "std.RawGet" | "RawGet" | "Merge"
                    )
                {
                    pending.push(Task::Builtin(generic.clone(), mode));
                    for argument in generic.get_params().iter().rev() {
                        pending.push(Task::Enter(argument.clone(), ExpansionMode::Structure));
                    }
                } else {
                    pending.push(Task::Alias(current, mode));
                }
            }
            Task::Alias(current, mode) => {
                let id = match &current {
                    LuaType::Ref(id) | LuaType::Def(id) if mode != ExpansionMode::Structure => {
                        Some(id.clone())
                    }
                    LuaType::Generic(generic) => Some(generic.get_base_type_id()),
                    _ => None,
                };
                let def = id
                    .and_then(|id| model.type_def_of(&id))
                    .filter(|def| def.kind == TypeDefKind::Alias);
                if let Some(def) = def {
                    if !alias_recursion::is_finite(model, &def)? || path.enter(&current).is_none() {
                        // Head expansion fails without publishing argument
                        // growth. A structural parent recovers the original
                        // child when it rebuilds, preserving other fields.
                        values.push(None);
                        continue;
                    }
                    let Some(target) = model.alias_target(&def) else {
                        path.leave();
                        if mode != ExpansionMode::Head {
                            values.push(Some(current));
                            continue;
                        }
                        values.push(None);
                        continue;
                    };
                    if let LuaType::Generic(generic) = &current
                        && matches!(&target, LuaType::Conditional(_))
                        && generic
                            .get_params()
                            .iter()
                            .any(contains_unbound_function_tpl)
                    {
                        // Leave an unbound conditional undecided,
                        // while still resolving independent arguments.
                        path.leave();
                        push_rebuild(&mut pending, current, mode);
                        continue;
                    }
                    pending.push(Task::Leave);
                    let mode = match mode {
                        ExpansionMode::Head => ExpansionMode::Head,
                        ExpansionMode::Structure | ExpansionMode::HeadAndStructure => {
                            ExpansionMode::HeadAndStructure
                        }
                    };
                    let result = if let LuaType::Generic(generic) = &current {
                        let bindings: unify::TplBindings = generic
                            .get_params()
                            .iter()
                            .enumerate()
                            .map(|(index, ty)| (GenericTplId::Type(index as u32), ty.clone()))
                            .collect();
                        let names: HashMap<String, LuaType> = def
                            .generic_params
                            .iter()
                            .zip(generic.get_params())
                            .map(|(param, ty)| (param.name.to_string(), ty.clone()))
                            .collect();
                        if let LuaType::Conditional(conditional) = &target {
                            let bound = match conditional.get_checked_type() {
                                LuaType::TplRef(tpl) => bindings.get(&tpl.get_tpl_id()),
                                LuaType::Ref(id) | LuaType::Def(id) => names.get(id.get_name()),
                                _ => None,
                            };
                            if let Some(bound) = bound.cloned() {
                                pending.push(Task::Conditional(
                                    conditional.clone(),
                                    bindings,
                                    names,
                                    mode,
                                ));
                                pending.push(Task::Enter(bound, ExpansionMode::HeadAndStructure));
                                continue;
                            }
                            eval_conditional_alias(model, conditional, &bindings, &names, None)
                        } else {
                            unify::substitute(&substitute_named_refs(&target, &names), &bindings)
                        }
                    } else {
                        target
                    };
                    pending.push(Task::Enter(result, mode));
                } else if mode != ExpansionMode::Head {
                    if let LuaType::Mapped(mapped) = &current
                        && let Some(expanded) = expand_mapped(model, mapped)
                        && expanded != current
                    {
                        pending.push(Task::Enter(expanded, mode));
                        continue;
                    }
                    push_rebuild(&mut pending, current, mode);
                } else {
                    values.push(Some(current));
                }
            }
        }
    }
    assert_eq!(values.len(), 1, "alias expansion must produce one result");
    values.remove(0)
}

/// Schedule expansion of `ty`'s children, followed by rebuilding `ty`.
///
/// Push children in reverse order so the stack processes them in the order
/// expected by `map_alias_children`.
fn push_rebuild(pending: &mut Vec<Task>, ty: LuaType, mode: ExpansionMode) {
    let mut children = Vec::new();
    map_alias_children(&ty, &mut |child| {
        children.push(child.clone());
        child.clone()
    });
    pending.push(Task::Rebuild {
        original: ty,
        child_count: children.len(),
        mode,
    });
    for child in children.into_iter().rev() {
        // Named references inside structures retain main's lazy projection.
        // Generic alias calls are expanded.
        pending.push(Task::Enter(child, ExpansionMode::Structure));
    }
}

fn evaluate_rebuilt(model: &SemanticModel, ty: LuaType) -> LuaType {
    if matches!(ty, LuaType::Conditional(_)) {
        return eval_conditionals(model, &ty);
    }
    evaluate_builtin(model, &ty).unwrap_or(ty)
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
