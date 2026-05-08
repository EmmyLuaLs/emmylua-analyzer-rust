use emmylua_parser::{
    LuaAstNode, LuaExpr, LuaForStat, LuaIndexKey, LuaIndexMemberExpr, LuaNameExpr, LuaUnaryExpr,
    UnaryOperator,
};

use crate::{
    DbIndex, InferFailReason, LuaArrayLen, LuaArrayType, LuaInferCache, LuaMemberKey, LuaType,
    TypeOps,
    semantic::infer::{infer_index::infer_expr_for_index, narrow::get_var_expr_var_ref_id},
};

pub(super) fn infer_array_member_by_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    array_type: &LuaArrayType,
    index_expr: LuaIndexMemberExpr,
    key_type: &LuaType,
    key: &LuaMemberKey,
) -> Result<LuaType, InferFailReason> {
    let base = array_type.get_base();

    let index_prefix_expr = match index_expr {
        LuaIndexMemberExpr::TableField(_) => return Ok(base.clone()),
        _ => index_expr.get_prefix_expr().ok_or(InferFailReason::None)?,
    };

    if let LuaMemberKey::Integer(index_value) = key {
        if !db.get_emmyrc().strict.array_index {
            return Ok(base.clone());
        }

        if let LuaArrayLen::Max(max_len) = array_type.get_len()
            && *index_value > 0
            && *index_value <= *max_len
        {
            return Ok(base.clone());
        }

        return Ok(array_member_fallback(db, base));
    }

    if !key_type.is_integer() {
        if key_type.is_number() {
            return Ok(array_member_fallback(db, base));
        }

        return Err(InferFailReason::FieldNotFound);
    }

    if let LuaArrayLen::Max(max_len) = array_type.get_len()
        && let LuaType::IntegerConst(index_value) | LuaType::DocIntegerConst(index_value) = key_type
        && *index_value > 0
        && *index_value <= *max_len
    {
        return Ok(base.clone());
    }

    if let Some(LuaIndexKey::Expr(expr)) = index_expr.get_index_key()
        && check_iter_var_range(db, cache, &expr, index_prefix_expr).unwrap_or(false)
    {
        return Ok(base.clone());
    }

    Ok(array_member_fallback(db, base))
}

pub(super) fn array_member_fallback(db: &DbIndex, base: &LuaType) -> LuaType {
    match base {
        LuaType::Any | LuaType::Unknown => base.clone(),
        _ if db.get_emmyrc().strict.array_index => TypeOps::Union.apply(db, base, &LuaType::Nil),
        _ => base.clone(),
    }
}

pub(super) fn check_iter_var_range(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    may_iter_var: &LuaExpr,
    prefix_expr: LuaExpr,
) -> Option<bool> {
    match may_iter_var {
        LuaExpr::NameExpr(name_expr) => check_index_var_in_range(db, cache, name_expr, prefix_expr),
        LuaExpr::UnaryExpr(unary_expr) => check_is_len(db, cache, unary_expr, prefix_expr),
        _ => None,
    }
}

fn check_index_var_in_range(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    iter_var: &LuaNameExpr,
    prefix_expr: LuaExpr,
) -> Option<bool> {
    let decl_id = db
        .get_reference_index()
        .get_var_reference_decl(&cache.get_file_id(), iter_var.get_range())?;

    let decl = db.get_decl_index().get_decl(&decl_id)?;
    let decl_syntax_id = decl.get_syntax_id();
    if !decl_syntax_id.is_token() {
        return None;
    }

    let root = prefix_expr.get_root();
    let token = decl_syntax_id.to_token_from_root(&root)?;
    let parent_node = token.parent()?;
    let for_stat = LuaForStat::cast(parent_node)?;
    let iter_exprs = for_stat.get_iter_expr().collect::<Vec<_>>();
    let test_len_expr = match iter_exprs.len() {
        2 => {
            let LuaExpr::UnaryExpr(unary_expr) = iter_exprs[1].clone() else {
                return None;
            };
            unary_expr
        }
        3 => {
            let step_type = infer_expr_for_index(db, cache, iter_exprs[2].clone()).ok()?;
            let LuaType::IntegerConst(step_value) = step_type else {
                return None;
            };
            if step_value > 0 {
                let LuaExpr::UnaryExpr(unary_expr) = iter_exprs[1].clone() else {
                    return None;
                };
                unary_expr
            } else if step_value < 0 {
                let LuaExpr::UnaryExpr(unary_expr) = iter_exprs[0].clone() else {
                    return None;
                };
                unary_expr
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let op = test_len_expr.get_op_token()?;
    if op.get_op() != UnaryOperator::OpLen {
        return None;
    }

    let len_expr = test_len_expr.get_expr()?;
    let len_expr_var_ref_id = get_var_expr_var_ref_id(db, cache, len_expr)?;
    let prefix_expr_var_ref_id = get_var_expr_var_ref_id(db, cache, prefix_expr)?;

    Some(len_expr_var_ref_id == prefix_expr_var_ref_id)
}

fn check_is_len(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    unary_expr: &LuaUnaryExpr,
    prefix_expr: LuaExpr,
) -> Option<bool> {
    let op = unary_expr.get_op_token()?;
    if op.get_op() != UnaryOperator::OpLen {
        return None;
    }

    let inner_var_expr = unary_expr.get_expr()?;
    let len_expr_var_ref_id = get_var_expr_var_ref_id(db, cache, inner_var_expr)?;
    let prefix_expr_var_ref_id = get_var_expr_var_ref_id(db, cache, prefix_expr)?;

    Some(len_expr_var_ref_id == prefix_expr_var_ref_id)
}
