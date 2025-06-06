use std::ops::Deref;

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaCallArgList, LuaCallExpr, LuaExpr, LuaIndexMemberExpr,
    LuaLiteralToken, LuaLocalStat, LuaTableExpr, LuaTableField,
};

use crate::{
    db_index::{DbIndex, LuaType},
    infer_call_expr_func, infer_expr, InferGuard, LuaDeclId, LuaInferCache, LuaMemberId,
    LuaTupleStatus, LuaTupleType, VariadicType,
};

use super::{
    infer_index::{infer_member_by_member_key, infer_member_by_operator},
    InferFailReason, InferResult,
};

pub fn infer_table_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table: LuaTableExpr,
) -> InferResult {
    if table.is_array() {
        return infer_table_tuple_or_array(db, cache, table);
    }

    Ok(LuaType::TableConst(crate::InFiled {
        file_id: cache.get_file_id(),
        value: table.get_range(),
    }))
}

fn infer_table_tuple_or_array(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table: LuaTableExpr,
) -> InferResult {
    let fields = table.get_fields().collect::<Vec<_>>();
    if fields.len() > 50 {
        let first_type = infer_expr(
            db,
            cache,
            fields[0].get_value_expr().ok_or(InferFailReason::None)?,
        )?;
        return Ok(LuaType::Array(first_type.into()));
    }

    if let Some(first_field) = fields.first() {
        let first_value_expr = first_field.get_value_expr().ok_or(InferFailReason::None)?;

        if is_dots_expr(&first_value_expr).unwrap_or(false) {
            let first_expr_type = infer_expr(db, cache, first_value_expr)?;
            match &first_expr_type {
                LuaType::Variadic(multi) => match &multi.deref() {
                    VariadicType::Base(base) => {
                        return Ok(LuaType::Array(base.clone().into()));
                    }
                    VariadicType::Multi(tuple) => {
                        return Ok(LuaType::Tuple(
                            LuaTupleType::new(tuple.clone(), LuaTupleStatus::InferResolve).into(),
                        ));
                    }
                },
                _ => {
                    return Ok(LuaType::Array(first_expr_type.into()));
                }
            };
        }
    }

    let mut types = Vec::new();
    for field in fields {
        let value_expr = field.get_value_expr().ok_or(InferFailReason::None)?;
        let typ = infer_expr(db, cache, value_expr)?;
        match typ {
            LuaType::Variadic(multi) => flatten_multi_into_tuple(&mut types, &multi),
            _ => {
                types.push(typ);
            }
        }
    }

    Ok(LuaType::Tuple(
        LuaTupleType::new(types, LuaTupleStatus::InferResolve).into(),
    ))
}

fn flatten_multi_into_tuple(tuple_list: &mut Vec<LuaType>, multi: &VariadicType) {
    match multi {
        VariadicType::Base(base) => {
            tuple_list.push(LuaType::Variadic(VariadicType::Base(base.clone()).into()));
        }
        VariadicType::Multi(multi) => {
            for typ in multi {
                match typ {
                    LuaType::Variadic(multi) => {
                        flatten_multi_into_tuple(tuple_list, multi.deref());
                    }
                    _ => {
                        tuple_list.push(typ.clone());
                    }
                }
            }
        }
    }
}

fn is_dots_expr(expr: &LuaExpr) -> Option<bool> {
    if let LuaExpr::LiteralExpr(literal) = expr {
        match literal.get_literal()? {
            LuaLiteralToken::Dots(_) => return Some(true),
            _ => {}
        }
    }

    Some(false)
}

pub fn infer_table_should_be(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table: LuaTableExpr,
) -> InferResult {
    match table.get_parent::<LuaAst>().ok_or(InferFailReason::None)? {
        LuaAst::LuaCallArgList(call_arg_list) => {
            infer_table_type_by_calleee(db, cache, call_arg_list, table)
        }
        LuaAst::LuaTableField(field) => infer_table_field_type_by_parent(db, cache, field),
        LuaAst::LuaLocalStat(local) => infer_table_type_by_local(db, cache, local, table),
        LuaAst::LuaAssignStat(assign_stat) => {
            infer_table_type_by_assign_stat(db, cache, assign_stat, table)
        }
        _ => Err(InferFailReason::None),
    }
}

pub fn infer_table_field_value_should_be(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    table_field: LuaTableField,
) -> InferResult {
    let parnet_table_expr = table_field
        .get_parent::<LuaTableExpr>()
        .ok_or(InferFailReason::None)?;
    let parent_table_expr_type = infer_table_should_be(db, cache, parnet_table_expr)?;

    let index = LuaIndexMemberExpr::TableField(table_field.clone());
    let reason = match infer_member_by_member_key(
        db,
        cache,
        &parent_table_expr_type,
        index.clone(),
        &mut InferGuard::new(),
    ) {
        Ok(member_type) => return Ok(member_type),
        Err(InferFailReason::FieldNotFound) => InferFailReason::FieldNotFound,
        Err(err) => return Err(err),
    };

    match infer_member_by_operator(
        db,
        cache,
        &parent_table_expr_type,
        index.into(),
        &mut InferGuard::new(),
    ) {
        Ok(member_type) => return Ok(member_type),
        Err(InferFailReason::FieldNotFound) => {}
        Err(err) => return Err(err),
    }

    let member_id = LuaMemberId::new(table_field.get_syntax_id(), cache.get_file_id());
    match db.get_type_index().get_type_cache(&member_id.into()) {
        Some(type_cache) => return Ok(type_cache.as_type().clone()),
        None => {}
    };

    Err(reason)
}

fn infer_table_type_by_calleee(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_arg_list: LuaCallArgList,
    table_expr: LuaTableExpr,
) -> InferResult {
    let call_expr = call_arg_list
        .get_parent::<LuaCallExpr>()
        .ok_or(InferFailReason::None)?;
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer_expr(db, cache, prefix_expr)?;
    let func_type = infer_call_expr_func(
        db,
        cache,
        call_expr.clone(),
        prefix_type,
        &mut InferGuard::new(),
        None,
    )?;
    let param_types = func_type.get_params();
    let mut call_arg_number = call_arg_list
        .children::<LuaAst>()
        .into_iter()
        .enumerate()
        .find(|(_, arg)| arg.get_position() == table_expr.get_position())
        .ok_or(InferFailReason::None)?
        .0;
    match (func_type.is_colon_define(), call_expr.is_colon_call()) {
        (true, true) | (false, false) => {}
        (false, true) => {
            call_arg_number += 1;
        }
        (true, false) => {
            call_arg_number -= 1;
        }
    }
    Ok(param_types
        .get(call_arg_number)
        .ok_or(InferFailReason::None)?
        .1
        .clone()
        .unwrap_or(LuaType::Any))
}

fn infer_table_field_type_by_parent(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    field: LuaTableField,
) -> InferResult {
    let member_id = LuaMemberId::new(field.get_syntax_id(), cache.get_file_id());
    if let Some(type_cache) = db.get_type_index().get_type_cache(&member_id.into()) {
        match type_cache.as_type() {
            LuaType::TableConst(_) => {}
            typ => return Ok(typ.clone()),
        }
    } else {
        return Err(InferFailReason::UnResolveMemberType(member_id));
    }

    let parnet_table_expr = field
        .get_parent::<LuaTableExpr>()
        .ok_or(InferFailReason::None)?;
    let parent_table_expr_type = infer_table_should_be(db, cache, parnet_table_expr)?;

    let index = LuaIndexMemberExpr::TableField(field);
    let reason = match infer_member_by_member_key(
        db,
        cache,
        &parent_table_expr_type,
        index.clone(),
        &mut InferGuard::new(),
    ) {
        Ok(member_type) => return Ok(member_type),
        Err(InferFailReason::FieldNotFound) => InferFailReason::FieldNotFound,
        Err(err) => return Err(err),
    };

    match infer_member_by_operator(
        db,
        cache,
        &parent_table_expr_type,
        index.into(),
        &mut InferGuard::new(),
    ) {
        Ok(member_type) => return Ok(member_type),
        Err(InferFailReason::FieldNotFound) => {}
        Err(err) => return Err(err),
    }

    Err(reason)
}

fn infer_table_type_by_local(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    local: LuaLocalStat,
    table_expr: LuaTableExpr,
) -> InferResult {
    let local_names = local.get_local_name_list().collect::<Vec<_>>();
    let values = local.get_value_exprs().collect::<Vec<_>>();
    let num = values
        .iter()
        .enumerate()
        .find(|(_, value)| value.get_position() == table_expr.get_position())
        .ok_or(InferFailReason::None)?
        .0;

    let local_name = local_names.get(num).ok_or(InferFailReason::None)?;
    let decl_id = LuaDeclId::new(cache.get_file_id(), local_name.get_position());
    match db.get_type_index().get_type_cache(&decl_id.into()) {
        Some(type_cache) => match type_cache.as_type() {
            LuaType::TableConst(_) => Err(InferFailReason::None),
            typ => return Ok(typ.clone()),
        },
        None => Err(InferFailReason::UnResolveDeclType(decl_id)),
    }
}

fn infer_table_type_by_assign_stat(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    assign_stat: LuaAssignStat,
    table_expr: LuaTableExpr,
) -> InferResult {
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    let num = exprs
        .iter()
        .enumerate()
        .find(|(_, expr)| expr.get_position() == table_expr.get_position())
        .ok_or(InferFailReason::None)?
        .0;
    let name = vars.get(num).ok_or(InferFailReason::None)?;

    let decl_id = LuaDeclId::new(cache.get_file_id(), name.get_position());
    if db.get_decl_index().get_decl(&decl_id).is_some() {
        match db.get_type_index().get_type_cache(&decl_id.into()) {
            Some(type_cache) => match type_cache.as_type() {
                LuaType::TableConst(_) => Err(InferFailReason::None),
                typ => return Ok(typ.clone()),
            },
            None => Err(InferFailReason::UnResolveDeclType(decl_id)),
        }
    } else {
        infer_expr(
            db,
            cache,
            LuaExpr::cast(name.syntax().clone()).ok_or(InferFailReason::None)?,
        )
    }
}
