use emmylua_code_analysis::{
    instantiate_func_generic, LuaFunctionType, LuaSemanticDeclId, LuaSignature, LuaType,
    SemanticModel,
};
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaSyntaxToken};
use std::sync::Arc;

pub fn find_match_function(
    semantic_model: &SemanticModel,
    trigger_token: &LuaSyntaxToken,
    semantic_decls: &Vec<LuaSemanticDeclId>,
) -> Option<LuaSemanticDeclId> {
    let call_expr = LuaCallExpr::cast(trigger_token.parent()?.parent()?)?;
    let call_function = get_call_function(semantic_model, &call_expr)?;

    let member_decls: Vec<_> = semantic_decls
        .iter()
        .filter_map(|decl| match decl {
            LuaSemanticDeclId::Member(member_id) => Some((decl, member_id)),
            _ => None,
        })
        .collect();

    for (decl, member_id) in member_decls {
        let typ = semantic_model.get_type(member_id.clone().into());
        match typ {
            LuaType::DocFunction(func) => {
                if compare_function_types(semantic_model, &call_function, &func, &call_expr)? {
                    return Some(decl.clone());
                }
            }
            LuaType::Signature(signature_id) => {
                let signature = semantic_model
                    .get_db()
                    .get_signature_index()
                    .get(&signature_id)?;
                let functions = get_signature_functions(signature);

                if functions.iter().any(|func| {
                    compare_function_types(semantic_model, &call_function, func, &call_expr)
                        .unwrap_or(false)
                }) {
                    return Some(decl.clone());
                }
            }
            _ => continue,
        }
    }

    None
}

/// 获取最匹配的函数(并不能确保完全匹配)
fn get_call_function(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Arc<LuaFunctionType>> {
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None);
    if let Some(func) = func {
        let call_expr_args_count = call_expr.get_args_count();
        if let Some(mut call_expr_args_count) = call_expr_args_count {
            let func_params_count = func.get_params().len();
            if !func.is_colon_define() && call_expr.is_colon_call() {
                // 不是冒号定义的函数, 但是是冒号调用
                call_expr_args_count += 1;
            }
            if call_expr_args_count == func_params_count {
                return Some(func);
            }
        }
    }
    None
}

fn get_signature_functions(signature: &LuaSignature) -> Vec<Arc<LuaFunctionType>> {
    let mut functions = Vec::new();
    functions.push(signature.to_doc_func_type());
    functions.extend(
        signature
            .overloads
            .iter()
            .map(|overload| Arc::clone(overload)),
    );
    functions
}

/// 比较函数类型是否匹配, 会处理泛型情况
fn compare_function_types(
    semantic_model: &SemanticModel,
    call_function: &LuaFunctionType,
    func: &Arc<LuaFunctionType>,
    call_expr: &LuaCallExpr,
) -> Option<bool> {
    let func = if func.contain_tpl() {
        instantiate_func_generic(
            semantic_model.get_db(),
            &mut semantic_model.get_config().borrow_mut(),
            func,
            call_expr.clone(),
        )
        .ok()?
    } else {
        (**func).clone()
    };
    Some(call_function == &func)
}
