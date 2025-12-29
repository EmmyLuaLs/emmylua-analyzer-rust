use emmylua_parser::{
    LuaAstNode, LuaExpr, LuaForStat, LuaIndexKey, LuaIndexMemberExpr, LuaNameExpr, LuaUnaryExpr,
    NumberResult, UnaryOperator,
};

use crate::{
    DbIndex, InferFailReason, LuaArrayLen, LuaArrayType, LuaInferCache, LuaType, TypeOps,
    infer_expr, semantic::infer::narrow::get_var_expr_var_ref_id,
};

pub fn infer_array_member(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    array_type: &LuaArrayType,
    index_member_expr: LuaIndexMemberExpr,
) -> Result<LuaType, InferFailReason> {
    let key = index_member_expr
        .get_index_key()
        .ok_or(InferFailReason::None)?;
    let index_prefix_expr = match index_member_expr {
        LuaIndexMemberExpr::TableField(_) => {
            return Ok(array_type.get_base().clone());
        }
        _ => index_member_expr
            .get_prefix_expr()
            .ok_or(InferFailReason::None)?,
    };

    match key {
        LuaIndexKey::Integer(i) => {
            if !db.get_emmyrc().strict.array_index {
                return Ok(array_type.get_base().clone());
            }

            let base_type = array_type.get_base();
            match array_type.get_len() {
                LuaArrayLen::None => {}
                LuaArrayLen::Max(max_len) => {
                    if let NumberResult::Int(index_value) = i.get_number_value() {
                        if index_value > 0 && index_value <= *max_len {
                            return Ok(base_type.clone());
                        }
                    }
                }
            }

            let result_type = match &base_type {
                LuaType::Any | LuaType::Unknown => base_type.clone(),
                _ => TypeOps::Union.apply(db, base_type, &LuaType::Nil),
            };

            Ok(result_type)
        }
        LuaIndexKey::Expr(expr) => {
            let expr_type = infer_expr(db, cache, expr.clone())?;
            if expr_type.is_integer() {
                let base_type = array_type.get_base();
                match (array_type.get_len(), expr_type) {
                    (
                        LuaArrayLen::Max(max_len),
                        LuaType::IntegerConst(index_value) | LuaType::DocIntegerConst(index_value),
                    ) => {
                        if index_value > 0 && index_value <= *max_len {
                            return Ok(base_type.clone());
                        }
                    }
                    _ => {
                        if check_iter_var_range(db, cache, &expr, index_prefix_expr)
                            .unwrap_or(false)
                        {
                            return Ok(base_type.clone());
                        }
                    }
                }

                let result_type = match &base_type {
                    LuaType::Any | LuaType::Unknown => base_type.clone(),
                    _ => {
                        if db.get_emmyrc().strict.array_index {
                            TypeOps::Union.apply(db, base_type, &LuaType::Nil)
                        } else {
                            base_type.clone()
                        }
                    }
                };

                Ok(result_type)
            } else {
                Err(InferFailReason::FieldNotFound)
            }
        }
        _ => Err(InferFailReason::FieldNotFound),
    }
}

pub fn check_iter_var_range(
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
            let step_type = infer_expr(db, cache, iter_exprs[2].clone()).ok()?;
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
