use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallArgList, LuaCallExpr, LuaExpr, LuaIndexMemberExpr, LuaLiteralToken,
    LuaLocalStat, LuaTableExpr, LuaTableField,
};

use crate::{
    db_index::{DbIndex, LuaType},
    infer_call_expr_func, infer_expr, InferGuard, LuaDeclId, LuaMemberId, LuaTupleType,
};

use super::{
    infer_index::{infer_member_by_member_key, infer_member_by_operator},
    InferResult, LuaInferConfig,
};

pub fn infer_table_expr(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    table: LuaTableExpr,
) -> InferResult {
    if table.is_array() {
        return infer_table_tuple_or_array(db, config, table);
    }

    Some(LuaType::TableConst(crate::InFiled {
        file_id: config.get_file_id(),
        value: table.get_range(),
    }))
}

fn infer_table_tuple_or_array(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    table: LuaTableExpr,
) -> InferResult {
    let fields = table.get_fields().collect::<Vec<_>>();
    if fields.len() > 10 {
        let first_type = infer_expr(db, config, fields[0].get_value_expr()?)?;
        return Some(LuaType::Array(first_type.into()));
    }

    if let Some(last_field) = fields.last() {
        let last_value_expr = last_field.get_value_expr()?;
        if is_dots_expr(&last_value_expr).unwrap_or(false) {
            let dots_type = infer_expr(db, config, last_value_expr)?;
            let typ = match &dots_type {
                LuaType::MuliReturn(multi) => multi.get_type(0).unwrap_or(&LuaType::Unknown),
                _ => &dots_type,
            };

            return Some(LuaType::Array(typ.clone().into()));
        }
    }

    let mut types = Vec::new();
    for field in fields {
        let value_expr = field.get_value_expr()?;
        let typ = infer_expr(db, config, value_expr)?;
        types.push(typ);
    }

    Some(LuaType::Tuple(LuaTupleType::new(types).into()))
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

#[allow(unused)]
pub fn infer_table_should_be(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    table: LuaTableExpr,
) -> InferResult {
    match table.get_parent::<LuaAst>()? {
        LuaAst::LuaCallArgList(call_arg_list) => {
            infer_table_type_by_calleee(db, config, call_arg_list, table)
        }
        LuaAst::LuaTableField(field) => infer_table_type_by_parent(db, config, field),
        LuaAst::LuaLocalStat(local) => infer_table_type_by_local(db, config, local, table),
        _ => None,
    }
}

fn infer_table_type_by_calleee(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    call_arg_list: LuaCallArgList,
    table_expr: LuaTableExpr,
) -> InferResult {
    let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
    let prefix_expr = call_expr.get_prefix_expr()?;
    let prefix_type = infer_expr(db, config, prefix_expr)?;
    let func_type = infer_call_expr_func(
        db,
        config,
        call_expr.clone(),
        prefix_type,
        &mut InferGuard::new(),
        None,
    )?;
    let param_types = func_type.get_params();
    let call_arg_number = call_arg_list
        .children::<LuaAst>()
        .into_iter()
        .enumerate()
        .find(|(_, arg)| arg.get_position() == table_expr.get_position())?
        .0;
    param_types.get(call_arg_number)?.1.clone()
}

fn infer_table_type_by_parent(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    field: LuaTableField,
) -> InferResult {
    let member_id = LuaMemberId::new(field.get_syntax_id(), config.get_file_id());
    if let Some(member) = db.get_member_index().get_member(&member_id) {
        let typ = member.get_decl_type();
        match typ {
            LuaType::TableConst(_) => {}
            _ => return Some(typ.clone()),
        }
    }

    let parnet_table_expr = field.get_parent::<LuaTableExpr>()?;
    let parent_table_expr_type = infer_table_should_be(db, config, parnet_table_expr)?;

    let index_member_expr = LuaIndexMemberExpr::TableField(field);
    if let Some(member_type) = infer_member_by_member_key(
        db,
        config,
        &parent_table_expr_type,
        index_member_expr.clone(),
        &mut InferGuard::new(),
    ) {
        return Some(member_type);
    }

    if let Some(member_type) = infer_member_by_operator(
        db,
        config,
        &parent_table_expr_type,
        index_member_expr,
        &mut InferGuard::new(),
    ) {
        return Some(member_type);
    }

    None
}

fn infer_table_type_by_local(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    local: LuaLocalStat,
    table_expr: LuaTableExpr,
) -> InferResult {
    let local_names = local.get_local_name_list().collect::<Vec<_>>();
    let values = local.get_value_exprs().collect::<Vec<_>>();
    let num = values
        .iter()
        .enumerate()
        .find(|(_, value)| value.get_position() == table_expr.get_position())?
        .0;

    let local_name = local_names.get(num)?;
    let decl_id = LuaDeclId::new(config.get_file_id(), local_name.get_position());
    let decl = db.get_decl_index().get_decl(&decl_id)?;
    let typ = decl.get_type()?;
    match typ {
        LuaType::TableConst(_) => None,
        _ => Some(typ.clone()),
    }
}
