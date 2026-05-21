use hashbrown::{HashMap, HashSet};
use std::ops::Deref;

use crate::{
    GenericParam, LuaAliasCallKind, LuaMappedType, LuaMemberKey, LuaObjectType, LuaTupleStatus,
    LuaTupleType, LuaType, TypeOps, VariadicType,
};

use super::{
    GenericInstantiateContext, GenericInstantiateFrame, instantiate_special_generic,
    instantiate_type_generic_inner, key_type_to_member_key,
};
use crate::semantic::generic::TypeMapper;

pub(super) fn instantiate_mapped_type(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    mapped: &LuaMappedType,
) -> LuaType {
    let Some(frame) = frame.enter() else {
        return instantiate_mapped_residual(context, frame, mapped);
    };

    let Some(constraint) = mapped.param.1.type_constraint.as_ref() else {
        return instantiate_mapped_residual(context, frame, mapped);
    };

    let Some(key_domain) = resolve_mapped_key_domain(context, frame, constraint) else {
        return instantiate_mapped_residual(context, frame, mapped);
    };

    let empty_object =
        || LuaType::Object(LuaObjectType::new_with_fields(HashMap::new(), Vec::new()).into());

    if key_domain.keys.is_empty() {
        return empty_object();
    }

    let key_count = key_domain.keys.len();
    let mut visited = HashSet::with_capacity(key_count);
    let mut field_indices: HashMap<LuaMemberKey, usize> = HashMap::with_capacity(key_count);
    let mut fields: Vec<(LuaMemberKey, LuaType)> = Vec::with_capacity(key_count);
    let mut index_access: Vec<(LuaType, LuaType)> = Vec::with_capacity(key_count);
    for key_ty in key_domain.keys {
        if !visited.insert(key_ty.clone()) {
            continue;
        }

        let local_mapper =
            TypeMapper::prepend(mapped.param.0, key_ty.clone(), Some(context.mapper.clone()));
        let local_context = context.with_mapper(local_mapper);
        let mut value_ty = instantiate_type_generic_inner(&local_context, frame, &mapped.value);
        if mapped.is_optional {
            value_ty = TypeOps::Union.apply(context.db, &value_ty, &LuaType::Nil);
        }

        if let Some(member_key) = key_type_to_member_key(&key_ty) {
            if let Some(index) = field_indices.get(&member_key).copied() {
                let (_, existing) = &mut fields[index];
                let merged = LuaType::from_vec(vec![existing.clone(), value_ty]);
                *existing = merged;
            } else {
                field_indices.insert(member_key.clone(), fields.len());
                fields.push((member_key, value_ty));
            }
        } else {
            index_access.push((key_ty, value_ty));
        }
    }

    if fields.is_empty() && index_access.is_empty() {
        return empty_object();
    }

    if key_domain.tuple_like
        && index_access.is_empty()
        && let Some(types) = mapped_tuple_types(&fields)
    {
        return LuaType::Tuple(LuaTupleType::new(types, LuaTupleStatus::InferResolve).into());
    }

    let field_map: HashMap<LuaMemberKey, LuaType> = fields.into_iter().collect();
    LuaType::Object(LuaObjectType::new_with_fields(field_map, index_access).into())
}

struct MappedKeyDomain {
    keys: Vec<LuaType>,
    tuple_like: bool,
}

fn resolve_mapped_key_domain(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    constraint: &LuaType,
) -> Option<MappedKeyDomain> {
    if let LuaType::Call(alias_call) = constraint
        && alias_call.get_call_kind() == LuaAliasCallKind::KeyOf
        && alias_call.get_operands().len() == 1
    {
        let source = instantiate_type_generic_inner(context, frame, &alias_call.get_operands()[0]);
        let keys = instantiate_special_generic::get_keyof_type(context.db, &source)?;
        let mut atoms = Vec::new();
        if !collect_mapped_key_atoms(&keys, &mut atoms) {
            return None;
        }
        return Some(MappedKeyDomain {
            keys: atoms,
            tuple_like: source.is_tuple() || matches!(source, LuaType::Variadic(_)),
        });
    }

    let instantiated = instantiate_type_generic_inner(context, frame, constraint);
    match &instantiated {
        LuaType::Call(alias_call)
            if alias_call.get_call_kind() == LuaAliasCallKind::KeyOf
                && alias_call.get_operands().len() == 1 =>
        {
            let source = &alias_call.get_operands()[0];
            let keys = instantiate_special_generic::get_keyof_type(context.db, source)?;
            let mut atoms = Vec::new();
            if !collect_mapped_key_atoms(&keys, &mut atoms) {
                return None;
            }
            Some(MappedKeyDomain {
                keys: atoms,
                tuple_like: source.is_tuple() || matches!(source, LuaType::Variadic(_)),
            })
        }
        _ => {
            let mut atoms = Vec::new();
            if !collect_mapped_key_atoms(&instantiated, &mut atoms) {
                return None;
            }
            Some(MappedKeyDomain {
                tuple_like: instantiated.is_tuple(),
                keys: atoms,
            })
        }
    }
}

fn instantiate_mapped_residual(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    mapped: &LuaMappedType,
) -> LuaType {
    let param = (
        mapped.param.0,
        GenericParam::new(
            mapped.param.1.name.clone(),
            mapped
                .param
                .1
                .type_constraint
                .as_ref()
                .map(|ty| instantiate_type_generic_inner(context, frame, ty)),
            mapped
                .param
                .1
                .default_type
                .as_ref()
                .map(|ty| instantiate_type_generic_inner(context, frame, ty)),
            mapped.param.1.attributes.clone(),
        ),
    );

    LuaType::Mapped(
        LuaMappedType::new(
            param,
            instantiate_type_generic_inner(context, frame, &mapped.value),
            mapped.is_readonly,
            mapped.is_optional,
        )
        .into(),
    )
}

fn mapped_tuple_types(fields: &[(LuaMemberKey, LuaType)]) -> Option<Vec<LuaType>> {
    let mut indexed = fields
        .iter()
        .filter_map(|(key, ty)| match key {
            LuaMemberKey::Integer(i) => Some((*i, ty.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();

    if indexed.len() != fields.len() {
        return None;
    }

    indexed.sort_by_key(|(index, _)| *index);
    let starts_at_zero = indexed.first().is_some_and(|(index, _)| *index == 0);
    let expected_start = if starts_at_zero { 0 } else { 1 };
    for (offset, (index, _)) in indexed.iter().enumerate() {
        if *index != expected_start + offset as i64 {
            return None;
        }
    }

    Some(indexed.into_iter().map(|(_, ty)| ty).collect())
}

fn collect_mapped_key_atoms(key_ty: &LuaType, acc: &mut Vec<LuaType>) -> bool {
    match key_ty {
        LuaType::Union(union) => {
            for member in union.into_vec() {
                if !collect_mapped_key_atoms(&member, acc) {
                    return false;
                }
            }
            true
        }
        LuaType::MultiLineUnion(multi) => {
            for (member, _) in multi.get_unions() {
                if !collect_mapped_key_atoms(member, acc) {
                    return false;
                }
            }
            true
        }
        LuaType::Variadic(variadic) => match variadic.deref() {
            VariadicType::Base(base) => collect_mapped_key_atoms(base, acc),
            VariadicType::Multi(types) => {
                for member in types {
                    if !collect_mapped_key_atoms(member, acc) {
                        return false;
                    }
                }
                true
            }
        },
        LuaType::Tuple(tuple) => {
            for member in tuple.get_types() {
                if !collect_mapped_key_atoms(member, acc) {
                    return false;
                }
            }
            true
        }
        LuaType::Never => true,
        LuaType::Unknown | LuaType::Call(_) | LuaType::Mapped(_) => false,
        _ => {
            acc.push(key_ty.clone());
            true
        }
    }
}
