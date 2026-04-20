mod infer_binary;
mod infer_call;
mod infer_doc_type;
mod infer_fail_reason;
mod infer_index;
mod infer_name;
mod infer_table;
mod infer_unary;
mod narrow;
mod test;

use std::ops::Deref;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaLiteralExpr, LuaLiteralToken,
    LuaSyntaxId, LuaTableExpr, LuaVarExpr, NumberResult,
};
use infer_binary::infer_binary_expr;
use infer_call::infer_call_expr;
pub use infer_call::infer_call_expr_func;
pub use infer_doc_type::{DocTypeInferContext, infer_doc_type};
pub use infer_fail_reason::InferFailReason;
pub use infer_index::infer_index_expr;
pub(crate) use infer_index::try_infer_expr_for_index;
use infer_name::infer_name_expr;
pub use infer_name::{find_self_decl_or_member_id, infer_param};
use infer_table::infer_table_expr;
pub use infer_table::{infer_table_field_value_should_be, infer_table_should_be};
use infer_unary::infer_unary_expr;
pub use narrow::VarRefId;
pub(in crate::semantic) use narrow::{ConditionFlowAction, InferConditionFlow};

use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    InFiled, InferGuard, LuaMemberKey, VariadicType,
    db_index::{DbIndex, LuaOperator, LuaOperatorMetaMethod, LuaSignatureId, LuaType},
};

use super::{CacheEntry, LuaInferCache, NoFlowCacheEntry, member::infer_raw_member_type};

pub type InferResult = Result<LuaType, InferFailReason>;
pub use infer_call::InferCallFuncResult;

fn prepare_expr_cache(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    syntax_id: LuaSyntaxId,
) -> Result<Option<LuaType>, InferFailReason> {
    let file_id = cache.get_file_id();
    if cache.is_no_flow() {
        if let Some(cache_entry) = cache.expr_no_flow_cache.get(&syntax_id) {
            match cache_entry {
                NoFlowCacheEntry::Cache(ty) => return Ok(Some(ty.clone())),
                NoFlowCacheEntry::Declined => return Err(InferFailReason::None),
                NoFlowCacheEntry::Ready => return Err(InferFailReason::RecursiveInfer),
            }
        }

        let in_filed_syntax_id = InFiled::new(file_id, syntax_id);
        if let Some(bind_type_cache) = db
            .get_type_index()
            .get_type_cache(&in_filed_syntax_id.into())
        {
            let ty = bind_type_cache.as_type().clone();
            cache
                .expr_no_flow_cache
                .insert(syntax_id, NoFlowCacheEntry::Cache(ty.clone()));
            return Ok(Some(ty));
        }

        cache
            .expr_no_flow_cache
            .insert(syntax_id, NoFlowCacheEntry::Ready);
        return Ok(None);
    }

    if let Some(cache_entry) = cache.expr_cache.get(&syntax_id) {
        match cache_entry {
            CacheEntry::Cache(ty) => return Ok(Some(ty.clone())),
            CacheEntry::Ready => return Err(InferFailReason::RecursiveInfer),
        }
    }

    let in_filed_syntax_id = InFiled::new(file_id, syntax_id);
    if let Some(bind_type_cache) = db
        .get_type_index()
        .get_type_cache(&in_filed_syntax_id.into())
    {
        let ty = bind_type_cache.as_type().clone();
        cache
            .expr_cache
            .insert(syntax_id, CacheEntry::Cache(ty.clone()));
        return Ok(Some(ty));
    }

    cache.expr_cache.insert(syntax_id, CacheEntry::Ready);
    Ok(None)
}

pub(in crate::semantic) fn supports_no_flow_expr(expr: &LuaExpr) -> bool {
    matches!(
        expr,
        LuaExpr::CallExpr(_)
            | LuaExpr::BinaryExpr(_)
            | LuaExpr::LiteralExpr(_)
            | LuaExpr::ClosureExpr(_)
            | LuaExpr::ParenExpr(_)
            | LuaExpr::NameExpr(_)
            | LuaExpr::UnaryExpr(_)
            | LuaExpr::IndexExpr(_)
    )
}

pub fn infer_expr(db: &DbIndex, cache: &mut LuaInferCache, expr: LuaExpr) -> InferResult {
    let no_flow = cache.is_no_flow();
    let syntax_id = expr.get_syntax_id();
    if let Some(result_type) = prepare_expr_cache(db, cache, syntax_id)? {
        return Ok(result_type);
    }
    if no_flow && !supports_no_flow_expr(&expr) {
        cache
            .expr_no_flow_cache
            .insert(syntax_id, NoFlowCacheEntry::Declined);
        return Err(InferFailReason::None);
    }
    let result_type = match expr {
        LuaExpr::CallExpr(call_expr) => infer_call_expr(db, cache, call_expr),
        LuaExpr::TableExpr(table_expr) => infer_table_expr(db, cache, table_expr),
        LuaExpr::LiteralExpr(literal_expr) => infer_literal_expr(db, cache, literal_expr),
        LuaExpr::BinaryExpr(binary_expr) => infer_binary_expr(db, cache, binary_expr),
        LuaExpr::UnaryExpr(unary_expr) => infer_unary_expr(db, cache, unary_expr),
        LuaExpr::ClosureExpr(closure_expr) => infer_closure_expr(db, cache, closure_expr),
        LuaExpr::ParenExpr(paren_expr) => infer_expr(
            db,
            cache,
            paren_expr.get_expr().ok_or(InferFailReason::None)?,
        ),
        LuaExpr::NameExpr(name_expr) => infer_name_expr(db, cache, name_expr),
        LuaExpr::IndexExpr(index_expr) => infer_index_expr(db, cache, index_expr, !no_flow),
    };

    match &result_type {
        Ok(result_type) => {
            if no_flow {
                cache
                    .expr_no_flow_cache
                    .insert(syntax_id, NoFlowCacheEntry::Cache(result_type.clone()));
            } else {
                cache
                    .expr_cache
                    .insert(syntax_id, CacheEntry::Cache(result_type.clone()));
            }
        }
        Err(InferFailReason::None) | Err(InferFailReason::RecursiveInfer) => {
            if no_flow {
                cache
                    .expr_no_flow_cache
                    .insert(syntax_id, NoFlowCacheEntry::Declined);
            } else {
                cache
                    .expr_cache
                    .insert(syntax_id, CacheEntry::Cache(LuaType::Unknown));
                return Ok(LuaType::Unknown);
            }
        }
        Err(InferFailReason::FieldNotFound) => {
            if no_flow {
                cache.expr_no_flow_cache.remove(&syntax_id);
            } else if cache.get_config().analysis_phase.is_force() {
                cache
                    .expr_cache
                    .insert(syntax_id, CacheEntry::Cache(LuaType::Nil));
                return Ok(LuaType::Nil);
            } else {
                cache.expr_cache.remove(&syntax_id);
            }
        }
        _ => {
            if no_flow {
                cache.expr_no_flow_cache.remove(&syntax_id);
            } else {
                cache.expr_cache.remove(&syntax_id);
            }
        }
    }

    result_type
}

pub(in crate::semantic) fn try_infer_expr_no_flow(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
) -> Result<Option<LuaType>, InferFailReason> {
    match cache.with_no_flow(|cache| infer_expr(db, cache, expr)) {
        Ok(result_type) => Ok(Some(result_type)),
        Err(InferFailReason::None) | Err(InferFailReason::RecursiveInfer) => Ok(None),
        Err(err) => Err(err),
    }
}

fn infer_literal_expr(db: &DbIndex, config: &LuaInferCache, expr: LuaLiteralExpr) -> InferResult {
    match expr.get_literal().ok_or(InferFailReason::None)? {
        LuaLiteralToken::Nil(_) => Ok(LuaType::Nil),
        LuaLiteralToken::Bool(bool) => Ok(LuaType::BooleanConst(bool.is_true())),
        LuaLiteralToken::Number(num) => match num.get_number_value() {
            NumberResult::Int(i) => Ok(LuaType::IntegerConst(i)),
            NumberResult::Float(f) => Ok(LuaType::FloatConst(f)),
            _ => Ok(LuaType::Number),
        },
        LuaLiteralToken::String(str) => {
            Ok(LuaType::StringConst(SmolStr::new(str.get_value()).into()))
        }
        LuaLiteralToken::Dots(_) => {
            let file_id = config.get_file_id();
            let range = expr.get_range();

            let decl_id = db
                .get_reference_index()
                .get_local_reference(&file_id)
                .and_then(|file_ref| file_ref.get_decl_id(&range));

            let decl_type = match decl_id.and_then(|id| db.get_decl_index().get_decl(&id)) {
                Some(decl) if decl.is_global() => LuaType::Any,
                Some(decl) if decl.is_param() => {
                    let base = infer_param(db, decl).unwrap_or(LuaType::Unknown);
                    LuaType::Variadic(VariadicType::Base(base).into())
                }
                _ => LuaType::Variadic(VariadicType::Base(LuaType::Any).into()),
            };

            Ok(decl_type)
        }
        // unreachable
        _ => Ok(LuaType::Any),
    }
}

fn infer_closure_expr(_: &DbIndex, config: &LuaInferCache, closure: LuaClosureExpr) -> InferResult {
    let signature_id = LuaSignatureId::from_closure(config.get_file_id(), &closure);
    Ok(LuaType::Signature(signature_id))
}

fn get_custom_type_operator(
    db: &DbIndex,
    operand_type: LuaType,
    op: LuaOperatorMetaMethod,
) -> Option<Vec<&LuaOperator>> {
    if operand_type.is_custom_type() {
        let type_id = match operand_type {
            LuaType::Ref(type_id) => type_id,
            LuaType::Def(type_id) => type_id,
            _ => return None,
        };
        let op_ids = db.get_operator_index().get_operators(&type_id.into(), op)?;
        let operators = op_ids
            .iter()
            .filter_map(|id| db.get_operator_index().get_operator(id))
            .collect();

        Some(operators)
    } else {
        None
    }
}

pub fn infer_expr_list_value_type_at(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    exprs: &[LuaExpr],
    value_idx: usize,
) -> Result<Option<LuaType>, InferFailReason> {
    let exprs_len = exprs.len();
    if exprs_len == 0 {
        Ok(None)
    } else if value_idx < exprs_len {
        Ok(
            infer_expr_list_types(db, cache, &exprs[value_idx..], Some(1), infer_expr)?
                .first()
                .map(|(ty, _)| ty.clone()),
        )
    } else {
        let last_idx = exprs_len - 1;
        let offset = value_idx - last_idx;
        Ok(
            infer_expr_list_types(db, cache, &exprs[last_idx..], Some(offset + 1), infer_expr)?
                .get(offset)
                .map(|(ty, _)| ty.clone()),
        )
    }
}

pub fn infer_expr_list_types<F>(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    exprs: &[LuaExpr],
    var_count: Option<usize>,
    mut infer: F,
) -> Result<Vec<(LuaType, TextRange)>, InferFailReason>
where
    F: FnMut(&DbIndex, &mut LuaInferCache, LuaExpr) -> InferResult,
{
    let mut value_types = Vec::new();
    for (idx, expr) in exprs.iter().enumerate() {
        if let Some(var_count) = var_count
            && value_types.len() >= var_count
        {
            break;
        }

        let expr_type = infer(db, cache, expr.clone())?;
        if let Some(var_count) = var_count
            && expr_type.contain_multi_return()
        {
            if idx < var_count {
                for i in idx..var_count {
                    if let Some(typ) = expr_type.get_result_slot_type(i - idx) {
                        value_types.push((typ, expr.get_range()));
                    } else {
                        break;
                    }
                }
            }

            break;
        }

        match expr_type {
            LuaType::Variadic(variadic) => {
                match variadic.deref() {
                    VariadicType::Base(base) => {
                        value_types.push((base.clone(), expr.get_range()));
                    }
                    VariadicType::Multi(vecs) => {
                        for typ in vecs {
                            value_types.push((typ.clone(), expr.get_range()));
                        }
                    }
                }

                break;
            }
            _ => value_types.push((expr_type, expr.get_range())),
        }
    }

    Ok(value_types)
}

/// 推断值已经绑定的类型(不是推断值的类型). 例如从右值推断左值类型, 从调用参数推断函数参数类型参数类型
pub fn infer_bind_value_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
) -> Option<LuaType> {
    let parent_node = expr.syntax().parent().and_then(LuaAst::cast)?;

    match parent_node {
        LuaAst::LuaAssignStat(assign) => {
            let (vars, exprs) = assign.get_var_and_expr_list();
            let mut typ = None;
            for (idx, assign_expr) in exprs.iter().enumerate() {
                if expr == *assign_expr {
                    let var = vars.get(idx);
                    if let Some(var) = var {
                        if let LuaVarExpr::IndexExpr(index_expr) = var {
                            let prefix_expr = index_expr.get_prefix_expr()?;
                            let prefix_type = infer_expr(db, cache, prefix_expr).ok()?;
                            // 如果前缀类型是定义类型, 则不认为存在左值绑定
                            if let LuaType::Def(_) = prefix_type {
                                return None;
                            }
                        };
                        typ = Some(infer_expr(db, cache, var.clone().into()).ok()?);
                        break;
                    }
                }
            }
            typ
        }
        LuaAst::LuaTableField(table_field) => {
            let field_key = table_field.get_field_key()?;
            let table_expr = table_field.get_parent::<LuaTableExpr>()?;
            let table_type = infer_table_should_be(db, cache, table_expr.clone()).ok()?;
            let member_key = match LuaMemberKey::from_index_key(db, cache, &field_key) {
                Ok(key) => key,
                Err(_) => return None,
            };
            match infer_raw_member_type(db, &table_type, &member_key) {
                Ok(typ) => Some(typ),
                Err(InferFailReason::FieldNotFound) => None,
                Err(_) => Some(LuaType::Unknown),
            }
        }
        LuaAst::LuaCallArgList(call_arg_list) => {
            let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
            // 获取调用位置
            let mut param_pos = 0;
            for (idx, arg) in call_arg_list.get_args().enumerate() {
                if arg == expr {
                    param_pos = idx;
                    break;
                }
            }
            let is_colon_call = call_expr.is_colon_call();

            let expr_type = infer_expr(db, cache, call_expr.get_prefix_expr()?).ok()?;
            let func_type = infer_call_expr_func(
                db,
                cache,
                call_expr,
                expr_type.clone(),
                &InferGuard::new(),
                None,
            )
            .ok()?;

            match (func_type.is_colon_define(), is_colon_call) {
                (true, false) => {
                    if param_pos == 0 {
                        return None;
                    }
                    param_pos -= 1;
                }
                (false, true) => {
                    param_pos += 1;
                }
                _ => {}
            }

            let param_info = func_type.get_params().get(param_pos)?;
            param_info.1.clone()
        }
        _ => None,
    }
}
