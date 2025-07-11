use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr};

use crate::{
    db_index::{DbIndex, LuaType},
    semantic::{infer::InferFailReason, infer_expr, LuaInferCache},
    LuaFunctionType,
};

use super::{
    instantiate_type_generic::instantiate_doc_function, tpl_pattern::tpl_pattern_match_args,
    TypeSubstitutor,
};

// todo: need cache
pub fn instantiate_func_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    func: &LuaFunctionType,
    call_expr: LuaCallExpr,
) -> Result<LuaFunctionType, InferFailReason> {
    let origin_params = func.get_params();
    let func_param_types: Vec<_> = origin_params
        .iter()
        .map(|(_, t)| t.clone().unwrap_or(LuaType::Unknown))
        .collect();

    let mut arg_types = collect_arg_types(db, cache, &call_expr)?;

    let colon_call = call_expr.is_colon_call();
    let colon_define = func.is_colon_define();
    match (colon_define, colon_call) {
        (true, true) | (false, false) => {}
        (true, false) => {
            if !arg_types.is_empty() {
                arg_types.remove(0);
            }
        }
        (false, true) => {
            arg_types.insert(0, LuaType::Any);
        }
    }

    let mut substitutor = TypeSubstitutor::new();
    substitutor.set_call_expr(call_expr.clone());
    tpl_pattern_match_args(
        db,
        cache,
        &func_param_types,
        &arg_types,
        &call_expr.get_root(),
        &mut substitutor,
    )?;

    if func.contain_self() {
        infer_self_type(db, cache, &call_expr, &mut substitutor)?;
    }

    if let LuaType::DocFunction(f) = instantiate_doc_function(db, func, &substitutor) {
        Ok(f.deref().clone())
    } else {
        Ok(func.clone())
    }
}

fn collect_arg_types(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: &LuaCallExpr,
) -> Result<Vec<LuaType>, InferFailReason> {
    let arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    let mut arg_types = Vec::new();
    for arg in arg_list.get_args() {
        let arg_type = infer_expr(db, cache, arg.clone())?;
        arg_types.push(arg_type);
    }

    Ok(arg_types)
}

fn infer_self_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: &LuaCallExpr,
    substitutor: &mut TypeSubstitutor,
) -> Result<(), InferFailReason> {
    let prefix_expr = call_expr.get_prefix_expr();
    if let Some(prefix_expr) = prefix_expr {
        if let LuaExpr::IndexExpr(index) = prefix_expr {
            let self_expr = index.get_prefix_expr();
            if let Some(self_expr) = self_expr {
                let self_type = infer_expr(db, cache, self_expr.into())?;
                substitutor.add_self_type(self_type);
            }
        }
    }

    Ok(())
}
