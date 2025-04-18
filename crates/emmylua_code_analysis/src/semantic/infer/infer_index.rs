use std::collections::HashSet;

use emmylua_parser::{
    LuaAstNode, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaIndexMemberExpr, PathTrait,
};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    db_index::{
        DbIndex, LuaGenericType, LuaIntersectionType, LuaMemberKey, LuaObjectType,
        LuaOperatorMetaMethod, LuaTupleType, LuaType, LuaTypeDeclId, LuaUnionType,
    },
    semantic::{
        generic::{instantiate_type_generic, TypeSubstitutor},
        member::get_buildin_type_map_type_id,
        type_check::{self, check_type_compact},
        InferGuard,
    },
    InFiled, LuaFlowId, LuaInferCache, LuaInstanceType, LuaMemberOwner, LuaOperatorOwner, TypeOps,
    VarRefId,
};

use super::{infer_expr, infer_name::infer_global_type, InferFailReason, InferResult};

pub fn infer_index_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: LuaIndexExpr,
) -> InferResult {
    let prefix_expr = index_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer_expr(db, cache, prefix_expr)?;
    let index_member_expr = LuaIndexMemberExpr::IndexExpr(index_expr.clone());

    let reason = match infer_member_by_member_key(
        db,
        cache,
        &prefix_type,
        index_member_expr.clone(),
        &mut InferGuard::new(),
    ) {
        Ok(member_type) => {
            return infer_member_type_pass_flow(db, cache, index_expr, &prefix_type, member_type);
        }
        Err(InferFailReason::FieldDotFound) => InferFailReason::FieldDotFound,
        Err(err) => return Err(err),
    };

    match infer_member_by_operator(
        db,
        cache,
        &prefix_type,
        index_member_expr,
        &mut InferGuard::new(),
    ) {
        Ok(member_type) => {
            return infer_member_type_pass_flow(db, cache, index_expr, &prefix_type, member_type)
        }
        Err(InferFailReason::FieldDotFound) => {}
        Err(err) => return Err(err),
    }

    Err(reason)
}

fn infer_member_type_pass_flow(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: LuaIndexExpr,
    prefix_type: &LuaType,
    mut member_type: LuaType,
) -> InferResult {
    let mut allow_reassign = true;
    match &prefix_type {
        // TODO: flow analysis should not generate corresponding `flow_chain` if the prefix type is an array
        LuaType::Array(_) => {
            return Ok(member_type.clone());
        }
        LuaType::Ref(decl_id) => {
            if let Some(members) = db
                .get_member_index()
                .get_members(&LuaMemberOwner::Type(decl_id.clone()))
            {
                if let Some(key) = index_expr.get_index_key() {
                    if let Some(_) = members
                        .iter()
                        .find(|m| m.get_key().to_path() == key.get_path_part())
                    {
                        allow_reassign = false
                    }
                }
            }
        }
        _ => {}
    }

    let access_path = match index_expr.get_access_path() {
        Some(path) => path,
        None => return Ok(member_type.clone()),
    };
    let var_ref_id = VarRefId::Name(SmolStr::new(&access_path));
    let flow_id = LuaFlowId::from_node(index_expr.syntax());
    let flow_chain = db
        .get_flow_index()
        .get_flow_chain(cache.get_file_id(), var_ref_id);
    if let Some(flow_chain) = flow_chain {
        let root = index_expr.get_root();
        for type_assert in flow_chain.get_type_asserts(index_expr.get_position(), flow_id) {
            let new_type = type_assert
                .tighten_type(db, cache, &root, member_type.clone())
                .unwrap_or(LuaType::Unknown);
            if type_assert.is_reassign() && !allow_reassign {
                // 允许仅去除 nil
                if member_type.is_nullable() && !new_type.is_nullable() {
                    member_type = new_type;
                }
                continue;
            }
            member_type = new_type;
        }
    }

    Ok(member_type)
}

pub fn infer_member_by_member_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
) -> InferResult {
    match &prefix_type {
        LuaType::Table | LuaType::Any | LuaType::Unknown => Ok(LuaType::Any),
        LuaType::TableConst(id) => infer_table_member(db, id.clone(), index_expr),
        LuaType::String | LuaType::Io | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            let decl_id =
                get_buildin_type_map_type_id(&prefix_type).ok_or(InferFailReason::None)?;
            infer_custom_type_member(db, cache, decl_id, index_expr, infer_guard)
        }
        LuaType::Ref(decl_id) => {
            infer_custom_type_member(db, cache, decl_id.clone(), index_expr, infer_guard)
        }
        LuaType::Def(decl_id) => {
            infer_custom_type_member(db, cache, decl_id.clone(), index_expr, infer_guard)
        }
        // LuaType::Module(_) => todo!(),
        LuaType::Tuple(tuple_type) => infer_tuple_member(tuple_type, index_expr),
        LuaType::Object(object_type) => infer_object_member(db, cache, object_type, index_expr),
        LuaType::Union(union_type) => infer_union_member(db, cache, union_type, index_expr),
        LuaType::Intersection(intersection_type) => {
            infer_intersection_member(db, cache, intersection_type, index_expr)
        }
        LuaType::Generic(generic_type) => infer_generic_member(db, cache, generic_type, index_expr),
        LuaType::Global => infer_global_field_member(db, cache, index_expr),
        LuaType::Instance(inst) => infer_instance_member(db, cache, inst, index_expr, infer_guard),
        LuaType::Namespace(ns) => infer_namespace_member(db, cache, ns, index_expr),
        LuaType::Array(array_type) => infer_array_member(db, cache, array_type, index_expr),
        _ => Err(InferFailReason::FieldDotFound),
    }
}

fn infer_array_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    array_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
) -> Result<LuaType, InferFailReason> {
    let key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let expression_type = if db.get_emmyrc().strict.array_index {
        TypeOps::Union.apply(array_type, &LuaType::Nil)
    } else {
        array_type.clone()
    };
    match key {
        LuaIndexKey::Integer(_) => Ok(expression_type),
        LuaIndexKey::Expr(expr) => {
            let expr_type = infer_expr(db, cache, expr.clone())?;
            if expr_type.is_integer() {
                Ok(expression_type)
            } else {
                Err(InferFailReason::FieldDotFound)
            }
        }
        _ => Err(InferFailReason::FieldDotFound),
    }
}

fn infer_table_member(
    db: &DbIndex,
    inst: InFiled<TextRange>,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let owner = LuaMemberOwner::Element(inst);
    let key: LuaMemberKey = index_expr
        .get_index_key()
        .ok_or(InferFailReason::None)?
        .into();
    let member_item = match db.get_member_index().get_member_item(&owner, &key) {
        Some(member_item) => member_item,
        None => return Err(InferFailReason::FieldDotFound),
    };

    member_item.resolve_type(db)
}

fn infer_custom_type_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type_id: LuaTypeDeclId,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
) -> InferResult {
    infer_guard.check(&prefix_type_id)?;
    let type_index = db.get_type_index();
    let type_decl = type_index
        .get_type_decl(&prefix_type_id)
        .ok_or(InferFailReason::None)?;
    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return infer_member_by_member_key(db, cache, &origin_type, index_expr, infer_guard);
        } else {
            return Err(InferFailReason::None);
        }
    }

    let owner = LuaMemberOwner::Type(prefix_type_id.clone());
    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let key: LuaMemberKey = index_key.clone().into();
    if let Some(member_item) = db.get_member_index().get_member_item(&owner, &key) {
        return member_item.resolve_type(db);
    }

    if type_decl.is_class() {
        if let Some(super_types) = type_index.get_super_types(&prefix_type_id) {
            for super_type in super_types {
                let result = infer_member_by_member_key(
                    db,
                    cache,
                    &super_type,
                    index_expr.clone(),
                    infer_guard,
                );

                match result {
                    Ok(member_type) => {
                        return Ok(member_type);
                    }
                    Err(InferFailReason::FieldDotFound) => {}
                    Err(err) => return Err(err),
                }
            }
        }
    }

    // 解决`key`为表达式的情况
    if let LuaIndexKey::Expr(expr) = index_key {
        if let Some(keys) = expr_to_member_key(db, cache, &expr) {
            let mut result_type = Vec::new();
            for key in keys {
                // 解决 enum[enum] | class[class] 的情况
                if let Some(member_type) = get_expr_key_members(db, &key, &owner) {
                    if !result_type.contains(&member_type) {
                        result_type.push(member_type);
                    }
                    continue;
                }

                if let Some(member_item) = db.get_member_index().get_member_item(&owner, &key) {
                    if let Ok(member_type) = member_item.resolve_type(db) {
                        if !result_type.contains(&member_type) {
                            result_type.push(member_type);
                        }
                    }
                }
            }
            match result_type.len() {
                0 => {}
                1 => return Ok(result_type[0].clone()),
                _ => return Ok(LuaType::Union(LuaUnionType::new(result_type).into())),
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn get_expr_key_members(
    db: &DbIndex,
    key: &LuaMemberKey,
    owner: &LuaMemberOwner,
) -> Option<LuaType> {
    let LuaMemberKey::Expr(LuaType::Ref(index_id)) = key else {
        return None;
    };
    let index_type_decl = db.get_type_index().get_type_decl(index_id)?;
    let mut result = Vec::new();

    let origin_type = if index_type_decl.is_alias() {
        index_type_decl.get_alias_origin(db, None)?
    } else {
        LuaType::Ref(index_id.clone())
    };

    if let Some(member_keys) = get_all_member_key(db, &origin_type) {
        for key in member_keys {
            if let Some(member_item) = db.get_member_index().get_member_item(&owner, &key) {
                if let Ok(member_type) = member_item.resolve_type(db) {
                    if !result.contains(&member_type) {
                        result.push(member_type);
                    }
                }
            }
        }
    }

    return match result.len() {
        0 => None,
        1 => Some(result[0].clone()),
        _ => Some(LuaType::Union(LuaUnionType::new(result).into())),
    };
}

fn get_all_member_key(db: &DbIndex, origin_type: &LuaType) -> Option<Vec<LuaMemberKey>> {
    let mut result = Vec::new();
    let mut stack = vec![origin_type]; // 堆栈用于迭代处理
    let mut visited = HashSet::new();

    while let Some(current_type) = stack.pop() {
        if visited.contains(current_type) {
            continue;
        }
        visited.insert(current_type.clone());
        match current_type {
            LuaType::MultiLineUnion(types) => {
                for (typ, _) in types.get_unions() {
                    match typ {
                        LuaType::DocStringConst(s) | LuaType::StringConst(s) => {
                            result.push((*s).to_string().into());
                        }
                        LuaType::DocIntegerConst(i) | LuaType::IntegerConst(i) => {
                            result.push((*i).into());
                        }
                        LuaType::Ref(_) => {
                            stack.push(typ); // 将 Ref 类型推入堆栈进一步处理
                        }
                        _ => {}
                    }
                }
            }
            LuaType::Union(union_type) => {
                for typ in union_type.get_types() {
                    if let LuaType::Ref(_) = typ {
                        stack.push(typ); // 推入堆栈
                    }
                }
            }
            LuaType::Ref(id) => {
                if let Some(type_decl) = db.get_type_index().get_type_decl(id) {
                    if type_decl.is_enum() {
                        let owner = LuaMemberOwner::Type(id.clone());
                        if let Some(members) = db.get_member_index().get_members(&owner) {
                            let is_enum_key = type_decl.is_enum_key();
                            for member in members {
                                if is_enum_key {
                                    result.push(member.get_key().clone());
                                } else {
                                    if let Some(typ) = db
                                        .get_type_index()
                                        .get_type_cache(&member.get_id().into())
                                        .map(|it| it.as_type())
                                    {
                                        match typ {
                                            LuaType::DocStringConst(s)
                                            | LuaType::StringConst(s) => {
                                                result.push((*s).to_string().into());
                                            }
                                            LuaType::DocIntegerConst(i)
                                            | LuaType::IntegerConst(i) => {
                                                result.push((*i).into());
                                            }
                                            _ => {}
                                        }
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

    Some(result)
}

fn infer_tuple_member(tuple_type: &LuaTupleType, index_expr: LuaIndexMemberExpr) -> InferResult {
    let key = index_expr
        .get_index_key()
        .ok_or(InferFailReason::None)?
        .into();
    if let LuaMemberKey::Integer(i) = key {
        let index = if i > 0 { i - 1 } else { 0 };
        return match tuple_type.get_type(index as usize) {
            Some(typ) => Ok(typ.clone()),
            None => Err(InferFailReason::FieldDotFound),
        };
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_object_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    object_type: &LuaObjectType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    if let Some(member_type) = object_type.get_field(&member_key.clone().into()) {
        return Ok(member_type.clone());
    }

    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    // todo
    let index_accesses = object_type.get_index_access();
    for (key, value) in index_accesses {
        let result = infer_index_metamethod(db, cache, &index_key, &key, value);
        match result {
            Ok(typ) => {
                return Ok(typ);
            }
            Err(InferFailReason::FieldDotFound) => {}
            Err(err) => {
                return Err(err);
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_index_metamethod(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_key: &LuaIndexKey,
    key_type: &LuaType,
    value_type: &LuaType,
) -> InferResult {
    let access_key_type = match &index_key {
        LuaIndexKey::Name(name) => LuaType::StringConst(SmolStr::new(name.get_name_text()).into()),
        LuaIndexKey::String(s) => LuaType::StringConst(SmolStr::new(s.get_value()).into()),
        LuaIndexKey::Integer(i) => LuaType::IntegerConst(i.get_int_value()),
        LuaIndexKey::Idx(i) => LuaType::IntegerConst(*i as i64),
        LuaIndexKey::Expr(expr) => infer_expr(db, cache, expr.clone())?,
    };

    if check_type_compact(db, key_type, &access_key_type).is_ok() {
        return Ok(value_type.clone());
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_union_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union_type: &LuaUnionType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let mut member_types = Vec::new();
    for sub_type in union_type.get_types() {
        let result = infer_member_by_member_key(
            db,
            cache,
            sub_type,
            index_expr.clone(),
            &mut InferGuard::new(),
        );
        match result {
            Ok(typ) => {
                if !typ.is_nil() {
                    member_types.push(typ);
                }
            }
            _ => {}
        }
    }

    member_types.dedup();
    match member_types.len() {
        0 => Ok(LuaType::Nil),
        1 => Ok(member_types[0].clone()),
        _ => Ok(LuaType::Union(LuaUnionType::new(member_types).into())),
    }
}

fn infer_intersection_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    intersection_type: &LuaIntersectionType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let mut member_type = LuaType::Nil;
    for member in intersection_type.get_types() {
        let sub_member_type = infer_member_by_member_key(
            db,
            cache,
            member,
            index_expr.clone(),
            &mut InferGuard::new(),
        )?;
        if member_type.is_nil() {
            member_type = sub_member_type;
        } else if member_type != sub_member_type {
            return Ok(LuaType::Nil);
        }
    }

    Ok(LuaType::Nil)
}

fn infer_generic_members_from_super_generics(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    type_decl_id: &LuaTypeDeclId,
    substitutor: &TypeSubstitutor,
    index_expr: LuaIndexMemberExpr,
) -> Option<LuaType> {
    let type_index = db.get_type_index();

    let type_decl = type_index.get_type_decl(&type_decl_id)?;
    if !type_decl.is_class() {
        return None;
    };

    let type_decl_id = type_decl.get_id();
    if let Some(super_types) = type_index.get_super_types(&type_decl_id) {
        super_types.iter().find_map(|super_type| {
            let super_type = instantiate_type_generic(db, &super_type, &substitutor);
            infer_member_by_member_key(
                db,
                cache,
                &super_type,
                index_expr.clone(),
                &mut InferGuard::new(),
            )
            .ok()
        })
    } else {
        return None;
    }
}

fn infer_generic_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic_type: &LuaGenericType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let base_type = generic_type.get_base_type();

    let generic_params = generic_type.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());

    // TODO: this is just a hack to support inheritance from the generic objects
    // like `---@class box<T>: T`. Should be rewritten: generic types should
    // be passed to the called instantiate_type_generic() in some kind of a
    // context.
    if let LuaType::Ref(base_type_decl_id) = &base_type {
        let result = infer_generic_members_from_super_generics(
            db,
            cache,
            base_type_decl_id,
            &substitutor,
            index_expr.clone(),
        );
        if let Some(result) = result {
            return Ok(result);
        }
    }

    let member_type =
        infer_member_by_member_key(db, cache, &base_type, index_expr, &mut InferGuard::new())?;

    Ok(instantiate_type_generic(db, &member_type, &substitutor))
}

fn infer_instance_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    inst: &LuaInstanceType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
) -> InferResult {
    let range = inst.get_range();

    let origin_type = inst.get_base();
    let base_result =
        infer_member_by_member_key(db, cache, &origin_type, index_expr.clone(), infer_guard);
    match base_result {
        Ok(typ) => {
            return Ok(typ);
        }
        Err(InferFailReason::FieldDotFound) => {}
        Err(err) => return Err(err),
    }

    infer_table_member(db, range.clone(), index_expr.clone())
}

pub fn infer_member_by_operator(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
) -> InferResult {
    match &prefix_type {
        LuaType::TableConst(in_filed) => {
            infer_member_by_index_table(db, cache, in_filed, index_expr)
        }
        LuaType::Ref(decl_id) => {
            infer_member_by_index_custom_type(db, cache, decl_id, index_expr, infer_guard)
        }
        LuaType::Def(decl_id) => {
            infer_member_by_index_custom_type(db, cache, decl_id, index_expr, infer_guard)
        }
        // LuaType::Module(arc) => todo!(),
        LuaType::Array(base) => infer_member_by_index_array(db, cache, base, index_expr),
        LuaType::Object(object) => infer_member_by_index_object(db, cache, object, index_expr),
        LuaType::Union(union) => infer_member_by_index_union(db, cache, union, index_expr),
        LuaType::Intersection(intersection) => {
            infer_member_by_index_intersection(db, cache, intersection, index_expr)
        }
        LuaType::Generic(generic) => infer_member_by_index_generic(db, cache, generic, index_expr),
        LuaType::TableGeneric(table_generic) => {
            infer_member_by_index_table_generic(db, cache, table_generic, index_expr)
        }
        LuaType::Instance(inst) => {
            let base = inst.get_base();
            infer_member_by_operator(db, cache, &base, index_expr, infer_guard)
        }
        _ => Err(InferFailReason::FieldDotFound),
    }
}

fn infer_member_by_index_table(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table_range: &InFiled<TextRange>,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let metatable = db.get_metatable_index().get(table_range);
    match metatable {
        Some(metatable) => {
            let meta_owner = LuaOperatorOwner::Table(metatable.clone());
            let operator_ids = db
                .get_operator_index()
                .get_operators(&meta_owner, LuaOperatorMetaMethod::Index)
                .ok_or(InferFailReason::FieldDotFound)?;

            let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;

            for operator_id in operator_ids {
                let operator = db
                    .get_operator_index()
                    .get_operator(operator_id)
                    .ok_or(InferFailReason::None)?;
                let operand = operator.get_operand(db);
                let return_type = operator.get_result(db)?;
                let typ = infer_index_metamethod(db, cache, &index_key, &operand, &return_type)?;
                return Ok(typ);
            }
        }
        None => {
            let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
            if let LuaIndexKey::Expr(expr) = index_key {
                let key_type = infer_expr(db, cache, expr.clone())?;
                let members = db
                    .get_member_index()
                    .get_members(&LuaMemberOwner::Element(table_range.clone()));
                if let Some(members) = members {
                    let mut result_type = LuaType::Unknown;
                    for member in members {
                        let member_key_type = match member.get_key() {
                            LuaMemberKey::Name(s) => LuaType::StringConst(s.clone().into()),
                            LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
                            _ => continue,
                        };
                        if check_type_compact(db, &key_type, &member_key_type).is_ok() {
                            let member_type = db
                                .get_type_index()
                                .get_type_cache(&member.get_id().into())
                                .map(|it| it.as_type())
                                .unwrap_or(&LuaType::Unknown);

                            result_type = TypeOps::Union.apply(&result_type, member_type);
                        }
                    }

                    if !result_type.is_unknown() {
                        if matches!(
                            key_type,
                            LuaType::String | LuaType::Number | LuaType::Integer
                        ) {
                            result_type = TypeOps::Union.apply(&result_type, &LuaType::Nil);
                        }

                        return Ok(result_type);
                    }
                }
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_member_by_index_custom_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type_id: &LuaTypeDeclId,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
) -> InferResult {
    infer_guard.check(&prefix_type_id)?;
    let type_index = db.get_type_index();
    let type_decl = type_index
        .get_type_decl(&prefix_type_id)
        .ok_or(InferFailReason::None)?;
    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return infer_member_by_operator(db, cache, &origin_type, index_expr, infer_guard);
        }
        return Err(InferFailReason::None);
    }

    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    if let Some(index_operator_ids) = db
        .get_operator_index()
        .get_operators(&prefix_type_id.clone().into(), LuaOperatorMetaMethod::Index)
    {
        for operator_id in index_operator_ids {
            let operator = db
                .get_operator_index()
                .get_operator(operator_id)
                .ok_or(InferFailReason::None)?;
            let operand = operator.get_operand(db);
            let return_type = operator.get_result(db)?;
            let typ = infer_index_metamethod(db, cache, &index_key, &operand, &return_type);
            if let Ok(typ) = typ {
                return Ok(typ);
            }
        }
    }

    // find member by key in super
    if type_decl.is_class() {
        if let Some(super_types) = type_index.get_super_types(&prefix_type_id) {
            for super_type in super_types {
                let result = infer_member_by_operator(
                    db,
                    cache,
                    &super_type,
                    index_expr.clone(),
                    infer_guard,
                );
                match result {
                    Ok(member_type) => {
                        return Ok(member_type);
                    }
                    Err(InferFailReason::FieldDotFound) => {}
                    Err(err) => return Err(err),
                }
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_member_by_index_array(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    base: &LuaType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let expression_type = if db.get_emmyrc().strict.array_index {
        TypeOps::Union.apply(base, &LuaType::Nil)
    } else {
        base.clone()
    };
    if member_key.is_integer() {
        return Ok(expression_type);
    } else if member_key.is_expr() {
        let expr = member_key.get_expr().ok_or(InferFailReason::None)?;
        let expr_type = infer_expr(db, cache, expr.clone())?;
        if check_type_compact(db, &LuaType::Number, &expr_type).is_ok() {
            return Ok(expression_type);
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_member_by_index_object(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    object: &LuaObjectType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let access_member_type = object.get_index_access();
    if member_key.is_expr() {
        let expr = member_key.get_expr().ok_or(InferFailReason::None)?;
        let expr_type = infer_expr(db, cache, expr.clone())?;
        for (key, field) in access_member_type {
            if type_check::check_type_compact(db, key, &expr_type).is_ok() {
                return Ok(field.clone());
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_member_by_index_union(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union: &LuaUnionType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let mut member_type = LuaType::Unknown;
    for member in union.get_types() {
        let result = infer_member_by_operator(
            db,
            cache,
            member,
            index_expr.clone(),
            &mut InferGuard::new(),
        );
        match result {
            Ok(typ) => {
                member_type = TypeOps::Union.apply(&member_type, &typ);
            }
            Err(InferFailReason::FieldDotFound) => {}
            Err(err) => {
                return Err(err);
            }
        }
    }

    if member_type.is_unknown() {
        return Err(InferFailReason::FieldDotFound);
    }

    Ok(member_type)
}

fn infer_member_by_index_intersection(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    intersection: &LuaIntersectionType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let mut member_type = LuaType::Unknown;
    for member in intersection.get_types() {
        let sub_member_type = infer_member_by_operator(
            db,
            cache,
            member,
            index_expr.clone(),
            &mut InferGuard::new(),
        )?;
        if member_type.is_unknown() {
            member_type = sub_member_type;
        } else if member_type != sub_member_type {
            return Err(InferFailReason::FieldDotFound);
        }
    }

    if member_type.is_unknown() {
        return Err(InferFailReason::FieldDotFound);
    }

    Ok(member_type)
}

fn infer_member_by_index_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic: &LuaGenericType,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let base_type = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base_type {
        id
    } else {
        return Err(InferFailReason::None);
    };
    let generic_params = generic.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());
    let type_index = db.get_type_index();
    let type_decl = type_index
        .get_type_decl(&type_decl_id)
        .ok_or(InferFailReason::None)?;
    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, Some(&substitutor)) {
            return infer_member_by_operator(
                db,
                cache,
                &instantiate_type_generic(db, &origin_type, &substitutor),
                index_expr.clone(),
                &mut InferGuard::new(),
            );
        }
        return Err(InferFailReason::None);
    }

    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let operator_index = db.get_operator_index();
    if let Some(index_operator_ids) =
        operator_index.get_operators(&type_decl_id.clone().into(), LuaOperatorMetaMethod::Index)
    {
        for index_operator_id in index_operator_ids {
            let index_operator = operator_index
                .get_operator(index_operator_id)
                .ok_or(InferFailReason::None)?;
            let operand = index_operator.get_operand(db);
            let instianted_operand = instantiate_type_generic(db, &operand, &substitutor);
            let return_type =
                instantiate_type_generic(db, &index_operator.get_result(db)?, &substitutor);

            let result =
                infer_index_metamethod(db, cache, &member_key, &instianted_operand, &return_type);

            match result {
                Ok(member_type) => {
                    if !member_type.is_nil() {
                        return Ok(member_type);
                    }
                }
                Err(InferFailReason::FieldDotFound) => {}
                Err(err) => return Err(err),
            }
        }
    }

    // for supers
    if let Some(supers) = type_index.get_super_types(&type_decl_id) {
        for super_type in supers {
            let result = infer_member_by_operator(
                db,
                cache,
                &instantiate_type_generic(db, &super_type, &substitutor),
                index_expr.clone(),
                &mut InferGuard::new(),
            );
            match result {
                Ok(member_type) => {
                    return Ok(member_type);
                }
                Err(InferFailReason::FieldDotFound) => {}
                Err(err) => return Err(err),
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_member_by_index_table_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table_params: &Vec<LuaType>,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let key_type = &table_params[0];
    let value_type = &table_params[1];
    infer_index_metamethod(db, cache, &index_key, key_type, value_type)
}

fn infer_global_field_member(
    db: &DbIndex,
    _: &LuaInferCache,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let name = member_key
        .get_name()
        .ok_or(InferFailReason::None)?
        .get_name_text();
    infer_global_type(db, name)
}

fn infer_namespace_member(
    db: &DbIndex,
    _: &LuaInferCache,
    ns: &str,
    index_expr: LuaIndexMemberExpr,
) -> InferResult {
    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let member_key = match member_key.into() {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(i) => i.to_string(),
        _ => return Err(InferFailReason::None),
    };

    let namespace_or_type_id = format!("{}.{}", ns, member_key);
    let type_id = LuaTypeDeclId::new(&namespace_or_type_id);
    if db.get_type_index().get_type_decl(&type_id).is_some() {
        return Ok(LuaType::Def(type_id));
    }

    Ok(LuaType::Namespace(
        SmolStr::new(namespace_or_type_id).into(),
    ))
}

fn expr_to_member_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: &LuaExpr,
) -> Option<HashSet<LuaMemberKey>> {
    let expr_type = infer_expr(db, cache, expr.clone()).ok()?;
    let mut keys: HashSet<LuaMemberKey> = HashSet::new();
    let mut stack = vec![expr_type.clone()];

    while let Some(current_type) = stack.pop() {
        match &current_type {
            LuaType::StringConst(name) | LuaType::DocStringConst(name) => {
                keys.insert(name.as_ref().to_string().into());
            }
            LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
                keys.insert((*i).into());
            }
            LuaType::Union(union_typ) => {
                for t in union_typ.get_types() {
                    stack.push(t.clone());
                }
            }
            LuaType::TableConst(_) => {
                keys.insert(LuaMemberKey::Expr(expr_type.clone()));
            }
            LuaType::Ref(id) => {
                if let Some(type_decl) = db.get_type_index().get_type_decl(id) {
                    if type_decl.is_enum() || type_decl.is_alias() {
                        keys.insert(LuaMemberKey::Expr(current_type.clone()));
                    }
                }
            }
            _ => {}
        }
    }
    Some(keys)
}
