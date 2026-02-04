use std::collections::{HashMap, HashSet};

use crate::{
    DbIndex, InFiled, InferGuardRef, LuaGenericType, LuaIntersectionType, LuaMemberKey,
    LuaMemberOwner, LuaObjectType, LuaOperatorMetaMethod, LuaOperatorOwner, LuaSemanticDeclId,
    LuaType, LuaTypeDeclId, LuaUnionType, TypeOps,
    semantic::{
        InferGuard,
        generic::{TypeSubstitutor, instantiate_type_generic},
    },
};

use super::{FindMembersResult, LuaMemberInfo, intersect_member_types};
use rowan::TextRange;

pub fn find_index_operations(db: &DbIndex, prefix_type: &LuaType) -> FindMembersResult {
    find_index_operations_guard(db, prefix_type, &InferGuard::new())
}

pub fn find_index_operations_guard(
    db: &DbIndex,
    prefix_type: &LuaType,
    infer_guard: &InferGuardRef,
) -> FindMembersResult {
    match &prefix_type {
        LuaType::TableConst(in_filed) => find_index_table(db, in_filed),
        LuaType::Ref(decl_id) => find_index_custom_type(db, decl_id, infer_guard),
        LuaType::Def(decl_id) => find_index_custom_type(db, decl_id, infer_guard),
        LuaType::Array(array_type) => find_index_array(db, array_type.get_base()),
        LuaType::Object(object) => find_index_object(db, object),
        LuaType::Union(union) => find_index_union(db, union, infer_guard),
        LuaType::Intersection(intersection) => {
            find_index_intersection(db, intersection, infer_guard)
        }
        LuaType::Generic(generic) => find_index_generic(db, generic, infer_guard),
        LuaType::TableGeneric(table_generic) => find_index_table_generic(db, table_generic),
        LuaType::Instance(inst) => {
            let base = inst.get_base();
            find_index_operations_guard(db, base, infer_guard)
        }
        LuaType::ModuleRef(file_id) => {
            let module_info = db.get_module_index().get_module(*file_id);
            if let Some(module_info) = module_info
                && let Some(export_type) = &module_info.export_type
            {
                return find_index_operations_guard(db, export_type, infer_guard);
            }

            None
        }
        _ => None,
    }
}

fn find_index_table(db: &DbIndex, table_range: &InFiled<TextRange>) -> FindMembersResult {
    let mut members = Vec::new();

    // Check for metatable __index operators
    let metatable = db.get_metatable_index().get(table_range);
    if let Some(metatable) = metatable {
        let meta_owner = LuaOperatorOwner::Table(metatable.clone());
        if let Some(operator_ids) = db
            .get_operator_index()
            .get_operators(&meta_owner, LuaOperatorMetaMethod::Index)
        {
            for operator_id in operator_ids {
                if let Some(operator) = db.get_operator_index().get_operator(operator_id) {
                    let operand = operator.get_operand(db);
                    if let Ok(return_type) = operator.get_result(db) {
                        members.push(LuaMemberInfo {
                            property_owner_id: None,
                            key: LuaMemberKey::ExprType(operand),
                            typ: return_type,
                            feature: None,
                            overload_index: None,
                        });
                    }
                }
            }
        }
    } else {
        // Check for direct table members
        let member_owner = LuaMemberOwner::Element(table_range.clone());
        if let Some(table_members) = db.get_member_index().get_members(&member_owner) {
            for member in table_members {
                let member_key_type = match member.get_key() {
                    LuaMemberKey::Name(s) => LuaType::StringConst(s.clone().into()),
                    LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
                    _ => continue,
                };

                let member_type = db
                    .get_type_index()
                    .get_type_cache(&member.get_id().into())
                    .map(|it| it.as_type().clone())
                    .unwrap_or(LuaType::Unknown);

                members.push(LuaMemberInfo {
                    property_owner_id: Some(LuaSemanticDeclId::Member(member.get_id())),
                    key: LuaMemberKey::ExprType(member_key_type),
                    typ: member_type,
                    feature: Some(member.get_feature()),
                    overload_index: None,
                });
            }
        }
    }

    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

fn find_index_custom_type(
    db: &DbIndex,
    prefix_type_id: &LuaTypeDeclId,
    infer_guard: &InferGuardRef,
) -> FindMembersResult {
    infer_guard.check(prefix_type_id).ok()?;
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(prefix_type_id)?;

    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return find_index_operations_guard(db, &origin_type, infer_guard);
        }
        return None;
    }

    let mut members = Vec::new();

    // Check for __index operators
    if let Some(index_operator_ids) = db
        .get_operator_index()
        .get_operators(&prefix_type_id.clone().into(), LuaOperatorMetaMethod::Index)
    {
        for operator_id in index_operator_ids {
            if let Some(operator) = db.get_operator_index().get_operator(operator_id) {
                let operand = operator.get_operand(db);
                if let Ok(return_type) = operator.get_result(db) {
                    members.push(LuaMemberInfo {
                        property_owner_id: None,
                        key: LuaMemberKey::ExprType(operand),
                        typ: return_type,
                        feature: None,
                        overload_index: None,
                    });
                }
            }
        }
    }

    // Find index operations in super types
    if type_decl.is_class()
        && let Some(super_types) = type_index.get_super_types(prefix_type_id)
    {
        for super_type in super_types {
            if let Some(super_members) = find_index_operations_guard(db, &super_type, infer_guard) {
                members.extend(super_members);
            }
        }
    }

    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

fn find_index_array(db: &DbIndex, base: &LuaType) -> FindMembersResult {
    let mut members = Vec::new();

    let expression_type = if db.get_emmyrc().strict.array_index {
        TypeOps::Union.apply(db, base, &LuaType::Nil)
    } else {
        base.clone()
    };

    // Array accepts integer indices
    members.push(LuaMemberInfo {
        property_owner_id: None,
        key: LuaMemberKey::ExprType(LuaType::Integer),
        typ: expression_type.clone(),
        feature: None,
        overload_index: None,
    });

    // Array accepts number indices (for compatibility)
    members.push(LuaMemberInfo {
        property_owner_id: None,
        key: LuaMemberKey::ExprType(LuaType::Number),
        typ: expression_type,
        feature: None,
        overload_index: None,
    });

    Some(members)
}

#[allow(unused)]
fn find_index_object(db: &DbIndex, object: &LuaObjectType) -> FindMembersResult {
    let mut members = Vec::new();

    let access_member_type = object.get_index_access();
    for (key, field) in access_member_type {
        members.push(LuaMemberInfo {
            property_owner_id: None,
            key: LuaMemberKey::ExprType(key.clone()),
            typ: field.clone(),
            feature: None,
            overload_index: None,
        });
    }

    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

fn find_index_union(
    db: &DbIndex,
    union: &LuaUnionType,
    infer_guard: &InferGuardRef,
) -> FindMembersResult {
    let mut members = Vec::new();

    for member in union.into_vec() {
        if let Some(sub_members) = find_index_operations_guard(db, &member, infer_guard) {
            members.extend(sub_members);
        }
    }

    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

fn find_index_intersection(
    db: &DbIndex,
    intersection: &LuaIntersectionType,
    infer_guard: &InferGuardRef,
) -> FindMembersResult {
    let mut order: Vec<LuaMemberKey> = Vec::new();
    let mut members: HashMap<LuaMemberKey, LuaType> = HashMap::new();

    for member in intersection.get_types() {
        let Some(sub_members) = find_index_operations_guard(db, member, infer_guard) else {
            continue;
        };

        // Within a single component type, treat duplicate keys as overrides (first wins).
        let mut component_seen: HashSet<LuaMemberKey> = HashSet::new();
        for member in sub_members {
            if !component_seen.insert(member.key.clone()) {
                continue;
            }

            match members.entry(member.key.clone()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    order.push(member.key.clone());
                    entry.insert(member.typ.clone());
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let merged =
                        intersect_member_types(db, entry.get().clone(), member.typ.clone());
                    entry.insert(merged);
                }
            }
        }
    }

    if members.is_empty() {
        None
    } else {
        let mut result: Vec<LuaMemberInfo> = Vec::new();
        for key in order {
            let Some(typ) = members.get(&key) else {
                continue;
            };
            result.push(LuaMemberInfo {
                property_owner_id: None,
                key,
                typ: typ.clone(),
                feature: None,
                overload_index: None,
            });
        }

        Some(result)
    }
}

fn find_index_generic(
    db: &DbIndex,
    generic: &LuaGenericType,
    infer_guard: &InferGuardRef,
) -> FindMembersResult {
    let base_type = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base_type {
        id
    } else {
        return None;
    };

    let generic_params = generic.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(&type_decl_id)?;

    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, Some(&substitutor)) {
            let instantiated_type = instantiate_type_generic(db, &origin_type, &substitutor);
            return find_index_operations_guard(db, &instantiated_type, infer_guard);
        }
        return None;
    }

    let mut members = Vec::new();

    // Check for __index operators with generic substitution
    let operator_index = db.get_operator_index();
    if let Some(index_operator_ids) =
        operator_index.get_operators(&type_decl_id.clone().into(), LuaOperatorMetaMethod::Index)
    {
        for index_operator_id in index_operator_ids {
            if let Some(index_operator) = operator_index.get_operator(index_operator_id) {
                let operand = index_operator.get_operand(db);
                let instantiated_operand = instantiate_type_generic(db, &operand, &substitutor);

                if let Ok(return_type) = index_operator.get_result(db) {
                    let instantiated_return_type =
                        instantiate_type_generic(db, &return_type, &substitutor);

                    members.push(LuaMemberInfo {
                        property_owner_id: None,
                        key: LuaMemberKey::ExprType(instantiated_operand),
                        typ: instantiated_return_type,
                        feature: None,
                        overload_index: None,
                    });
                }
            }
        }
    }

    // Find index operations in super types
    if let Some(supers) = type_index.get_super_types(&type_decl_id) {
        for super_type in supers {
            let instantiated_super = instantiate_type_generic(db, &super_type, &substitutor);
            if let Some(super_members) =
                find_index_operations_guard(db, &instantiated_super, infer_guard)
            {
                members.extend(super_members);
            }
        }
    }

    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

#[allow(unused)]
fn find_index_table_generic(db: &DbIndex, table_params: &[LuaType]) -> FindMembersResult {
    if table_params.len() != 2 {
        return None;
    }

    let mut members = Vec::new();
    let key_type = &table_params[0];
    let value_type = &table_params[1];

    members.push(LuaMemberInfo {
        property_owner_id: None,
        key: LuaMemberKey::ExprType(key_type.clone()),
        typ: value_type.clone(),
        feature: None,
        overload_index: None,
    });

    Some(members)
}
