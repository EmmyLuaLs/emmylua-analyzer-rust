use std::collections::HashMap;

use smol_str::SmolStr;

use crate::{
    semantic::{
        generic::{instantiate_type_generic, TypeSubstitutor},
        InferGuard,
    },
    DbIndex, FileId, LuaGenericType, LuaInstanceType, LuaIntersectionType, LuaMemberKey,
    LuaMemberOwner, LuaObjectType, LuaSemanticDeclId, LuaTupleType, LuaType, LuaTypeDeclId,
    LuaUnionType,
};

use super::{get_buildin_type_map_type_id, LuaMemberInfo};

pub type InferMemberForKeyResult = Option<Vec<LuaMemberInfo>>;

pub fn infer_member_for_key(
    db: &DbIndex,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> InferMemberForKeyResult {
    infer_member_for_key_guard(db, prefix_type, key, &mut InferGuard::new())
}

pub fn infer_member_for_key_guard(
    db: &DbIndex,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> InferMemberForKeyResult {
    match &prefix_type {
        LuaType::TableConst(id) => {
            let member_owner = LuaMemberOwner::Element(id.clone());
            infer_normal_member_for_key(db, member_owner, key)
        }
        LuaType::TableGeneric(table_type) => infer_table_generic_member_for_key(table_type, key),
        LuaType::String | LuaType::Io | LuaType::StringConst(_) => {
            let type_decl_id = get_buildin_type_map_type_id(&prefix_type)?;
            infer_custom_type_member_for_key(db, &type_decl_id, key, infer_guard)
        }
        LuaType::Ref(type_decl_id) => {
            infer_custom_type_member_for_key(db, type_decl_id, key, infer_guard)
        }
        LuaType::Def(type_decl_id) => {
            infer_custom_type_member_for_key(db, type_decl_id, key, infer_guard)
        }
        LuaType::Tuple(tuple_type) => infer_tuple_member_for_key(tuple_type, key),
        LuaType::Object(object_type) => infer_object_member_for_key(object_type, key),
        LuaType::Union(union_type) => infer_union_member_for_key(db, union_type, key, infer_guard),
        LuaType::Intersection(intersection_type) => {
            infer_intersection_member_for_key(db, intersection_type, key, infer_guard)
        }
        LuaType::Generic(generic_type) => {
            infer_generic_member_for_key(db, generic_type, key, infer_guard)
        }
        LuaType::Global => infer_global_member_for_key(db, key),
        LuaType::Instance(inst) => infer_instance_member_for_key(db, inst, key, infer_guard),
        LuaType::Namespace(ns) => infer_namespace_member_for_key(db, ns, key),
        _ => None,
    }
}

fn infer_table_generic_member_for_key(
    table_type: &Vec<LuaType>,
    key: &LuaMemberKey,
) -> InferMemberForKeyResult {
    if table_type.len() != 2 {
        return None;
    }

    let key_type = &table_type[0];
    let value_type = &table_type[1];

    if let LuaMemberKey::Expr(expr_type) = key {
        if expr_type == key_type {
            return Some(LuaMemberInfo {
                property_owner_id: None,
                key: key.clone(),
                typ: value_type.clone(),
                feature: None,
                overload_index: None,
            });
        }
    }

    None
}

fn infer_normal_member_for_key(
    db: &DbIndex,
    member_owner: LuaMemberOwner,
    key: &LuaMemberKey,
) -> InferMemberForKeyResult {
    let member_index = db.get_member_index();
    if let Some(member_item) = member_index.get_member_item(&member_owner, key) {
        return Some(LuaMemberInfo {
            property_owner_id: Some(LuaSemanticDeclId::Member(member_item.get_id())),
            key: key.clone(),
            typ: db
                .get_type_index()
                .get_type_cache(&member_item.get_id().into())
                .map(|t| t.as_type().clone())
                .unwrap_or(LuaType::Unknown),
            feature: Some(member_item.get_feature()),
            overload_index: None,
        });
    }

    None
}

fn infer_custom_type_member_for_key(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
    key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> InferMemberForKeyResult {
    if infer_guard.check(&type_decl_id).is_err() {
        return None;
    }
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(&type_decl_id)?;
    if type_decl.is_alias() {
        if let Some(origin) = type_decl.get_alias_origin(db, None) {
            return infer_member_for_key_guard(db, &origin, key, infer_guard);
        } else {
            return infer_member_for_key_guard(db, &LuaType::String, key, infer_guard);
        }
    }
    let mut members = Vec::new();
    let member_index = db.get_member_index();
    if let Some(type_members) =
        member_index.get_members(&LuaMemberOwner::Type(type_decl_id.clone()))
    {
        for member in type_members {
            if member.get_key() != key {
                continue;
            }
            members.push(LuaMemberInfo {
                property_owner_id: Some(LuaSemanticDeclId::Member(member.get_id())),
                key: member.get_key().clone(),
                typ: db
                    .get_type_index()
                    .get_type_cache(&member.get_id().into())
                    .map(|t| t.as_type().clone())
                    .unwrap_or(LuaType::Unknown),
                feature: Some(member.get_feature()),
                overload_index: None,
            });
        }
    }

    if members.is_empty() {
        if type_decl.is_class() {
            if let Some(super_types) = type_index.get_super_types(&type_decl_id) {
                for super_type in super_types {
                    if let Some(super_members) =
                        infer_member_for_key_guard(db, &super_type, key, infer_guard)
                    {
                        members.extend(super_members);
                    }
                }
            }
        }
    }

    Some(members)
}

fn infer_tuple_member_for_key(
    tuple_type: &LuaTupleType,
    key: &LuaMemberKey,
) -> InferMemberForKeyResult {
    let mut members = Vec::new();
    for (idx, typ) in tuple_type.get_types().iter().enumerate() {
        if key == &LuaMemberKey::Integer((idx + 1) as i64) {
            members.push(LuaMemberInfo {
                property_owner_id: None,
                key: key.clone(),
                typ: typ.clone(),
                feature: None,
                overload_index: None,
            });
        }
    }

    Some(members)
}

fn infer_object_member_for_key(
    object_type: &LuaObjectType,
    key: &LuaMemberKey,
) -> InferMemberForKeyResult {
    let mut members = Vec::new();
    for (key, typ) in object_type.get_fields().iter() {
        if key == key {
            members.push(LuaMemberInfo {
                property_owner_id: None,
                key: key.clone(),
                typ: typ.clone(),
                feature: None,
                overload_index: None,
            });
        }
    }

    Some(members)
}

fn infer_union_member_for_key(
    db: &DbIndex,
    union_type: &LuaUnionType,
    key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> InferMemberForKeyResult {
    let mut members = Vec::new();
    for typ in union_type.get_types().iter() {
        let sub_members = infer_member_for_key_guard(db, typ, key, infer_guard);
        if let Some(sub_members) = sub_members {
            members.extend(sub_members);
        }
    }

    Some(members)
}

fn infer_intersection_member_for_key(
    db: &DbIndex,
    intersection_type: &LuaIntersectionType,
    key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> InferMemberForKeyResult {
    let mut result = None;

    for typ in intersection_type.get_types().iter() {
        if let Some(member_info) = infer_member_for_key_guard(db, typ, key, infer_guard) {
            if result.is_none() {
                result = Some(member_info);
            } else if result.as_ref().unwrap().typ != member_info.typ {
                return None; // Types don't match across intersection members
            }
        } else {
            return None; // All intersection types must have the member
        }
    }

    result
}

fn infer_generic_member_for_key(
    db: &DbIndex,
    generic_type: &LuaGenericType,
    key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> InferMemberForKeyResult {
    let base_type = generic_type.get_base_type();
    let member_info = infer_member_for_key_guard(db, &base_type, key, infer_guard)?;

    let generic_params = generic_type.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());

    let mut result = member_info.clone();
    result.typ = instantiate_type_generic(db, &result.typ, &substitutor);

    // Handle inheritance from generic objects
    if let LuaType::Ref(base_type_decl_id) = base_type {
        let type_index = db.get_type_index();
        if let Some(type_decl) = type_index.get_type_decl(&base_type_decl_id) {
            if type_decl.is_class() {
                if let Some(super_types) = type_index.get_super_types(&base_type_decl_id) {
                    for super_type in super_types {
                        let super_type_sub =
                            instantiate_type_generic(db, &super_type, &substitutor);
                        if !super_type_sub.eq(&super_type) {
                            if let Some(member_info) =
                                infer_member_for_key_guard(db, &super_type_sub, key, infer_guard)
                            {
                                return Some(member_info);
                            }
                        }
                    }
                }
            }
        }
    }

    Some(result)
}

fn infer_global_member_for_key(db: &DbIndex, key: &LuaMemberKey) -> InferMemberForKeyResult {
    if let LuaMemberKey::Name(name) = key {
        let global_decls = db.get_global_index().get_all_global_decl_ids();
        for decl_id in global_decls {
            if let Some(decl) = db.get_decl_index().get_decl(&decl_id) {
                if decl.get_name().to_string() == name.to_string() {
                    return Some(LuaMemberInfo {
                        property_owner_id: Some(LuaSemanticDeclId::LuaDecl(decl_id)),
                        key: key.clone(),
                        typ: db
                            .get_type_index()
                            .get_type_cache(&decl_id.into())
                            .map(|t| t.as_type().clone())
                            .unwrap_or(LuaType::Unknown),
                        feature: None,
                        overload_index: None,
                    });
                }
            }
        }
    }

    None
}

fn infer_instance_member_for_key(
    db: &DbIndex,
    inst: &LuaInstanceType,
    key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> InferMemberForKeyResult {
    let range = inst.get_range();
    let member_owner = LuaMemberOwner::Element(range.clone());

    if let Some(member_info) = infer_normal_member_for_key(db, member_owner, key) {
        return Some(member_info);
    }

    let origin_type = inst.get_base();
    infer_member_for_key_guard(db, origin_type, key, infer_guard)
}

fn infer_namespace_member_for_key(
    db: &DbIndex,
    ns: &str,
    key: &LuaMemberKey,
) -> InferMemberForKeyResult {
    if let LuaMemberKey::Name(name) = key {
        let prefix = format!("{}.", ns);
        let type_index = db.get_type_index();
        let type_decl_id_map = type_index.find_type_decls(FileId::VIRTUAL, &prefix);

        for (decl_name, type_decl_id) in type_decl_id_map {
            if decl_name == name.as_str() {
                if let Some(type_decl_id) = type_decl_id {
                    let typ = LuaType::Def(type_decl_id.clone());
                    let property_owner_id = LuaSemanticDeclId::TypeDecl(type_decl_id);
                    return Some(LuaMemberInfo {
                        property_owner_id: Some(property_owner_id),
                        key: key.clone(),
                        typ,
                        feature: None,
                        overload_index: None,
                    });
                } else {
                    return Some(LuaMemberInfo {
                        property_owner_id: None,
                        key: key.clone(),
                        typ: LuaType::Namespace(SmolStr::new(format!("{}.{}", ns, name)).into()),
                        feature: None,
                        overload_index: None,
                    });
                }
            }
        }
    }

    None
}
