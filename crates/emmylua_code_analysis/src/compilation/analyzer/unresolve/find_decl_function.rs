use emmylua_parser::{LuaAstNode, LuaIndexKey, LuaIndexMemberExpr};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    check_type_compact,
    db_index::{
        DbIndex, LuaGenericType, LuaIntersectionType, LuaMemberKey, LuaObjectType,
        LuaOperatorMetaMethod, LuaTupleType, LuaType, LuaTypeDeclId, LuaUnionType,
    },
    infer_expr, instantiate_type_generic,
    semantic::InferGuard,
    InFiled, InferFailReason, LuaInferCache, LuaInstanceType, LuaMemberId, LuaMemberOwner,
    LuaOperatorOwner, TypeOps, TypeSubstitutor,
};

type FunctionTypeResult = Result<LuaType, InferFailReason>;

pub type FindFunctionResult = Result<FindFunctionType, InferFailReason>;

#[derive(Debug)]
pub struct FindFunctionType {
    pub typ: LuaType,
    pub is_current_owner: bool,
}

#[derive(Debug)]
struct DeepGuard {
    deep: usize,
}

impl DeepGuard {
    pub fn new() -> Self {
        Self { deep: 0 }
    }
    pub fn next(&mut self) {
        self.deep += 1;
    }
    pub fn get(&self) -> usize {
        self.deep
    }
}

fn get_member_id(cache: &mut LuaInferCache, index_member_expr: &LuaIndexMemberExpr) -> LuaMemberId {
    let file_id = cache.get_file_id();
    match index_member_expr {
        LuaIndexMemberExpr::IndexExpr(index_expr) => {
            let syntax_id = index_expr.get_syntax_id();
            LuaMemberId::new(syntax_id, file_id)
        }
        LuaIndexMemberExpr::TableField(table_field) => {
            let syntax_id = table_field.get_syntax_id();
            LuaMemberId::new(syntax_id, file_id)
        }
    }
}

/// 寻找声明的函数类型(排除自身), 假设目标具有多个声明, 那么将返回一个联合类型
pub fn find_decl_function_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_member_expr: LuaIndexMemberExpr,
) -> FindFunctionResult {
    index_member_expr
        .get_prefix_expr()
        .ok_or(InferFailReason::None)?;
    let mut deep_guard = DeepGuard::new();
    let reason = match find_function_type_by_member_key(
        db,
        cache,
        &prefix_type,
        index_member_expr.clone(),
        &mut InferGuard::new(),
        &mut deep_guard,
    ) {
        Ok(member_type) => {
            return Ok(FindFunctionType {
                typ: member_type,
                is_current_owner: deep_guard.get() == 0,
            });
        }
        Err(InferFailReason::FieldDotFound) => InferFailReason::FieldDotFound,
        Err(err) => return Err(err),
    };

    let mut deep_guard = DeepGuard::new();
    match find_function_type_by_operator(
        db,
        cache,
        &prefix_type,
        index_member_expr,
        &mut InferGuard::new(),
        &mut deep_guard,
    ) {
        Ok(member_type) => {
            return Ok(FindFunctionType {
                typ: member_type,
                is_current_owner: deep_guard.get() == 0,
            });
        }
        Err(InferFailReason::FieldDotFound) => {}
        Err(err) => return Err(err),
    }

    Err(reason)
}

fn find_function_type_by_member_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    match &prefix_type {
        LuaType::Ref(decl_id) => find_custom_type_function_member(
            db,
            cache,
            decl_id.clone(),
            index_expr,
            infer_guard,
            deep_guard,
        ),
        LuaType::Def(decl_id) => find_custom_type_function_member(
            db,
            cache,
            decl_id.clone(),
            index_expr,
            infer_guard,
            deep_guard,
        ),
        LuaType::Tuple(tuple_type) => find_tuple_function_member(db, cache, tuple_type, index_expr),
        LuaType::Object(object_type) => {
            find_object_function_member(db, cache, object_type, index_expr)
        }
        LuaType::Union(union_type) => {
            find_union_function_member(db, cache, union_type, index_expr, deep_guard)
        }
        LuaType::Generic(generic_type) => {
            find_generic_member(db, cache, generic_type, index_expr, deep_guard)
        }
        LuaType::Instance(inst) => {
            find_instance_member_decl_type(db, cache, inst, index_expr, infer_guard, deep_guard)
        }
        LuaType::Namespace(ns) => infer_namespace_member_decl_type(db, cache, ns, index_expr),
        LuaType::Array(array_type) => find_array_function(db, cache, array_type, index_expr),
        _ => Err(InferFailReason::FieldDotFound),
    }
}

fn find_array_function(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    array_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
) -> FunctionTypeResult {
    let key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    match key {
        LuaIndexKey::Integer(_) => Ok(array_type.clone()),
        LuaIndexKey::Expr(expr) => {
            let expr_type = infer_expr(db, cache, expr.clone())?;
            if expr_type.is_integer() {
                Ok(array_type.clone())
            } else {
                Err(InferFailReason::FieldDotFound)
            }
        }
        _ => Err(InferFailReason::FieldDotFound),
    }
}

fn find_custom_type_function_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type_id: LuaTypeDeclId,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    infer_guard.check(&prefix_type_id)?;
    let type_index = db.get_type_index();
    let type_decl = type_index
        .get_type_decl(&prefix_type_id)
        .ok_or(InferFailReason::None)?;
    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return find_function_type_by_member_key(
                db,
                cache,
                &origin_type,
                index_expr,
                infer_guard,
                deep_guard,
            );
        } else {
            return Err(InferFailReason::None);
        }
    }

    let owner = LuaMemberOwner::Type(prefix_type_id.clone());
    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let key = LuaMemberKey::from_index_key(db, cache, &index_key)?;
    if let Some(member_item) = db.get_member_index().get_member_item(&owner, &key) {
        let index_member_id = get_member_id(cache, &index_expr);
        let mut result_type = LuaType::Unknown;
        for member_id in member_item.get_member_ids() {
            if index_member_id != member_id {
                match db.get_type_index().get_type_cache(&member_id.into()) {
                    Some(type_cache) => {
                        result_type = TypeOps::Union.apply(db, &result_type, &type_cache.as_type());
                    }
                    None => {}
                }
            }
        }
        if !result_type.is_unknown() {
            return Ok(result_type);
        }
    }

    if type_decl.is_class() {
        if let Some(super_types) = type_index.get_super_types(&prefix_type_id) {
            deep_guard.next();
            for super_type in super_types {
                let result = find_function_type_by_member_key(
                    db,
                    cache,
                    &super_type,
                    index_expr.clone(),
                    infer_guard,
                    deep_guard,
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

fn find_tuple_function_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    tuple_type: &LuaTupleType,
    index_expr: LuaIndexMemberExpr,
) -> FunctionTypeResult {
    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let key = LuaMemberKey::from_index_key(db, cache, &index_key)?;
    if let LuaMemberKey::Integer(i) = key {
        let index = if i > 0 { i - 1 } else { 0 };
        return match tuple_type.get_type(index as usize) {
            Some(typ) => Ok(typ.clone()),
            None => Err(InferFailReason::FieldDotFound),
        };
    }

    Err(InferFailReason::FieldDotFound)
}

fn find_object_function_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    object_type: &LuaObjectType,
    index_expr: LuaIndexMemberExpr,
) -> FunctionTypeResult {
    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let member_key = LuaMemberKey::from_index_key(db, cache, &index_key)?;
    if let Some(member_type) = object_type.get_field(&member_key) {
        return Ok(member_type.clone());
    }

    // todo
    let index_accesses = object_type.get_index_access();
    for (key, value) in index_accesses {
        let result = find_index_metamethod(db, cache, &index_key, &key, value);
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

fn find_index_metamethod(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_key: &LuaIndexKey,
    key_type: &LuaType,
    value_type: &LuaType,
) -> FunctionTypeResult {
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

fn find_union_function_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union_type: &LuaUnionType,
    index_expr: LuaIndexMemberExpr,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    let mut member_types = Vec::new();
    for sub_type in union_type.get_types() {
        let result = find_function_type_by_member_key(
            db,
            cache,
            sub_type,
            index_expr.clone(),
            &mut InferGuard::new(),
            deep_guard,
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

fn index_generic_members_from_super_generics(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    type_decl_id: &LuaTypeDeclId,
    substitutor: &TypeSubstitutor,
    index_expr: LuaIndexMemberExpr,
    deep_guard: &mut DeepGuard,
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
            find_function_type_by_member_key(
                db,
                cache,
                &super_type,
                index_expr.clone(),
                &mut InferGuard::new(),
                deep_guard,
            )
            .ok()
        })
    } else {
        return None;
    }
}

fn find_generic_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic_type: &LuaGenericType,
    index_expr: LuaIndexMemberExpr,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    let base_type = generic_type.get_base_type();

    let generic_params = generic_type.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());
    if let LuaType::Ref(base_type_decl_id) = &base_type {
        let result = index_generic_members_from_super_generics(
            db,
            cache,
            base_type_decl_id,
            &substitutor,
            index_expr.clone(),
            deep_guard,
        );
        if let Some(result) = result {
            return Ok(result);
        }
    }

    let member_type = find_function_type_by_member_key(
        db,
        cache,
        &base_type,
        index_expr,
        &mut InferGuard::new(),
        deep_guard,
    )?;

    Ok(instantiate_type_generic(db, &member_type, &substitutor))
}

fn find_instance_member_decl_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    inst: &LuaInstanceType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    let origin_type = inst.get_base();
    find_function_type_by_member_key(
        db,
        cache,
        &origin_type,
        index_expr.clone(),
        infer_guard,
        deep_guard,
    )
}

fn find_function_type_by_operator(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    match &prefix_type {
        LuaType::TableConst(in_filed) => {
            find_member_by_index_table(db, cache, in_filed, index_expr)
        }
        LuaType::Ref(decl_id) => find_member_by_index_custom_type(
            db,
            cache,
            decl_id,
            index_expr,
            infer_guard,
            deep_guard,
        ),
        LuaType::Def(decl_id) => find_member_by_index_custom_type(
            db,
            cache,
            decl_id,
            index_expr,
            infer_guard,
            deep_guard,
        ),
        // LuaType::Module(arc) => todo!(),
        LuaType::Array(base) => infer_member_by_index_array(db, cache, base, index_expr),
        LuaType::Object(object) => infer_member_by_index_object(db, cache, object, index_expr),
        LuaType::Union(union) => {
            find_member_by_index_union(db, cache, union, index_expr, deep_guard)
        }
        LuaType::Intersection(intersection) => {
            find_member_by_index_intersection(db, cache, intersection, index_expr, deep_guard)
        }
        LuaType::Generic(generic) => {
            find_member_by_index_generic(db, cache, generic, index_expr, deep_guard)
        }
        LuaType::TableGeneric(table_generic) => {
            find_member_by_index_table_generic(db, cache, table_generic, index_expr)
        }
        LuaType::Instance(inst) => {
            let base = inst.get_base();
            find_function_type_by_operator(db, cache, &base, index_expr, infer_guard, deep_guard)
        }
        _ => Err(InferFailReason::FieldDotFound),
    }
}

fn find_member_by_index_table(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table_range: &InFiled<TextRange>,
    index_expr: LuaIndexMemberExpr,
) -> FunctionTypeResult {
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
                let typ = find_index_metamethod(db, cache, &index_key, &operand, &return_type)?;
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

                            result_type = TypeOps::Union.apply(db, &result_type, member_type);
                        }
                    }

                    if !result_type.is_unknown() {
                        if matches!(
                            key_type,
                            LuaType::String | LuaType::Number | LuaType::Integer
                        ) {
                            result_type = TypeOps::Union.apply(db, &result_type, &LuaType::Nil);
                        }

                        return Ok(result_type);
                    }
                }
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn find_member_by_index_custom_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type_id: &LuaTypeDeclId,
    index_expr: LuaIndexMemberExpr,
    infer_guard: &mut InferGuard,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    infer_guard.check(&prefix_type_id)?;
    let type_index = db.get_type_index();
    let type_decl = type_index
        .get_type_decl(&prefix_type_id)
        .ok_or(InferFailReason::None)?;
    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return find_function_type_by_operator(
                db,
                cache,
                &origin_type,
                index_expr,
                infer_guard,
                deep_guard,
            );
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
            let typ = find_index_metamethod(db, cache, &index_key, &operand, &return_type);
            if let Ok(typ) = typ {
                return Ok(typ);
            }
        }
    }

    // find member by key in super
    if type_decl.is_class() {
        if let Some(super_types) = type_index.get_super_types(&prefix_type_id) {
            deep_guard.next();
            for super_type in super_types {
                let result = find_function_type_by_operator(
                    db,
                    cache,
                    &super_type,
                    index_expr.clone(),
                    infer_guard,
                    deep_guard,
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
) -> FunctionTypeResult {
    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    if member_key.is_integer() {
        return Ok(base.clone());
    } else if member_key.is_expr() {
        let expr = member_key.get_expr().ok_or(InferFailReason::None)?;
        let expr_type = infer_expr(db, cache, expr.clone())?;
        if check_type_compact(db, &LuaType::Number, &expr_type).is_ok() {
            return Ok(base.clone());
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn infer_member_by_index_object(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    object: &LuaObjectType,
    index_expr: LuaIndexMemberExpr,
) -> FunctionTypeResult {
    let member_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let access_member_type = object.get_index_access();
    if member_key.is_expr() {
        let expr = member_key.get_expr().ok_or(InferFailReason::None)?;
        let expr_type = infer_expr(db, cache, expr.clone())?;
        for (key, field) in access_member_type {
            if check_type_compact(db, key, &expr_type).is_ok() {
                return Ok(field.clone());
            }
        }
    }

    Err(InferFailReason::FieldDotFound)
}

fn find_member_by_index_union(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union: &LuaUnionType,
    index_expr: LuaIndexMemberExpr,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    let mut member_type = LuaType::Unknown;
    for member in union.get_types() {
        let result = find_function_type_by_operator(
            db,
            cache,
            member,
            index_expr.clone(),
            &mut InferGuard::new(),
            deep_guard,
        );
        match result {
            Ok(typ) => {
                member_type = TypeOps::Union.apply(db, &member_type, &typ);
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

fn find_member_by_index_intersection(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    intersection: &LuaIntersectionType,
    index_expr: LuaIndexMemberExpr,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
    for member in intersection.get_types() {
        match find_function_type_by_operator(
            db,
            cache,
            member,
            index_expr.clone(),
            &mut InferGuard::new(),
            deep_guard,
        ) {
            Ok(ty) => return Ok(ty),
            Err(InferFailReason::FieldDotFound) => {
                continue;
            }
            Err(reason) => return Err(reason),
        };
    }

    Err(InferFailReason::FieldDotFound)
}

fn find_member_by_index_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic: &LuaGenericType,
    index_expr: LuaIndexMemberExpr,
    deep_guard: &mut DeepGuard,
) -> FunctionTypeResult {
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
            return find_function_type_by_operator(
                db,
                cache,
                &instantiate_type_generic(db, &origin_type, &substitutor),
                index_expr.clone(),
                &mut InferGuard::new(),
                deep_guard,
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
                find_index_metamethod(db, cache, &member_key, &instianted_operand, &return_type);

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
            let result = find_function_type_by_operator(
                db,
                cache,
                &instantiate_type_generic(db, &super_type, &substitutor),
                index_expr.clone(),
                &mut InferGuard::new(),
                deep_guard,
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

fn find_member_by_index_table_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table_params: &Vec<LuaType>,
    index_expr: LuaIndexMemberExpr,
) -> FunctionTypeResult {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let key_type = &table_params[0];
    let value_type = &table_params[1];
    find_index_metamethod(db, cache, &index_key, key_type, value_type)
}

fn infer_namespace_member_decl_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    ns: &str,
    index_expr: LuaIndexMemberExpr,
) -> FunctionTypeResult {
    let index_key = index_expr.get_index_key().ok_or(InferFailReason::None)?;
    let member_key = LuaMemberKey::from_index_key(db, cache, &index_key)?;
    let member_key = match member_key {
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
