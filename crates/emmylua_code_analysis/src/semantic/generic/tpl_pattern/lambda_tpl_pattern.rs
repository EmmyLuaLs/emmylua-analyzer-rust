use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaExpr, LuaNameExpr, LuaSyntaxId};
use hashbrown::HashMap;

use crate::{
    InferFailReason, LuaDeclId, LuaFunctionType, LuaSignatureId, LuaType, LuaTypeNode, TplContext,
    analyze_func_body_returns_with, analyze_return_point,
    db_index::return_row::row_to_multi_return_type, infer_expr, instantiate_type_generic,
};

use super::{TplPatternMatchResult, return_type_pattern_match_target_type};

pub fn check_lambda_tpl_pattern(
    context: &mut TplContext,
    tpl_func: &LuaFunctionType,
    signature_id: LuaSignatureId,
) -> TplPatternMatchResult {
    let Some(closure) = find_current_call_lambda(context, signature_id)? else {
        return Ok(());
    };
    let expected_func = match instantiate_type_generic(
        context.db,
        &LuaType::DocFunction(tpl_func.clone().into()),
        context.substitutor,
    ) {
        LuaType::DocFunction(func) => func.as_ref().clone(),
        _ => tpl_func.clone(),
    };
    let inferred_return = match infer_lambda_return_type(context, &closure, &expected_func) {
        Ok(Some(inferred_return)) => inferred_return,
        _ => return Ok(()),
    };

    let expected_return = row_to_multi_return_type(tpl_func.get_return_row().to_vec());
    return_type_pattern_match_target_type(context, &expected_return, &inferred_return)
}

fn find_current_call_lambda(
    context: &mut TplContext,
    signature_id: LuaSignatureId,
) -> Result<Option<LuaClosureExpr>, InferFailReason> {
    let call_expr = context.call_expr.clone().ok_or(InferFailReason::None)?;
    let call_arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    for arg in call_arg_list.get_args() {
        match arg {
            LuaExpr::ClosureExpr(closure)
                if LuaSignatureId::from_closure(context.cache.get_file_id(), &closure)
                    == signature_id =>
            {
                return Ok(Some(closure));
            }
            _ => {
                if let Ok(LuaType::Signature(arg_signature_id)) =
                    infer_expr(context.db, context.cache, arg.clone())
                    && arg_signature_id == signature_id
                {
                    return Ok(None);
                }
            }
        }
    }

    Err(InferFailReason::UnResolveSignatureReturn(signature_id))
}

fn infer_lambda_return_type(
    context: &mut TplContext,
    closure: &LuaClosureExpr,
    expected_func: &LuaFunctionType,
) -> Result<Option<LuaType>, InferFailReason> {
    let block = closure.get_block().ok_or(InferFailReason::None)?;
    let param_overlays = collect_lambda_param_overlays(context, closure, expected_func);
    let db = context.db;
    // 在当前泛型调用轮次内重放闭包参数类型, 让 `return item` 能看到已推导出的 `T`.
    let return_docs = context.cache.with_no_flow(|cache| {
        cache.with_replay_overlay(&param_overlays, &[], |cache| {
            // 这里只临时推断闭包返回值用于绑定回调返回泛型, 不写回签名索引.
            let return_points = analyze_func_body_returns_with(block.clone(), &mut |expr| {
                infer_expr(db, cache, expr.clone())
            })?;
            analyze_return_point(db, cache, &return_points)
        })
    })?;

    Ok(return_docs
        .first()
        .map(|return_info| return_info.type_ref.clone()))
}

fn collect_lambda_param_overlays(
    context: &TplContext,
    closure: &LuaClosureExpr,
    expected_func: &LuaFunctionType,
) -> Vec<(LuaSyntaxId, LuaType)> {
    let Some(block) = closure.get_block() else {
        return Vec::new();
    };
    let Some(param_list) = closure.get_params_list() else {
        return Vec::new();
    };

    let file_id = context.cache.get_file_id();
    let mut param_types = HashMap::new();
    for (idx, param) in param_list.get_params().enumerate() {
        let Some((_, Some(param_type))) = expected_func.get_params().get(idx) else {
            continue;
        };
        // 只有已实例化的参数类型可以参与 overlay, 未完成的泛型继续交给后续推断.
        if param_type.contains_tpl_node() || param_type.is_unknown() {
            continue;
        }

        let decl_id = LuaDeclId::new(file_id, param.get_range().start());
        param_types.insert(decl_id, param_type.clone());
    }

    if param_types.is_empty() {
        return Vec::new();
    }

    let Some(file_ref) = context
        .db
        .get_reference_index()
        .get_local_reference(&file_id)
    else {
        return Vec::new();
    };

    let mut overlays = Vec::new();
    for name_expr in block.descendants::<LuaNameExpr>() {
        // 按引用关系确认 name 真的指向闭包形参, 避免同名局部变量被错误覆盖.
        let Some(decl_id) = file_ref.get_decl_id(&name_expr.get_range()) else {
            continue;
        };
        if let Some(param_type) = param_types.get(&decl_id) {
            overlays.push((name_expr.get_syntax_id(), param_type.clone()));
        }
    }

    overlays
}
