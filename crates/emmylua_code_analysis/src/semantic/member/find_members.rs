use std::collections::HashSet;

use smol_str::SmolStr;

use crate::{
    DbIndex, FileId, InferGuardRef, LuaGenericType, LuaInstanceType, LuaIntersectionType,
    LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaSemanticDeclId, LuaTupleType, LuaType,
    LuaTypeDeclId, LuaUnionType,
    semantic::{
        InferGuard,
        generic::{TypeSubstitutor, instantiate_type_generic},
    },
};

use super::{FindMembersResult, LuaMemberInfo, get_buildin_type_map_type_id};

#[derive(Debug, Clone)]
pub enum FindMemberFilter {
    /// 寻找所有成员
    All,
    /// 根据指定的key寻找成员
    ByKey {
        /// 要搜索的成员key
        member_key: LuaMemberKey,
        /// 是否寻找所有匹配的成员,为`false`时,找到第一个匹配的成员后停止
        find_all: bool,
    },
}

pub fn find_members(db: &DbIndex, prefix_type: &LuaType) -> FindMembersResult {
    let ctx = FindMembersContext::new(InferGuard::new());
    find_members_guard(db, prefix_type, &ctx, &FindMemberFilter::All)
}

pub fn find_members_with_key(
    db: &DbIndex,
    prefix_type: &LuaType,
    member_key: LuaMemberKey,
    find_all: bool,
) -> FindMembersResult {
    let ctx = FindMembersContext::new(InferGuard::new());
    find_members_guard(
        db,
        prefix_type,
        &ctx,
        &FindMemberFilter::ByKey {
            member_key,
            find_all,
        },
    )
}

#[derive(Clone)]
struct FindMembersContext {
    infer_guard: InferGuardRef,
    substitutor: Option<TypeSubstitutor>,
}

impl FindMembersContext {
    fn new(infer_guard: InferGuardRef) -> Self {
        Self {
            infer_guard,
            substitutor: None,
        }
    }
    fn with_substitutor(&self, substitutor: TypeSubstitutor) -> Self {
        Self {
            infer_guard: self.infer_guard.clone(),
            substitutor: Some(substitutor),
        }
    }

    fn fork_infer(&self) -> Self {
        Self {
            infer_guard: self.infer_guard.fork(),
            substitutor: self.substitutor.clone(),
        }
    }

    fn instantiate_type(&self, db: &DbIndex, ty: &LuaType) -> LuaType {
        if let Some(substitutor) = &self.substitutor {
            instantiate_type_generic(db, ty, substitutor)
        } else {
            ty.clone()
        }
    }

    fn infer_guard(&self) -> &InferGuardRef {
        &self.infer_guard
    }
}

fn find_members_guard(
    db: &DbIndex,
    prefix_type: &LuaType,
    ctx: &FindMembersContext,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    match &prefix_type {
        LuaType::TableConst(id) => {
            let member_owner = LuaMemberOwner::Element(id.clone());
            find_normal_members(db, ctx, member_owner, filter)
        }
        LuaType::TableGeneric(table_type) => {
            find_table_generic_members(db, ctx, table_type, filter)
        }
        LuaType::String
        | LuaType::Io
        | LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::Language(_) => {
            let type_decl_id = get_buildin_type_map_type_id(prefix_type)?;
            find_custom_type_members(db, ctx, &type_decl_id, filter)
        }
        LuaType::Ref(type_decl_id) => find_custom_type_members(db, ctx, type_decl_id, filter),
        LuaType::Def(type_decl_id) => find_custom_type_members(db, ctx, type_decl_id, filter),
        LuaType::Tuple(tuple_type) => find_tuple_members(db, ctx, tuple_type, filter),
        LuaType::Object(object_type) => find_object_members(db, ctx, object_type, filter),
        LuaType::Union(union_type) => find_union_members(db, union_type, ctx, filter),
        LuaType::MultiLineUnion(multi_union) => {
            let union_type = multi_union.to_union();
            if let LuaType::Union(union_type) = union_type {
                find_union_members(db, &union_type, ctx, filter)
            } else {
                None
            }
        }
        LuaType::Intersection(intersection_type) => {
            find_intersection_members(db, intersection_type, ctx, filter)
        }
        LuaType::Generic(generic_type) => find_generic_members(db, generic_type, ctx, filter),
        LuaType::Global => find_global_members(db, ctx, filter),
        LuaType::Instance(inst) => find_instance_members(db, inst, ctx, filter),
        LuaType::Namespace(ns) => find_namespace_members(db, ctx, ns, filter),
        LuaType::ModuleRef(file_id) => {
            let module_info = db.get_module_index().get_module(*file_id);
            if let Some(module_info) = module_info
                && let Some(export_type) = &module_info.export_type
            {
                return find_members_guard(db, export_type, ctx, filter);
            }

            None
        }
        _ => None,
    }
}

/// 检查成员是否应该被包含
fn should_include_member(key: &LuaMemberKey, filter: &FindMemberFilter) -> bool {
    match filter {
        FindMemberFilter::All => true,
        FindMemberFilter::ByKey { member_key, .. } => member_key == key,
    }
}

/// 检查是否应该停止收集更多成员
fn should_stop_collecting(current_count: usize, filter: &FindMemberFilter) -> bool {
    match filter {
        FindMemberFilter::ByKey { find_all, .. } => !find_all && current_count > 0,
        _ => false,
    }
}

fn find_table_generic_members(
    db: &DbIndex,
    ctx: &FindMembersContext,
    table_type: &[LuaType],
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    if table_type.len() != 2 {
        return None;
    }

    let key_type = ctx.instantiate_type(db, &table_type[0]);
    let value_type = ctx.instantiate_type(db, &table_type[1]);
    let member_key = LuaMemberKey::ExprType(key_type);

    if should_include_member(&member_key, filter) {
        members.push(LuaMemberInfo {
            property_owner_id: None,
            key: member_key,
            typ: value_type,
            feature: None,
            overload_index: None,
        });
    }
    Some(members)
}

fn find_normal_members(
    db: &DbIndex,
    ctx: &FindMembersContext,
    member_owner: LuaMemberOwner,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    let member_index = db.get_member_index();
    let owner_members = member_index.get_members(&member_owner)?;

    for member in owner_members {
        let member_key = member.get_key().clone();

        if should_include_member(&member_key, filter) {
            let raw_type = db
                .get_type_index()
                .get_type_cache(&member.get_id().into())
                .map(|t| t.as_type().clone())
                .unwrap_or(LuaType::Unknown);
            members.push(LuaMemberInfo {
                property_owner_id: Some(LuaSemanticDeclId::Member(member.get_id())),
                key: member_key,
                typ: ctx.instantiate_type(db, &raw_type),
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
    ctx: &FindMembersContext,
    type_decl_id: &LuaTypeDeclId,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    ctx.infer_guard().check(type_decl_id).ok()?;
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(type_decl_id)?;
    if type_decl.is_alias() {
        if let Some(origin) = type_decl.get_alias_origin(db, None) {
            return find_members_guard(db, &origin, ctx, filter);
        } else {
            return find_members_guard(db, &LuaType::String, ctx, filter);
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
                let raw_type = db
                    .get_type_index()
                    .get_type_cache(&member.get_id().into())
                    .map(|t| t.as_type().clone())
                    .unwrap_or(LuaType::Unknown);
                members.push(LuaMemberInfo {
                    property_owner_id: Some(LuaSemanticDeclId::Member(member.get_id())),
                    key: member_key,
                    typ: ctx.instantiate_type(db, &raw_type),
                    feature: Some(member.get_feature()),
                    overload_index: None,
                });

                if should_stop_collecting(members.len(), filter) {
                    return Some(members);
                }
            }
        }
    }

    if type_decl.is_class()
        && let Some(super_types) = type_index.get_super_types(type_decl_id)
    {
        for super_type in super_types {
            let instantiated_super = ctx.instantiate_type(db, &super_type);
            if let Some(super_members) = find_members_guard(db, &instantiated_super, ctx, filter) {
                members.extend(super_members);

                if should_stop_collecting(members.len(), filter) {
                    return Some(members);
                }
            }
        }
    }

    Some(members)
}

fn find_tuple_members(
    db: &DbIndex,
    ctx: &FindMembersContext,
    tuple_type: &LuaTupleType,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    for (idx, typ) in tuple_type.get_types().iter().enumerate() {
        let member_key = LuaMemberKey::Integer((idx + 1) as i64);

        if should_include_member(&member_key, filter) {
            members.push(LuaMemberInfo {
                property_owner_id: None,
                key: member_key,
                typ: ctx.instantiate_type(db, typ),
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
    db: &DbIndex,
    ctx: &FindMembersContext,
    object_type: &LuaObjectType,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    for (key, typ) in object_type.get_fields().iter() {
        if should_include_member(key, filter) {
            members.push(LuaMemberInfo {
                property_owner_id: None,
                key: key.clone(),
                typ: ctx.instantiate_type(db, typ),
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
    ctx: &FindMembersContext,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    let mut meet_string = false;
    for typ in union_type.into_vec().iter() {
        let instantiated_type = ctx.instantiate_type(db, typ);
        if instantiated_type.is_string() {
            if meet_string {
                continue;
            }
            meet_string = true;
        }

        let fork_ctx = ctx.fork_infer();
        let sub_members = find_members_guard(db, &instantiated_type, &fork_ctx, filter);
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
    ctx: &FindMembersContext,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    for typ in intersection_type.get_types().iter() {
        let instantiated_type = ctx.instantiate_type(db, typ);
        let fork_ctx = ctx.fork_infer();
        let sub_members = find_members_guard(db, &instantiated_type, &fork_ctx, filter);
        if let Some(sub_members) = sub_members {
            members.push(sub_members);
        }
    }

    if members.is_empty() {
        None
    } else if members.len() == 1 {
        Some(members.remove(0))
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

fn find_generic_members(
    db: &DbIndex,
    generic_type: &LuaGenericType,
    ctx: &FindMembersContext,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let base_ref_id = generic_type.get_base_type_id_ref();
    let instantiated_params: Vec<LuaType> = generic_type
        .get_params()
        .iter()
        .map(|param| ctx.instantiate_type(db, param))
        .collect();
    let substitutor = TypeSubstitutor::from_type_array(instantiated_params);
    let type_decl = db.get_type_index().get_type_decl(&base_ref_id)?;
    let ctx_with_substitutor = ctx.with_substitutor(substitutor.clone());
    if let Some(origin) = type_decl.get_alias_origin(db, Some(&substitutor)) {
        return find_members_guard(db, &origin, &ctx_with_substitutor, filter);
    }

    find_members_guard(
        db,
        &LuaType::Ref(base_ref_id.clone()),
        &ctx_with_substitutor,
        filter,
    )
}

fn find_global_members(
    db: &DbIndex,
    ctx: &FindMembersContext,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    let global_decls = db.get_global_index().get_all_global_decl_ids();
    for decl_id in global_decls {
        if let Some(decl) = db.get_decl_index().get_decl(&decl_id) {
            let member_key = LuaMemberKey::Name(decl.get_name().to_string().into());

            if should_include_member(&member_key, filter) {
                let raw_type = db
                    .get_type_index()
                    .get_type_cache(&decl_id.into())
                    .map(|t| t.as_type().clone())
                    .unwrap_or(LuaType::Unknown);
                members.push(LuaMemberInfo {
                    property_owner_id: Some(LuaSemanticDeclId::LuaDecl(decl_id)),
                    key: member_key,
                    typ: ctx.instantiate_type(db, &raw_type),
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
    ctx: &FindMembersContext,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();
    let range = inst.get_range();
    let member_owner = LuaMemberOwner::Element(range.clone());
    if let Some(normal_members) = find_normal_members(db, ctx, member_owner, filter) {
        members.extend(normal_members);

        if should_stop_collecting(members.len(), filter) {
            return Some(members);
        }
    }

    let origin_type = ctx.instantiate_type(db, inst.get_base());
    if let Some(origin_members) = find_members_guard(db, &origin_type, ctx, filter) {
        members.extend(origin_members);
    }

    Some(members)
}

fn find_namespace_members(
    db: &DbIndex,
    ctx: &FindMembersContext,
    ns: &str,
    filter: &FindMemberFilter,
) -> FindMembersResult {
    let mut members = Vec::new();

    let prefix = format!("{}.", ns);
    let type_index = db.get_type_index();
    let type_decl_id_map = type_index.find_type_decls(FileId::VIRTUAL, &prefix);
    for (name, type_decl_id) in type_decl_id_map {
        let member_key = LuaMemberKey::Name(name.clone().into());

        if should_include_member(&member_key, filter) {
            if let Some(type_decl_id) = type_decl_id {
                let def_type = LuaType::Def(type_decl_id.clone());
                let typ = ctx.instantiate_type(db, &def_type);
                let property_owner_id = LuaSemanticDeclId::TypeDecl(type_decl_id);
                members.push(LuaMemberInfo {
                    property_owner_id: Some(property_owner_id),
                    key: member_key,
                    typ,
                    feature: None,
                    overload_index: None,
                });
            } else {
                let ns_type = LuaType::Namespace(SmolStr::new(format!("{}.{}", ns, &name)).into());
                members.push(LuaMemberInfo {
                    property_owner_id: None,
                    key: member_key,
                    typ: ctx.instantiate_type(db, &ns_type),
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
