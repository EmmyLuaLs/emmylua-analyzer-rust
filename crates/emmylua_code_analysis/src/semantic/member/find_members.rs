use std::collections::HashSet;

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

use super::{get_buildin_type_map_type_id, FindMembersResult, LuaMemberInfo};

/// 搜索成员的过滤条件
#[derive(Debug, Clone)]
pub enum FindMemberFilter {
    /// 寻找所有成员
    All,
    /// 根据指定的key寻找成员, 是否寻找所有成员
    ByKey(LuaMemberKey, bool),
}

pub fn find_members(db: &DbIndex, prefix_type: &LuaType) -> FindMembersResult {
    find_members_guard(
        db,
        prefix_type,
        &mut InferGuard::new(),
        &FindMemberFilter::All,
    )
}

pub fn find_members_with_key(
    db: &DbIndex,
    prefix_type: &LuaType,
    member_key: LuaMemberKey,
    find_all: bool,
) -> FindMembersResult {
    find_members_guard(
        db,
        prefix_type,
        &mut InferGuard::new(),
        &FindMemberFilter::ByKey(member_key, find_all),
    )
}

fn find_members_guard(
    db: &DbIndex,
    prefix_type: &LuaType,
    infer_guard: &mut InferGuard,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    match &prefix_type {
        LuaType::TableConst(id) => {
            let member_owner = LuaMemberOwner::Element(id.clone());
            find_normal_members(db, member_owner, filter)
        }
        LuaType::TableGeneric(table_type) => find_table_generic_members(table_type, filter),
        LuaType::String | LuaType::Io | LuaType::StringConst(_) => {
            let type_decl_id = get_buildin_type_map_type_id(&prefix_type)?;
            find_custom_type_members(db, &type_decl_id, infer_guard, filter)
        }
        LuaType::Ref(type_decl_id) => {
            find_custom_type_members(db, type_decl_id, infer_guard, filter)
        }
        LuaType::Def(type_decl_id) => {
            find_custom_type_members(db, type_decl_id, infer_guard, filter)
        }
        // // LuaType::Module(_) => todo!(),
        LuaType::Tuple(tuple_type) => find_tuple_members(tuple_type, filter),
        LuaType::Object(object_type) => find_object_members(object_type, filter),
        LuaType::Union(union_type) => find_union_members(db, union_type, infer_guard, filter),
        LuaType::Intersection(intersection_type) => {
            find_intersection_members(db, intersection_type, infer_guard, filter)
        }
        LuaType::Generic(generic_type) => {
            find_generic_members(db, generic_type, infer_guard, filter)
        }
        LuaType::Global => find_global_members(db, filter),
        LuaType::Instance(inst) => find_instance_members(db, inst, infer_guard, filter),
        LuaType::Namespace(ns) => find_namespace_members(db, ns, filter),
        _ => None,
    }
}

/// 检查成员是否应该被包含
fn should_include_member(member_key: &LuaMemberKey, filter: &FindMemberFilter) -> bool {
    match filter {
        FindMemberFilter::All => true,
        FindMemberFilter::ByKey(target_key, _) => member_key == target_key,
    }
}

/// 检查是否应该停止收集更多成员
fn should_stop_collecting(current_count: usize, filter: &FindMemberFilter) -> bool {
    match filter {
        FindMemberFilter::All => false,
        FindMemberFilter::ByKey(_, find_all) => !find_all && current_count > 0,
    }
}

fn find_table_generic_members(
    table_type: &Vec<LuaType>,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    if table_type.len() != 2 {
        return None;
    }

    let key_type = &table_type[0];
    let value_type = &table_type[1];
    let member_key = LuaMemberKey::ExprType(key_type.clone());

    if should_include_member(&member_key, filter) {
        members.push(LuaMemberInfo {
            property_owner_id: None,
            key: member_key,
            typ: value_type.clone(),
            feature: None,
            overload_index: None,
        });
    }
    Some(members)
}

fn find_normal_members(
    db: &DbIndex,
    member_owner: LuaMemberOwner,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    let member_index = db.get_member_index();
    let owner_members = member_index.get_members(&member_owner)?;

    for member in owner_members {
        let member_key = member.get_key().clone();

        if should_include_member(&member_key, filter) {
            members.push(LuaMemberInfo {
                property_owner_id: Some(LuaSemanticDeclId::Member(member.get_id())),
                key: member_key,
                typ: db
                    .get_type_index()
                    .get_type_cache(&member.get_id().into())
                    .map(|t| t.as_type().clone())
                    .unwrap_or(LuaType::Unknown),
                feature: Some(member.get_feature()),
                overload_index: None,
            });

            if should_stop_collecting(members.len(), filter) {
                break;
            }
        }
    }

    Some(members)
}

fn find_custom_type_members(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
    infer_guard: &mut InferGuard,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    infer_guard.check(&type_decl_id).ok()?;
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(&type_decl_id)?;
    if type_decl.is_alias() {
        if let Some(origin) = type_decl.get_alias_origin(db, None) {
            return find_members_guard(db, &origin, infer_guard, filter);
        } else {
            return find_members_guard(db, &LuaType::String, infer_guard, filter);
        }
    }

    let mut members = Vec::new();
    let member_index = db.get_member_index();
    if let Some(type_members) =
        member_index.get_members(&LuaMemberOwner::Type(type_decl_id.clone()))
    {
        for member in type_members {
            let member_key = member.get_key().clone();

            if should_include_member(&member_key, filter) {
                members.push(LuaMemberInfo {
                    property_owner_id: Some(LuaSemanticDeclId::Member(member.get_id())),
                    key: member_key,
                    typ: db
                        .get_type_index()
                        .get_type_cache(&member.get_id().into())
                        .map(|t| t.as_type().clone())
                        .unwrap_or(LuaType::Unknown),
                    feature: Some(member.get_feature()),
                    overload_index: None,
                });

                if should_stop_collecting(members.len(), filter) {
                    return Some(members);
                }
            }
        }
    }

    if type_decl.is_class() {
        if let Some(super_types) = type_index.get_super_types(&type_decl_id) {
            for super_type in super_types {
                if let Some(super_members) =
                    find_members_guard(db, &super_type, infer_guard, filter)
                {
                    members.extend(super_members);

                    if should_stop_collecting(members.len(), filter) {
                        return Some(members);
                    }
                }
            }
        }
    }

    Some(members)
}

fn find_tuple_members(tuple_type: &LuaTupleType, filter: &FindMemberFilter) -> FindMembersResult {
    let mut members = Vec::new();
    for (idx, typ) in tuple_type.get_types().iter().enumerate() {
        let member_key = LuaMemberKey::Integer((idx + 1) as i64);

        if should_include_member(&member_key, filter) {
            members.push(LuaMemberInfo {
                property_owner_id: None,
                key: member_key,
                typ: typ.clone(),
                feature: None,
                overload_index: None,
            });

            if should_stop_collecting(members.len(), filter) {
                break;
            }
        }
    }

    Some(members)
}

fn find_object_members(
    object_type: &LuaObjectType,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    for (key, typ) in object_type.get_fields().iter() {
        if should_include_member(key, filter) {
            members.push(LuaMemberInfo {
                property_owner_id: None,
                key: key.clone(),
                typ: typ.clone(),
                feature: None,
                overload_index: None,
            });

            if should_stop_collecting(members.len(), filter) {
                break;
            }
        }
    }

    Some(members)
}

fn find_union_members(
    db: &DbIndex,
    union_type: &LuaUnionType,
    infer_guard: &mut InferGuard,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    for typ in union_type.get_types().iter() {
        let sub_members = find_members_guard(db, typ, infer_guard, filter);
        if let Some(sub_members) = sub_members {
            members.extend(sub_members);

            if should_stop_collecting(members.len(), filter) {
                break;
            }
        }
    }

    Some(members)
}

fn find_intersection_members(
    db: &DbIndex,
    intersection_type: &LuaIntersectionType,
    infer_guard: &mut InferGuard,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    for typ in intersection_type.get_types().iter() {
        let sub_members = find_members_guard(db, typ, infer_guard, filter);
        if let Some(sub_members) = sub_members {
            members.push(sub_members);
        }
    }

    if members.is_empty() {
        return None;
    } else if members.len() == 1 {
        return Some(members.remove(0));
    } else {
        let mut result = Vec::new();
        let mut member_set = HashSet::new();

        for member in members.iter().flatten() {
            let key = &member.key;
            let typ = &member.typ;
            if member_set.contains(key) {
                continue;
            }
            member_set.insert(key.clone());

            result.push(LuaMemberInfo {
                property_owner_id: None,
                key: key.clone(),
                typ: typ.clone(),
                feature: None,
                overload_index: None,
            });

            if should_stop_collecting(result.len(), filter) {
                break;
            }
        }

        Some(result)
    }
}

fn find_generic_members_from_super_generics(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
    substitutor: &TypeSubstitutor,
    infer_guard: &mut InferGuard,
    filter: &FindMemberFilter,
) -> Vec<LuaMemberInfo> {
    let type_index = db.get_type_index();

    let Some(type_decl) = type_index.get_type_decl(&type_decl_id) else {
        return vec![];
    };
    if !type_decl.is_class() {
        return vec![];
    };

    let type_decl_id = type_decl.get_id();
    if let Some(super_types) = type_index.get_super_types(&type_decl_id) {
        super_types
            .iter() /*.filter(|super_type| super_type.is_generic())*/
            .filter_map(|super_type| {
                let super_type_sub = instantiate_type_generic(db, &super_type, &substitutor);
                if !super_type_sub.eq(&super_type) {
                    Some(super_type_sub)
                } else {
                    None
                }
            })
            .filter_map(|super_type| {
                let super_type = instantiate_type_generic(db, &super_type, &substitutor);
                find_members_guard(db, &super_type, infer_guard, filter)
            })
            .flatten()
            .collect()
    } else {
        vec![]
    }
}

fn find_generic_members(
    db: &DbIndex,
    generic_type: &LuaGenericType,
    infer_guard: &mut InferGuard,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let base_type = generic_type.get_base_type();
    let mut members = find_members_guard(db, &base_type, infer_guard, filter)?;

    let generic_params = generic_type.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());
    for info in members.iter_mut() {
        info.typ = instantiate_type_generic(db, &info.typ, &substitutor);
    }

    // TODO: this is just a hack to support inheritance from the generic objects
    // like `---@class box<T>: T`. Should be rewritten: generic types should
    // be passed to the called instantiate_type_generic() in some kind of a
    // context.
    if let LuaType::Ref(base_type_decl_id) = base_type {
        members.extend(find_generic_members_from_super_generics(
            db,
            &base_type_decl_id,
            &substitutor,
            infer_guard,
            filter,
        ))
    };

    Some(members)
}

fn find_global_members(db: &DbIndex, filter: &FindMemberFilter) -> FindMembersResult {
    let mut members = Vec::new();
    let global_decls = db.get_global_index().get_all_global_decl_ids();
    for decl_id in global_decls {
        if let Some(decl) = db.get_decl_index().get_decl(&decl_id) {
            let member_key = LuaMemberKey::Name(decl.get_name().to_string().into());

            if should_include_member(&member_key, filter) {
                members.push(LuaMemberInfo {
                    property_owner_id: Some(LuaSemanticDeclId::LuaDecl(decl_id)),
                    key: member_key,
                    typ: db
                        .get_type_index()
                        .get_type_cache(&decl_id.into())
                        .map(|t| t.as_type().clone())
                        .unwrap_or(LuaType::Unknown),
                    feature: None,
                    overload_index: None,
                });

                if should_stop_collecting(members.len(), filter) {
                    break;
                }
            }
        }
    }

    Some(members)
}

fn find_instance_members(
    db: &DbIndex,
    inst: &LuaInstanceType,
    infer_guard: &mut InferGuard,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    let range = inst.get_range();
    let member_owner = LuaMemberOwner::Element(range.clone());
    if let Some(normal_members) = find_normal_members(db, member_owner, filter) {
        members.extend(normal_members);

        if should_stop_collecting(members.len(), filter) {
            return Some(members);
        }
    }

    let origin_type = inst.get_base();
    if let Some(origin_members) = find_members_guard(db, origin_type, infer_guard, filter) {
        members.extend(origin_members);
    }

    Some(members)
}

fn find_namespace_members(db: &DbIndex, ns: &str, filter: &FindMemberFilter) -> FindMembersResult {
    let mut members = Vec::new();

    let prefix = format!("{}.", ns);
    let type_index = db.get_type_index();
    let type_decl_id_map = type_index.find_type_decls(FileId::VIRTUAL, &prefix);
    for (name, type_decl_id) in type_decl_id_map {
        let member_key = LuaMemberKey::Name(name.clone().into());

        if should_include_member(&member_key, filter) {
            if let Some(type_decl_id) = type_decl_id {
                let typ = LuaType::Def(type_decl_id.clone());
                let property_owner_id = LuaSemanticDeclId::TypeDecl(type_decl_id);
                members.push(LuaMemberInfo {
                    property_owner_id: Some(property_owner_id),
                    key: member_key,
                    typ,
                    feature: None,
                    overload_index: None,
                });
            } else {
                members.push(LuaMemberInfo {
                    property_owner_id: None,
                    key: member_key,
                    typ: LuaType::Namespace(SmolStr::new(format!("{}.{}", ns, &name)).into()),
                    feature: None,
                    overload_index: None,
                });
            }

            if should_stop_collecting(members.len(), filter) {
                break;
            }
        }
    }

    Some(members)
}
