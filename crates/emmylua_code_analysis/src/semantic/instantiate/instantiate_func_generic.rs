use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaSyntaxNode};

use crate::{
    db_index::{DbIndex, LuaType},
    semantic::{infer_expr, LuaInferConfig},
    LuaFunctionType,
};

use super::{
    instantiate_class_generic::instantiate_doc_function, tpl_pattern::{tpl_pattern_match, variadic_tpl_pattern_match},
    type_substitutor::TypeSubstitutor,
};

pub fn instantiate_func_generic(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    func: &LuaFunctionType,
    call_expr: LuaCallExpr,
) -> Option<LuaFunctionType> {
    let origin_params = func.get_params();
    let func_param_types: Vec<_> = origin_params
        .iter()
        .map(|(_, t)| t.clone().unwrap_or(LuaType::Unknown))
        .collect();

    let mut arg_types = collect_arg_types(db, config, &call_expr)?;

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

    let substitutor = match_tpl_args(
        db,
        config,
        &func_param_types,
        &arg_types,
        &call_expr.get_root(),
    );

    if let LuaType::DocFunction(f) = instantiate_doc_function(db, func, &substitutor) {
        Some(f.deref().clone())
    } else {
        func.clone().into()
    }
}

fn collect_arg_types(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    call_expr: &LuaCallExpr,
) -> Option<Vec<LuaType>> {
    let arg_list = call_expr.get_args_list()?;
    let mut arg_types = Vec::new();
    for arg in arg_list.get_args() {
        let arg_type = infer_expr(db, config, arg.clone())?;
        arg_types.push(arg_type);
    }

    Some(arg_types)
}

fn match_tpl_args(
    db: &DbIndex,
    infer_config: &mut LuaInferConfig,
    func_param_types: &Vec<LuaType>,
    arg_types: &Vec<LuaType>,
    root: &LuaSyntaxNode,
) -> TypeSubstitutor {
    let mut substitutor = TypeSubstitutor::new();
    for (i, func_param_type) in func_param_types.iter().enumerate() {
        let arg_type = if i < arg_types.len() {
            &arg_types[i]
        } else {
            continue;
        };

        if let LuaType::Variadic(inner) = func_param_type {
            let rest_arg_types = &arg_types[i..];
            variadic_tpl_pattern_match(&inner, rest_arg_types, &mut substitutor);
            break;
        }

        tpl_pattern_match(
            db,
            infer_config,
            root,
            func_param_type,
            arg_type,
            &mut substitutor,
        );
    }

    substitutor
}

// fn instantiate_func_by_return(
//     db: &mut DbIndex,
//     infer_config: &mut LuaInferConfig,
//     file_id: FileId,
//     func_param_types: &mut Vec<LuaType>,
//     func_return_types: &mut Vec<LuaType>,
//     return_types: Vec<LuaType>,
// ) -> Option<()> {
//     todo!()
// }
