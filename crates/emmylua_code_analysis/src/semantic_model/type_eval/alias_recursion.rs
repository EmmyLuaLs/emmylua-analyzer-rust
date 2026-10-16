//! Reject recursive argument growth in aliases and indexed member types.
//!
//! Parameter forwarding and fixed replacements cannot create new argument
//! structure indefinitely. Cycles that can add argument structure without end
//! are unsupported. Alias calls and member projections share this analysis.

use super::*;
use crate::semantic_model::cache::CacheEntry;
use crate::semantic_model::type_cycles::strongly_connected_components;
use crate::{LuaTypeNode, SemanticId};
use hashbrown::HashSet;

type Parameter = (SemanticId, usize);

enum Definition {
    Alias(TypeDef),
    Member(SemanticId),
}

impl Definition {
    fn id(&self) -> &SemanticId {
        match self {
            Self::Alias(def) => &def.id,
            Self::Member(id) => id,
        }
    }

    fn project(&self, model: &SemanticModel) -> Option<(Vec<SmolStr>, LuaType)> {
        match self {
            Self::Alias(def) => Some((
                def.generic_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect(),
                model.alias_target(def)?,
            )),
            Self::Member(id @ SemanticId::Member(member)) => Some((
                model.member_generic_names(member.file_id, id),
                model.type_of_member(id)?,
            )),
            _ => None,
        }
    }
}

pub(crate) fn is_finite(model: &SemanticModel, def: &TypeDef) -> Option<bool> {
    if def.kind != TypeDefKind::Alias {
        return Some(true);
    }
    check(model, Definition::Alias(def.clone()))
}

pub(super) fn member_is_finite(model: &SemanticModel, id: &SemanticId) -> Option<bool> {
    check(model, Definition::Member(id.clone()))
}

fn check(model: &SemanticModel, definition: Definition) -> Option<bool> {
    let root = definition.id().clone();
    {
        let cache = model.cache.borrow();
        if let Some(entry) = cache.alias_recursion.get(&root) {
            return entry.get_complete(&cache).copied();
        }
    }
    let _query = crate::semantic_model::cache::QueryCacheScope::new(&model.cache);
    let revision = model.cache.borrow().revision();
    model
        .cache
        .borrow_mut()
        .alias_recursion
        .insert(root.clone(), CacheEntry::InProgress);
    let mut definitions = HashSet::new();
    let mut pending = vec![definition];
    let mut edges: HashMap<Parameter, Vec<(Parameter, bool)>> = HashMap::new();
    while let Some(definition) = pending.pop() {
        let id = definition.id().clone();
        if !definitions.insert(id.clone()) {
            continue;
        }
        let Some((params, body)) = definition.project(model) else {
            model.cache.borrow().mark_incomplete();
            continue;
        };
        // Follow the same component selections as collect_infer_assignments.
        // Object and constructor lookups use fixed semantic types; neither
        // lookup instantiates generic owners.
        let mut origins = HashMap::new();
        let mut types = vec![&body];
        while let Some(ty) = types.pop() {
            if let LuaType::Conditional(conditional) = ty {
                let parameter = parameter_index(&params, conditional.get_checked_type());
                let mut patterns = vec![(conditional.get_extends_type(), Some(0))];
                while let Some((pattern, depth)) = patterns.pop() {
                    match pattern {
                        LuaType::TplRef(inferred)
                            if matches!(
                                inferred.get_tpl_id(),
                                GenericTplId::ConditionalInfer(_)
                            ) =>
                        {
                            let origin = parameter.zip(depth);
                            origins
                                .entry(inferred.get_tpl_id())
                                .and_modify(|existing: &mut Option<(usize, usize)>| {
                                    *existing = match (*existing, origin) {
                                        (Some((a, x)), Some((b, y))) if a == b => {
                                            Some((a, x.min(y)))
                                        }
                                        _ => None,
                                    };
                                })
                                .or_insert(origin);
                        }
                        LuaType::Array(_) | LuaType::Tuple(_) | LuaType::Generic(_) => {
                            let mut children = Vec::new();
                            pattern.push_direct_children(&mut children);
                            patterns.extend(
                                children
                                    .into_iter()
                                    .map(|ty| (ty, depth.map(|depth| depth + 1))),
                            );
                        }
                        LuaType::DocFunction(function) => {
                            let params = function.get_params();
                            let fixed = infer_fixed_param_count(function);
                            patterns.extend(params[..fixed].iter().filter_map(|(_, ty)| {
                                ty.as_ref().map(|ty| (ty, depth.map(|depth| depth + 1)))
                            }));
                            let rest = params.get(fixed).and_then(|(_, ty)| ty.as_ref());
                            if let Some(rest) = rest {
                                // Rest inference can replace the function wrapper with
                                // a tuple, so it does not consume a level of structure.
                                patterns.push((rest, depth));
                            }
                            let return_depth = if rest.is_some_and(contains_conditional_infer) {
                                // The matcher finishes after inferring rest parameters.
                                None
                            } else {
                                depth.map(|depth| depth + 1)
                            };
                            patterns.push((function.get_ret(), return_depth));
                        }
                        LuaType::Variadic(variadic)
                            if let VariadicType::Base(base) = variadic.as_ref() =>
                        {
                            // A multi-value source is packed into a tuple here too.
                            patterns.push((base, depth));
                        }
                        LuaType::Object(object) => {
                            patterns.extend(
                                object
                                    .get_fields()
                                    .values()
                                    .map(|ty| (ty, depth.map(|depth| depth + 1))),
                            );
                            for (key, value) in object.get_index_access() {
                                patterns.extend([(key, None), (value, None)]);
                            }
                        }
                        _ => {
                            let mut children = Vec::new();
                            pattern.push_direct_children(&mut children);
                            patterns.extend(children.into_iter().map(|ty| (ty, None)));
                        }
                    }
                }
            }
            ty.push_direct_children(&mut types);
        }
        let mut types = vec![&body];
        while let Some(ty) = types.pop() {
            let target = match ty {
                LuaType::Generic(generic)
                    if matches!(
                        generic.get_base_type_id().get_name(),
                        "std.RawGet" | "RawGet"
                    ) && generic.get_params().len() == 2 =>
                {
                    indexed_definition(model, &generic.get_params()[0], &generic.get_params()[1])
                }
                LuaType::Call(call)
                    if call.get_call_kind() == LuaAliasCallKind::Index
                        && call.get_operands().len() == 2 =>
                {
                    indexed_definition(model, &call.get_operands()[0], &call.get_operands()[1])
                }
                _ => {
                    let target_id = match ty {
                        LuaType::Generic(generic) => Some(generic.get_base_type_id()),
                        LuaType::Ref(id) | LuaType::Def(id)
                            if !params.iter().any(|param| param == id.get_name()) =>
                        {
                            Some(id.clone())
                        }
                        _ => None,
                    };
                    target_id
                        .and_then(|id| model.type_def_of(&id))
                        .filter(|def| def.kind == TypeDefKind::Alias)
                        .map(|def| {
                            let arguments = match ty {
                                LuaType::Generic(generic) => generic.get_params().to_vec(),
                                _ => Vec::new(),
                            };
                            (Definition::Alias(def), arguments)
                        })
                }
            };
            if let Some((target, arguments)) = target {
                for (index, argument) in arguments.iter().enumerate() {
                    let mut arguments = vec![(argument, 0)];
                    while let Some((ty, depth)) = arguments.pop() {
                        let parameter = parameter_index(&params, ty)
                            .map(|index| (index, 0))
                            .or_else(|| match ty {
                                LuaType::TplRef(tpl) => {
                                    origins.get(&tpl.get_tpl_id()).copied().flatten()
                                }
                                _ => None,
                            });
                        if let Some((parameter, consumed)) = parameter {
                            edges
                                .entry((id.clone(), parameter))
                                .or_default()
                                .push(((target.id().clone(), index), depth > consumed));
                        } else if matches!(ty, LuaType::TplRef(_)) {
                            for parameter in 0..params.len() {
                                edges
                                    .entry((id.clone(), parameter))
                                    .or_default()
                                    .push(((target.id().clone(), index), true));
                            }
                        } else {
                            let mut children = Vec::new();
                            ty.push_direct_children(&mut children);
                            arguments.extend(children.into_iter().map(|ty| (ty, depth + 1)));
                        }
                    }
                }
                pending.push(target);
            }
            ty.push_direct_children(&mut types);
        }
    }

    let pairs: Vec<_> = edges
        .iter()
        .flat_map(|(source, outgoing)| {
            outgoing
                .iter()
                .map(|(target, _)| (source.clone(), target.clone()))
        })
        .collect();
    let components = strongly_connected_components(&pairs);
    let mut growing = HashSet::new();
    for (parameter, outgoing) in &edges {
        for (target, grows) in outgoing {
            if *grows && components[parameter] == components[target] {
                growing.insert(components[parameter]);
            }
        }
    }
    let unsupported: HashSet<_> = components
        .iter()
        .filter(|(_, component)| growing.contains(*component))
        .map(|(parameter, _)| &parameter.0)
        .collect();
    let mut cache = model.cache.borrow_mut();
    let complete = cache.is_complete(revision);
    for id in &definitions {
        if id != &root && matches!(cache.alias_recursion.get(id), Some(CacheEntry::InProgress)) {
            // A re-entered declaration check still owns its active entry.
            continue;
        }
        cache.alias_recursion.insert(
            id.clone(),
            CacheEntry::finish(!unsupported.contains(id), complete),
        );
    }
    complete.then(|| !unsupported.contains(&root))
}

fn parameter_index(params: &[SmolStr], ty: &LuaType) -> Option<usize> {
    match ty {
        LuaType::TplRef(tpl) => match tpl.get_tpl_id() {
            GenericTplId::Type(index) if (index as usize) < params.len() => Some(index as usize),
            _ => None,
        },
        LuaType::Ref(id) | LuaType::Def(id) => {
            params.iter().position(|param| param == id.get_name())
        }
        _ => None,
    }
}

// Lookup is a type function too. Keep its declaration and arguments separate,
// just as for an alias, so repeated projections do not grow the analysis graph.
fn indexed_definition(
    model: &SemanticModel,
    base: &LuaType,
    key: &LuaType,
) -> Option<(Definition, Vec<LuaType>)> {
    let base = expand_alias_head(model, base)?;
    let key = expand_alias_head(model, key)?;
    let member = model.member_info(&base, &member_key_from_literal(&key)?)?;
    let id = member.id?;
    let arguments = match base {
        LuaType::Generic(generic) => generic.get_params().to_vec(),
        _ => Vec::new(),
    };
    Some((Definition::Member(id), arguments))
}
