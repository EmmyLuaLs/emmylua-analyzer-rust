mod infer_array;

use emmylua_parser::{
    LuaExpr, LuaIndexExpr, LuaIndexKey, LuaIndexMemberExpr, NumberResult, PathTrait,
};
use hashbrown::HashSet;
use internment::ArcIntern;
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    CacheEntry, GenericTpl, InFiled, InferFailReason, InferGuardRef, LuaAliasCallKind,
    LuaDeclOrMemberId, LuaInferCache, LuaInstanceType, LuaMemberOwner, LuaOperatorOwner, TypeOps,
    complete_type_generic_args,
    db_index::{
        DbIndex, LuaGenericType, LuaIntersectionType, LuaMemberKey, LuaObjectType,
        LuaOperatorMetaMethod, LuaTupleType, LuaType, LuaTypeDeclId, LuaUnionType,
    },
    enum_variable_is_param, get_keyof_members, get_member_item, get_members, get_operator,
    get_operators, get_tpl_ref_extend_type, get_type_by_owner, get_type_def_kind,
    infer_compilation_type_property_type, infer_compilation_type_super_types, infer_expr,
    module_query::export::infer_module_export_type,
    semantic::{
        InferGuard,
        generic::{TypeSubstitutor, instantiate_type_generic},
        infer::{
            InferResult, VarRefId,
            infer_index::infer_array::{
                array_member_fallback, check_iter_var_range, infer_array_member_by_key,
            },
            infer_name::{get_name_expr_var_ref_id, infer_global_type},
            narrow::infer_expr_narrow_type,
        },
        member::{get_buildin_type_map_type_id, intersect_member_types},
        type_check::check_type_compact,
    },
    try_infer_expr_no_flow, type_def_alias_origin, type_def_is_alias, type_def_is_class,
    type_def_is_enum,
};

pub(crate) fn try_infer_expr_for_index(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
) -> Result<Option<LuaType>, InferFailReason> {
    if cache.is_no_flow() {
        return match expr {
            LuaExpr::ParenExpr(paren_expr) => {
                let Some(expr) = paren_expr.get_expr() else {
                    return Ok(None);
                };
                try_infer_expr_for_index(db, cache, expr)
            }
            LuaExpr::ClosureExpr(_) => Ok(None),
            expr => match try_infer_expr_no_flow(db, cache, expr) {
                Ok(result) => Ok(result),
                Err(err) if err.is_need_resolve() => Ok(None),
                Err(err) => Err(err),
            },
        };
    }

    Ok(Some(infer_expr(db, cache, expr)?))
}

pub(super) fn infer_expr_for_index(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
) -> InferResult {
    let Some(expr_type) = try_infer_expr_for_index(db, cache, expr)? else {
        return Err(InferFailReason::None);
    };
    Ok(expr_type)
}

pub fn infer_index_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: LuaIndexExpr,
    pass_flow: bool,
) -> InferResult {
    let prefix_expr = index_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer_expr_for_index(db, cache, prefix_expr)?;
    let index_member_expr = LuaIndexMemberExpr::IndexExpr(index_expr.clone());

    let member_type = infer_member(
        db,
        cache,
        &prefix_type,
        index_member_expr,
        &InferGuard::new(),
    )?;

    if pass_flow {
        infer_member_type_pass_flow(db, cache, index_expr, member_type)
    } else {
        Ok(member_type)
    }
}

fn infer_member_type_pass_flow(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: LuaIndexExpr,
    member_type: LuaType,
) -> InferResult {
    let Some(var_ref_id) = get_index_expr_var_ref_id(db, cache, &index_expr) else {
        return Ok(member_type);
    };

    cache
        .index_ref_origin_type_cache
        .insert(var_ref_id.clone(), CacheEntry::Cache(member_type.clone()));

    let result = infer_expr_narrow_type(db, cache, LuaExpr::IndexExpr(index_expr), var_ref_id);
    match &result {
        Err(InferFailReason::None) => Ok(member_type.clone()),
        _ => result,
    }
}

pub fn get_index_expr_var_ref_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: &LuaIndexExpr,
) -> Option<VarRefId> {
    let access_path = match index_expr.get_access_path() {
        Some(path) => ArcIntern::new(SmolStr::new(&path)),
        None => return None,
    };

    let mut prefix_expr = index_expr.get_prefix_expr()?;
    while let LuaExpr::IndexExpr(index_expr) = prefix_expr {
        prefix_expr = index_expr.get_prefix_expr()?;
    }

    if let LuaExpr::NameExpr(name_expr) = prefix_expr {
        let decl_or_member_id = match get_name_expr_var_ref_id(db, cache, &name_expr) {
            Some(VarRefId::SelfRef(decl_or_id)) => decl_or_id,
            Some(VarRefId::VarRef(decl_id)) => LuaDeclOrMemberId::Decl(decl_id),
            _ => return None,
        };

        let var_ref_id = VarRefId::IndexRef(decl_or_member_id, access_path);
        return Some(var_ref_id);
    }

    None
}

fn infer_index_key_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_key: &LuaIndexKey,
) -> Result<Option<LuaType>, InferFailReason> {
    match index_key {
        LuaIndexKey::Name(name) => Ok(Some(LuaType::StringConst(
            SmolStr::new(name.get_name_text()).into(),
        ))),
        LuaIndexKey::String(s) => Ok(Some(LuaType::StringConst(
            SmolStr::new(s.get_value()).into(),
        ))),
        LuaIndexKey::Integer(i) => {
            if let NumberResult::Int(index_value) = i.get_number_value() {
                Ok(Some(LuaType::IntegerConst(index_value)))
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }
        LuaIndexKey::Idx(i) => Ok(Some(LuaType::IntegerConst(*i as i64))),
        LuaIndexKey::Expr(expr) => try_infer_expr_for_index(db, cache, expr.clone()),
    }
}

fn infer_index_expr_key_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: &LuaIndexMemberExpr,
) -> InferResult {
    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    infer_index_key_type(db, cache, &index_key)?.ok_or(InferFailReason::None)
}

fn member_key_from_type(key_type: &LuaType) -> LuaMemberKey {
    match key_type {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => LuaMemberKey::Name((**s).clone()),
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => LuaMemberKey::Integer(*i),
        _ => LuaMemberKey::ExprType(key_type.clone()),
    }
}

#[derive(Debug, Clone)]
struct MemberLookupQuery {
    index_expr: LuaIndexMemberExpr,
    key_type: LuaType,
    key: LuaMemberKey,
}

impl MemberLookupQuery {
    fn from_index_expr(
        db: &DbIndex,
        cache: &mut LuaInferCache,
        index_expr: LuaIndexMemberExpr,
    ) -> Result<Self, InferFailReason> {
        let key_type = infer_index_expr_key_type(db, cache, &index_expr)?;
        Ok(Self::from_key_type(index_expr, key_type))
    }

    fn from_key_type(index_expr: LuaIndexMemberExpr, key_type: LuaType) -> Self {
        let key = member_key_from_type(&key_type);
        Self {
            index_expr,
            key_type,
            key,
        }
    }
}

pub fn infer_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let lookup = MemberLookupQuery::from_index_expr(db, cache, index_expr)?;
    match infer_member_by_lookup(db, cache, prefix_type, &lookup, infer_guard) {
        Ok(member_type) => Ok(member_type),
        Err(InferFailReason::FieldNotFound) => infer_member_by_operator_key_type(
            db,
            cache,
            prefix_type,
            &lookup.key_type,
            &InferGuard::new(),
        ),
        Err(err) => Err(err),
    }
}

pub fn infer_member_by_member_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let lookup = MemberLookupQuery::from_index_expr(db, cache, index_expr)?;
    infer_member_by_lookup(db, cache, prefix_type, &lookup, infer_guard)
}

pub fn infer_member_by_key_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
    key_type: &LuaType,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let lookup = MemberLookupQuery::from_key_type(index_expr, key_type.clone());
    infer_member_by_lookup(db, cache, prefix_type, &lookup, infer_guard)
}

fn infer_member_by_lookup(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> InferResult {
    match &prefix_type {
        LuaType::Table | LuaType::Any | LuaType::Unknown => Ok(LuaType::Any),
        LuaType::Nil => Ok(LuaType::Never),
        LuaType::TableConst(id) => {
            infer_table_member(db, id.clone(), &lookup.key_type, &lookup.key)
        }
        LuaType::String
        | LuaType::Io
        | LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::Language(_) => {
            let decl_id = get_buildin_type_map_type_id(prefix_type).ok_or(InferFailReason::None)?;
            infer_custom_type_member(db, cache, decl_id, lookup, infer_guard)
        }
        LuaType::Ref(decl_id) | LuaType::Def(decl_id) => {
            infer_custom_type_member(db, cache, decl_id.clone(), lookup, infer_guard)
        }
        // LuaType::Module(_) => todo!(),
        LuaType::Tuple(tuple_type) => infer_tuple_member(db, cache, tuple_type, lookup),
        LuaType::Object(object_type) => {
            infer_object_member(db, object_type, &lookup.key_type, &lookup.key)
        }
        LuaType::Union(union_type) => {
            infer_union_member(db, cache, union_type, lookup, infer_guard)
        }
        LuaType::MultiLineUnion(multi_union) => {
            let union_type = multi_union.to_union();
            if let LuaType::Union(union_type) = union_type {
                infer_union_member(db, cache, &union_type, lookup, infer_guard)
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }
        LuaType::Intersection(intersection_type) => {
            infer_intersection_member(db, cache, intersection_type, lookup, infer_guard)
        }
        LuaType::Generic(generic_type) => {
            infer_generic_member(db, cache, generic_type, lookup, infer_guard)
        }
        LuaType::Global => infer_global_field_member(db, &lookup.key),
        LuaType::Instance(inst) => infer_instance_member(db, cache, inst, lookup, infer_guard),
        LuaType::Namespace(ns) => infer_namespace_member(db, ns, &lookup.key),
        LuaType::Array(array_type) => infer_array_member_by_key(
            db,
            cache,
            array_type,
            lookup.index_expr.clone(),
            &lookup.key_type,
            &lookup.key,
        ),
        LuaType::TableGeneric(table_generic) => {
            infer_table_generic_member_by_key_type(db, table_generic, &lookup.key_type)
        }
        LuaType::TplRef(tpl) => infer_tpl_ref_member(db, cache, tpl, lookup, infer_guard),
        LuaType::ModuleRef(file_id) => {
            if let Some(export_type) = infer_module_export_type(db, *file_id) {
                if export_type.is_module_ref() {
                    return Err(InferFailReason::RecursiveInfer);
                }

                return infer_member_by_lookup(db, cache, &export_type, lookup, infer_guard);
            }

            Err(InferFailReason::UnResolveModuleExport(*file_id))
        }
        _ => Err(InferFailReason::FieldNotFound),
    }
}

fn infer_table_member(
    db: &DbIndex,
    inst: InFiled<TextRange>,
    key_type: &LuaType,
    key: &LuaMemberKey,
) -> InferResult {
    let owner = LuaMemberOwner::Element(inst);
    if let Some(member_item) = get_member_item(db, &owner, key) {
        return member_item.resolve_type(db);
    }

    if matches!(key, LuaMemberKey::Name(_) | LuaMemberKey::Integer(_)) {
        // Exact keys already missed above. The matching scan is only for broad keys.
        return Err(InferFailReason::FieldNotFound);
    }

    if let Some(result_type) = infer_type_key_member_type(db, key_type, &owner) {
        return Ok(result_type);
    }

    infer_matching_member_key_type(db, &owner, key_type).ok_or(InferFailReason::FieldNotFound)
}

fn infer_custom_type_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type_id: LuaTypeDeclId,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> InferResult {
    infer_guard.check(&prefix_type_id)?;
    let generic_substitutor = complete_type_generic_args(db, &prefix_type_id, Vec::new())
        .completed_args
        .filter(|args| !args.is_empty())
        .map(TypeSubstitutor::from_type_array);
    if type_def_is_alias(db, &prefix_type_id) {
        if let Some(origin_type) = type_def_alias_origin(db, &prefix_type_id) {
            let origin = if let Some(substitutor) = &generic_substitutor {
                instantiate_type_generic(db, &origin_type, substitutor)
            } else {
                origin_type
            };
            return infer_member_by_lookup(db, cache, &origin, lookup, infer_guard);
        }
        return Err(InferFailReason::FieldNotFound);
    }
    if let LuaIndexMemberExpr::IndexExpr(index_expr) = &lookup.index_expr
        && enum_variable_is_param(db, cache, index_expr, &LuaType::Ref(prefix_type_id.clone()))
            .is_some()
    {
        return Err(InferFailReason::None);
    }

    let owner = LuaMemberOwner::Type(prefix_type_id.clone());
    if let Some(summary_type) =
        infer_compilation_type_property_type(db, &prefix_type_id, &lookup.key)
    {
        return Ok(if let Some(substitutor) = &generic_substitutor {
            instantiate_type_generic(db, &summary_type, substitutor)
        } else {
            summary_type
        });
    }

    if let Some(member_item) = get_member_item(db, &owner, &lookup.key) {
        let member_type = member_item.resolve_type(db)?;
        return Ok(if let Some(substitutor) = &generic_substitutor {
            instantiate_type_generic(db, &member_type, substitutor)
        } else {
            member_type
        });
    }

    // Exact keys may still resolve through super types below; only broad keys need key-type matching here.
    if !matches!(lookup.key, LuaMemberKey::Name(_) | LuaMemberKey::Integer(_))
        && let Some(result_type) = infer_type_key_member_type(db, &lookup.key_type, &owner)
    {
        return Ok(result_type);
    }

    if let Some(super_types) = infer_compilation_type_super_types(db, &prefix_type_id)
        .or_else(|| db.get_type_index().get_super_types(&prefix_type_id))
    {
        for super_type in super_types {
            let super_type = if let Some(substitutor) = &generic_substitutor {
                instantiate_type_generic(db, &super_type, substitutor)
            } else {
                super_type
            };
            let result = infer_member_by_lookup(db, cache, &super_type, lookup, infer_guard);

            match result {
                Ok(member_type) => {
                    return Ok(member_type);
                }
                Err(InferFailReason::FieldNotFound) => {}
                Err(err) => return Err(err),
            }
        }
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_type_key_member_type(
    db: &DbIndex,
    key_type: &LuaType,
    owner: &LuaMemberOwner,
) -> Option<LuaType> {
    let keys = get_type_member_key(db, key_type)?;

    let mut result_types = Vec::new();
    for key in keys {
        if let Some(member_item) = get_member_item(db, owner, &key)
            && let Ok(member_type) = member_item.resolve_type(db)
        {
            result_types.push(member_type);
        }
    }

    match &result_types[..] {
        [] => None,
        [first] => Some(first.clone()),
        _ => Some(LuaType::from_vec(result_types)),
    }
}

fn infer_matching_member_key_type(
    db: &DbIndex,
    owner: &LuaMemberOwner,
    key_type: &LuaType,
) -> Option<LuaType> {
    let mut members = get_members(db, owner)?;
    members.sort_by(|a, b| a.get_key().cmp(b.get_key()));

    // Build the union once; broad dynamic keys can match thousands of table members.
    let mut result_types = Vec::new();
    for member in members {
        let member_key_type = match member.get_key() {
            LuaMemberKey::Name(s) => LuaType::StringConst(s.clone().into()),
            LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
            _ => continue,
        };
        if check_type_compact(db, key_type, &member_key_type).is_ok() {
            let member_type =
                get_type_by_owner(db, &member.get_id().into()).unwrap_or(LuaType::Unknown);

            result_types.push(member_type);
        }
    }

    if result_types.is_empty() {
        return None;
    }

    if matches!(
        key_type,
        LuaType::String | LuaType::Number | LuaType::Integer
    ) {
        result_types.push(LuaType::Nil);
    }

    Some(LuaType::from_vec(result_types))
}

fn get_type_member_key(db: &DbIndex, key_type: &LuaType) -> Option<Vec<LuaMemberKey>> {
    let mut keys = HashSet::new();
    collect_type_member_keys(db, key_type, &mut keys);
    if keys.is_empty() {
        return None;
    }

    let mut keys: Vec<_> = keys.into_iter().collect();
    keys.sort();
    Some(keys)
}

fn infer_tuple_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    tuple_type: &LuaTupleType,
    lookup: &MemberLookupQuery,
) -> InferResult {
    let index_key = lookup
        .index_expr
        .get_index_key()
        .ok_or(InferFailReason::None)?;
    match &lookup.key {
        LuaMemberKey::Integer(i) => {
            let index = if *i > 0 { *i - 1 } else { 0 };
            return match tuple_type.get_type(index as usize) {
                Some(typ) => Ok(typ.clone()),
                None => Err(InferFailReason::FieldNotFound),
            };
        }
        LuaMemberKey::ExprType(expr_type) => match expr_type {
            LuaType::IntegerConst(i) => {
                let index = if *i > 0 { *i - 1 } else { 0 };
                return match tuple_type.get_type(index as usize) {
                    Some(typ) => Ok(typ.clone()),
                    None => Err(InferFailReason::FieldNotFound),
                };
            }
            LuaType::Integer => {
                let mut result_types = tuple_type.get_types().to_vec();
                let include_nil = match &lookup.index_expr {
                    LuaIndexMemberExpr::TableField(_) => false,
                    _ => {
                        let index_prefix_expr = lookup
                            .index_expr
                            .get_prefix_expr()
                            .ok_or(InferFailReason::None)?;
                        match &index_key {
                            LuaIndexKey::Expr(expr) => {
                                !check_iter_var_range(db, cache, expr, index_prefix_expr)
                                    .unwrap_or(false)
                            }
                            _ => false,
                        }
                    }
                };
                if include_nil {
                    result_types.push(LuaType::Nil);
                }

                return Ok(TypeOps::union_all(db, result_types));
            }
            _ => {}
        },
        _ => {}
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_object_member(
    db: &DbIndex,
    object_type: &LuaObjectType,
    key_type: &LuaType,
    member_key: &LuaMemberKey,
) -> InferResult {
    if let Some(member_type) = object_type.get_field(member_key) {
        return Ok(member_type.clone());
    }

    // todo
    let index_accesses = object_type.get_index_access();
    for (key, value) in index_accesses {
        let result = infer_index_metamethod_by_key_type(db, key_type, key, value);
        match result {
            Ok(typ) => {
                return Ok(typ);
            }
            Err(InferFailReason::FieldNotFound) => {}
            Err(err) => {
                return Err(err);
            }
        }
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_index_metamethod_by_key_type(
    db: &DbIndex,
    access_key_type: &LuaType,
    key_type: &LuaType,
    value_type: &LuaType,
) -> InferResult {
    if check_type_compact(db, key_type, access_key_type).is_ok() {
        return Ok(value_type.clone());
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_union_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union_type: &LuaUnionType,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let mut member_types = Vec::new();
    let mut has_missing_member = false;
    let mut meet_string = false;
    for sub_type in union_type.into_vec() {
        if sub_type.is_string() {
            if meet_string {
                continue;
            }
            meet_string = true;
        }
        let result = infer_member_by_lookup(db, cache, &sub_type, lookup, &infer_guard.fork());
        match result {
            Ok(typ) => {
                member_types.push(typ);
            }
            Err(_) => {
                has_missing_member = true;
            }
        }
    }

    if member_types.is_empty() {
        return Err(InferFailReason::FieldNotFound);
    }

    if has_missing_member {
        member_types.push(LuaType::Nil);
    }

    Ok(TypeOps::union_all(db, member_types))
}

fn infer_intersection_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    intersection_type: &LuaIntersectionType,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let mut result: Option<LuaType> = None;
    for member in intersection_type.get_types() {
        match infer_member_by_lookup(db, cache, member, lookup, &infer_guard.fork()) {
            Ok(ty) => {
                result = Some(match result {
                    Some(prev) => intersect_member_types(db, prev, ty),
                    None => ty,
                });

                if matches!(result, Some(LuaType::Never)) {
                    break;
                }
            }
            Err(InferFailReason::FieldNotFound) => continue,
            Err(reason) => return Err(reason),
        }
    }

    result.ok_or(InferFailReason::FieldNotFound)
}

fn infer_generic_members_from_super_generics(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    type_decl_id: &LuaTypeDeclId,
    substitutor: &TypeSubstitutor,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> Option<LuaType> {
    if !type_def_is_class(db, type_decl_id) {
        return None;
    }

    if let Some(super_types) = infer_compilation_type_super_types(db, type_decl_id)
        .or_else(|| db.get_type_index().get_super_types(type_decl_id))
    {
        super_types.iter().find_map(|super_type| {
            let super_type = instantiate_type_generic(db, super_type, substitutor);
            infer_member_by_lookup(db, cache, &super_type, lookup, &infer_guard.fork()).ok()
        })
    } else {
        None
    }
}

fn normalize_generic_member_param(db: &DbIndex, ty: LuaType) -> LuaType {
    match ty {
        LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl) => tpl
            .get_default_type()
            .map(|default_type| crate::complete_type_generic_args_in_type(db, default_type))
            .unwrap_or(LuaType::TplRef(tpl)),
        other => crate::complete_type_generic_args_in_type(db, &other),
    }
}

fn infer_generic_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic_type: &LuaGenericType,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let base_type = generic_type.get_base_type();

    let generic_params = if let LuaType::Ref(base_type_decl_id) = &base_type {
        complete_type_generic_args(db, base_type_decl_id, generic_type.get_params().clone())
            .completed_args
            .unwrap_or_else(|| generic_type.get_params().clone())
    } else {
        generic_type.get_params().clone()
    };
    let generic_params = generic_params
        .into_iter()
        .map(|param| normalize_generic_member_param(db, param))
        .collect();
    let substitutor = TypeSubstitutor::from_type_array(generic_params);

    if let LuaType::Ref(base_type_decl_id) = &base_type {
        if type_def_is_alias(db, base_type_decl_id)
            && let Some(origin_type) = type_def_alias_origin(db, base_type_decl_id)
        {
            let instantiated = instantiate_type_generic(db, &origin_type, &substitutor);
            return infer_member_by_lookup(db, cache, &instantiated, lookup, &infer_guard.fork());
        }

        let result = infer_generic_members_from_super_generics(
            db,
            cache,
            base_type_decl_id,
            &substitutor,
            lookup,
            infer_guard,
        );
        if let Some(result) = result {
            return Ok(result);
        }
    }

    let member_type = infer_member_by_lookup(db, cache, &base_type, lookup, infer_guard)?;

    Ok(instantiate_type_generic(db, &member_type, &substitutor))
}

fn infer_instance_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    inst: &LuaInstanceType,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let range = inst.get_range();

    let origin_type = inst.get_base();
    let base_result = infer_member_by_lookup(db, cache, origin_type, lookup, infer_guard);
    match base_result {
        Ok(typ) => match infer_table_member(db, range.clone(), &lookup.key_type, &lookup.key) {
            Ok(table_type) => {
                return Ok(match TypeOps::Intersect.apply(db, &typ, &table_type) {
                    LuaType::Never => typ,
                    intersected => intersected,
                });
            }
            Err(InferFailReason::FieldNotFound) => return Ok(typ),
            Err(err) => return Err(err),
        },
        Err(InferFailReason::FieldNotFound) => {}
        Err(err) => return Err(err),
    }

    infer_table_member(db, range.clone(), &lookup.key_type, &lookup.key)
}

fn infer_member_by_operator_key_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    key_type: &LuaType,
    infer_guard: &InferGuardRef,
) -> InferResult {
    match &prefix_type {
        LuaType::TableConst(in_filed) => infer_member_by_index_table(db, in_filed, key_type),
        LuaType::Ref(decl_id) | LuaType::Def(decl_id) => {
            infer_member_by_index_custom_type(db, cache, decl_id, key_type, infer_guard)
        }
        // LuaType::Module(arc) => todo!(),
        LuaType::Array(array_type) => {
            if key_type.is_number() {
                Ok(array_member_fallback(db, array_type.get_base()))
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }
        LuaType::Object(object) => {
            let key = member_key_from_type(key_type);
            infer_object_member(db, object, key_type, &key)
        }
        LuaType::Union(union) => {
            infer_member_by_index_union(db, cache, union, key_type, infer_guard)
        }
        LuaType::Intersection(intersection) => {
            infer_member_by_index_intersection(db, cache, intersection, key_type, infer_guard)
        }
        LuaType::Generic(generic) => {
            infer_member_by_index_generic(db, cache, generic, key_type, infer_guard)
        }
        LuaType::TableGeneric(table_generic) => {
            infer_table_generic_member_by_key_type(db, table_generic, key_type)
        }
        LuaType::Instance(inst) => {
            let base = inst.get_base();
            infer_member_by_operator_key_type(db, cache, base, key_type, infer_guard)
        }
        LuaType::ModuleRef(file_id) => {
            let module_info = db.get_module_index().get_module(*file_id);
            if let Some(module_info) = module_info {
                if let Some(export_type) = &module_info.export_type {
                    return infer_member_by_operator_key_type(
                        db,
                        cache,
                        export_type,
                        key_type,
                        infer_guard,
                    );
                } else {
                    return Err(InferFailReason::UnResolveModuleExport(*file_id));
                }
            }

            Err(InferFailReason::UnResolveModuleExport(*file_id))
        }
        _ => Err(InferFailReason::FieldNotFound),
    }
}

fn infer_member_by_index_table(
    db: &DbIndex,
    table_range: &InFiled<TextRange>,
    key_type: &LuaType,
) -> InferResult {
    let metatable = db.get_metatable_index().get(table_range);
    match metatable {
        Some(metatable) => {
            let meta_owner = LuaOperatorOwner::Table(metatable.clone());
            let operator_ids = get_operators(db, &meta_owner, LuaOperatorMetaMethod::Index)
                .ok_or(InferFailReason::FieldNotFound)?;

            for operator_id in &operator_ids {
                let operator = get_operator(db, operator_id).ok_or(InferFailReason::None)?;
                let operand = operator.get_operand(db);
                let return_type = operator.get_result(db)?;
                if let Ok(typ) =
                    infer_index_metamethod_by_key_type(db, key_type, &operand, &return_type)
                {
                    return Ok(typ);
                }
            }
        }
        None => {
            let key = member_key_from_type(key_type);
            return infer_table_member(db, table_range.clone(), key_type, &key);
        }
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_member_by_index_custom_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type_id: &LuaTypeDeclId,
    key_type: &LuaType,
    infer_guard: &InferGuardRef,
) -> InferResult {
    infer_guard.check(prefix_type_id)?;
    if type_def_is_alias(db, prefix_type_id) {
        if let Some(origin_type) = type_def_alias_origin(db, prefix_type_id) {
            return infer_member_by_operator_key_type(
                db,
                cache,
                &origin_type,
                key_type,
                infer_guard,
            );
        }
        return Err(InferFailReason::None);
    }

    if let Some(index_operator_ids) = get_operators(
        db,
        &prefix_type_id.clone().into(),
        LuaOperatorMetaMethod::Index,
    ) {
        for operator_id in &index_operator_ids {
            let operator = get_operator(db, operator_id).ok_or(InferFailReason::None)?;
            let operand = operator.get_operand(db);
            let return_type = operator.get_result(db)?;
            let typ = infer_index_metamethod_by_key_type(db, key_type, &operand, &return_type);
            if let Ok(typ) = typ {
                return Ok(typ);
            }
        }
    }

    // find member by key in super
    if type_def_is_class(db, prefix_type_id)
        && let Some(super_types) = infer_compilation_type_super_types(db, prefix_type_id)
            .or_else(|| db.get_type_index().get_super_types(prefix_type_id))
    {
        for super_type in super_types {
            let result =
                infer_member_by_operator_key_type(db, cache, &super_type, key_type, infer_guard);
            match result {
                Ok(member_type) => {
                    return Ok(member_type);
                }
                Err(InferFailReason::FieldNotFound) => {}
                Err(err) => return Err(err),
            }
        }
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_member_by_index_union(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union: &LuaUnionType,
    key_type: &LuaType,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let mut member_types = Vec::new();
    for member in union.into_vec() {
        let result =
            infer_member_by_operator_key_type(db, cache, &member, key_type, &infer_guard.fork());
        match result {
            Ok(typ) => {
                member_types.push(typ);
            }
            Err(InferFailReason::FieldNotFound) => {}
            Err(err) => {
                return Err(err);
            }
        }
    }

    if member_types.is_empty() {
        return Err(InferFailReason::FieldNotFound);
    }

    Ok(TypeOps::union_all(db, member_types))
}

fn infer_member_by_index_intersection(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    intersection: &LuaIntersectionType,
    key_type: &LuaType,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let mut result: Option<LuaType> = None;
    for member in intersection.get_types() {
        match infer_member_by_operator_key_type(db, cache, member, key_type, &infer_guard.fork()) {
            Ok(ty) => {
                result = Some(match result {
                    Some(prev) => intersect_member_types(db, prev, ty),
                    None => ty,
                });

                if matches!(result, Some(LuaType::Never)) {
                    break;
                }
            }
            Err(InferFailReason::FieldNotFound) => continue,
            Err(reason) => return Err(reason),
        }
    }

    result.ok_or(InferFailReason::FieldNotFound)
}

fn infer_member_by_index_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic: &LuaGenericType,
    key_type: &LuaType,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let base_type = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base_type {
        id
    } else {
        return Err(InferFailReason::None);
    };
    let generic_params = generic.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());
    if type_def_is_alias(db, &type_decl_id) {
        if let Some(origin_type) = type_def_alias_origin(db, &type_decl_id) {
            return infer_member_by_operator_key_type(
                db,
                cache,
                &instantiate_type_generic(db, &origin_type, &substitutor),
                key_type,
                &infer_guard.fork(),
            );
        }
        return Err(InferFailReason::None);
    }
    let operator_ids = get_operators(
        db,
        &type_decl_id.clone().into(),
        LuaOperatorMetaMethod::Index,
    );
    if let Some(index_operator_ids) = operator_ids {
        for index_operator_id in &index_operator_ids {
            let index_operator =
                get_operator(db, index_operator_id).ok_or(InferFailReason::None)?;
            let operand = index_operator.get_operand(db);
            let instianted_operand = instantiate_type_generic(db, &operand, &substitutor);
            let return_type =
                instantiate_type_generic(db, &index_operator.get_result(db)?, &substitutor);

            let result =
                infer_index_metamethod_by_key_type(db, key_type, &instianted_operand, &return_type);

            match result {
                Ok(member_type) => {
                    if !member_type.is_nil() {
                        return Ok(member_type);
                    }
                }
                Err(InferFailReason::FieldNotFound) => {}
                Err(err) => return Err(err),
            }
        }
    }

    // for supers
    if let Some(supers) = infer_compilation_type_super_types(db, &type_decl_id)
        .or_else(|| db.get_type_index().get_super_types(&type_decl_id))
    {
        for super_type in supers {
            let result = infer_member_by_operator_key_type(
                db,
                cache,
                &instantiate_type_generic(db, &super_type, &substitutor),
                key_type,
                &infer_guard.fork(),
            );
            match result {
                Ok(member_type) => {
                    return Ok(member_type);
                }
                Err(InferFailReason::FieldNotFound) => {}
                Err(err) => return Err(err),
            }
        }
    }

    Err(InferFailReason::FieldNotFound)
}

fn infer_table_generic_member_by_key_type(
    db: &DbIndex,
    table_params: &[LuaType],
    key_type: &LuaType,
) -> InferResult {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    let table_key_type = &table_params[0];
    let value_type = &table_params[1];
    infer_index_metamethod_by_key_type(db, key_type, table_key_type, value_type)
}

fn infer_global_field_member(db: &DbIndex, key: &LuaMemberKey) -> InferResult {
    let name = key.get_name().ok_or(InferFailReason::None)?;
    infer_global_type(db, name)
}

fn infer_namespace_member(db: &DbIndex, ns: &str, key: &LuaMemberKey) -> InferResult {
    let member_key = match key {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(i) => i.to_string(),
        _ => return Err(InferFailReason::None),
    };

    let namespace_or_type_id = format!("{}.{}", ns, member_key);
    let type_id = LuaTypeDeclId::global(&namespace_or_type_id);
    if get_type_def_kind(db, &type_id).is_some() {
        return Ok(LuaType::Def(type_id));
    }

    Ok(LuaType::Namespace(
        SmolStr::new(namespace_or_type_id).into(),
    ))
}

fn collect_type_member_keys(db: &DbIndex, key_type: &LuaType, keys: &mut HashSet<LuaMemberKey>) {
    let mut stack = vec![key_type.clone()];
    let mut visited = HashSet::new();

    while let Some(current_type) = stack.pop() {
        if !visited.insert(current_type.clone()) {
            continue;
        }
        match &current_type {
            LuaType::StringConst(name) | LuaType::DocStringConst(name) => {
                keys.insert(LuaMemberKey::Name((**name).clone()));
            }
            LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
                keys.insert(LuaMemberKey::Integer(*i));
            }
            LuaType::Call(alias_call) => {
                if alias_call.get_call_kind() == LuaAliasCallKind::KeyOf {
                    let operands = alias_call.get_operands();
                    if operands.len() == 1 {
                        if let Some(members) = get_keyof_members(db, &operands[0]) {
                            keys.extend(members.into_iter().map(|member| member.key));
                        }
                    }
                }
            }
            LuaType::MultiLineUnion(multi_union) => {
                for (typ, _) in multi_union.get_unions() {
                    if !visited.contains(typ) {
                        stack.push(typ.clone());
                    }
                }
            }
            LuaType::Union(union_typ) => {
                for t in union_typ.into_vec() {
                    if !visited.contains(&t) {
                        stack.push(t.clone());
                    }
                }
            }
            LuaType::TableConst(_) | LuaType::Tuple(_) => {
                keys.insert(LuaMemberKey::ExprType(current_type.clone()));
            }
            LuaType::Ref(id) => {
                if get_type_def_kind(db, id).is_some() {
                    if type_def_is_alias(db, id) {
                        if let Some(origin_type) = type_def_alias_origin(db, id) {
                            if !visited.contains(&origin_type) {
                                stack.push(origin_type);
                            }
                            continue;
                        }
                    }
                    if type_def_is_enum(db, id)
                        && let Some(type_decl) = db.get_type_index().get_type_decl(id)
                    {
                        let owner = LuaMemberOwner::Type(id.clone());
                        if let Some(members) = get_members(db, &owner) {
                            let is_enum_key = type_decl.is_enum_key();
                            for member in members {
                                if is_enum_key {
                                    keys.insert(member.get_key().clone());
                                } else if let Some(typ) =
                                    get_type_by_owner(db, &member.get_id().into())
                                {
                                    match typ {
                                        LuaType::DocStringConst(s) | LuaType::StringConst(s) => {
                                            keys.insert(s.to_string().into());
                                        }
                                        LuaType::DocIntegerConst(i) | LuaType::IntegerConst(i) => {
                                            keys.insert(i.into());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn infer_tpl_ref_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic: &GenericTpl,
    lookup: &MemberLookupQuery,
    infer_guard: &InferGuardRef,
) -> InferResult {
    let extend_type = get_tpl_ref_extend_type(
        db,
        cache,
        &LuaType::TplRef(generic.clone().into()),
        lookup
            .index_expr
            .get_index_expr()
            .ok_or(InferFailReason::None)?
            .get_prefix_expr()
            .ok_or(InferFailReason::None)?,
        0,
    )
    .ok_or(InferFailReason::None)?;
    infer_member_by_lookup(db, cache, &extend_type, lookup, infer_guard)
}
