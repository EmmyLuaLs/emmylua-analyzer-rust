use emmylua_code_analysis::{
    instantiate_func_generic, LuaCompilation, LuaFunctionType, LuaSemanticDeclId, LuaSignature,
    LuaSignatureId, LuaType, SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaSyntaxToken, LuaTokenKind};
use rowan::{NodeOrToken, TokenAtOffset};
use std::sync::Arc;

pub fn find_call_match_function_for_member(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    trigger_token: &LuaSyntaxToken,
    semantic_decls: &Vec<LuaSemanticDeclId>,
) -> Option<Vec<LuaSemanticDeclId>> {
    let call_expr = LuaCallExpr::cast(trigger_token.parent()?.parent()?)?;
    let call_function = get_call_function(semantic_model, &call_expr)?;
    let mut result = Vec::new();
    let member_decls: Vec<_> = semantic_decls
        .iter()
        .filter_map(|decl| match decl {
            LuaSemanticDeclId::Member(member_id) => Some((decl, member_id)),
            _ => None,
        })
        .collect();

    let mut has_match = false;
    for (decl, member_id) in member_decls {
        let typ = semantic_model.get_type(member_id.clone().into());
        match typ {
            LuaType::DocFunction(func) => {
                if compare_function_types(semantic_model, &call_function, &func, &call_expr)? {
                    result.push(decl.clone());
                    has_match = true;
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
                    has_match = true;
                }
                // 无论是否匹配, 都需要将真实的定义添加到结果中
                // 如果存在原始定义, 则优先使用原始定义
                let origin = get_signature_origin(compilation, &signature_id);
                if let Some(origin) = origin {
                    result.insert(0, origin);
                } else {
                    result.insert(0, decl.clone());
                }
            }
            _ => continue,
        }
    }

    if !has_match {
        return None;
    }

    match result.len() {
        0 => None,
        _ => Some(result),
    }
}

fn get_signature_origin(
    compilation: &LuaCompilation,
    signature_id: &LuaSignatureId,
) -> Option<LuaSemanticDeclId> {
    let semantic_model = compilation.get_semantic_model(signature_id.get_file_id())?;
    let root = semantic_model.get_root_by_file_id(signature_id.get_file_id())?;
    let token = match root.syntax().token_at_offset(signature_id.get_position()) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, right) => {
            if left.kind() == LuaTokenKind::TkName.into() {
                left
            } else if left.kind() == LuaTokenKind::TkLeftBracket.into()
                && right.kind() == LuaTokenKind::TkInt.into()
            {
                left
            } else {
                right
            }
        }
        TokenAtOffset::None => {
            return None;
        }
    };
    let semantic_info =
        semantic_model.find_decl(NodeOrToken::Token(token), SemanticDeclLevel::default());
    semantic_info
}

pub fn find_call_expr_origin_for_decl(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    trigger_token: &LuaSyntaxToken,
    semantic_decl: &LuaSemanticDeclId,
) -> Option<LuaSemanticDeclId> {
    let call_expr = LuaCallExpr::cast(trigger_token.parent()?.parent()?)?;
    let call_function = get_call_function(semantic_model, &call_expr)?;
    let decl_id = match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => decl_id,
        _ => return None,
    };

    let typ = semantic_model.get_type(decl_id.clone().into());
    match typ {
        LuaType::DocFunction(func) => {
            if compare_function_types(semantic_model, &call_function, &func, &call_expr)? {
                return Some(decl_id.clone().into());
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
                let semantic_model = compilation.get_semantic_model(signature_id.get_file_id())?;
                let root = semantic_model.get_root_by_file_id(signature_id.get_file_id())?;
                let token = match root.syntax().token_at_offset(signature_id.get_position()) {
                    TokenAtOffset::Single(token) => token,
                    TokenAtOffset::Between(left, right) => {
                        if left.kind() == LuaTokenKind::TkName.into() {
                            left
                        } else if left.kind() == LuaTokenKind::TkLeftBracket.into()
                            && right.kind() == LuaTokenKind::TkInt.into()
                        {
                            left
                        } else {
                            right
                        }
                    }
                    TokenAtOffset::None => {
                        return None;
                    }
                };
                let semantic_info = semantic_model
                    .find_decl(NodeOrToken::Token(token), SemanticDeclLevel::default());
                if let Some(semantic_info) = semantic_info {
                    return Some(semantic_info);
                }
            }
        }
        _ => return None,
    };

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
            let mut func_params_count = func.get_params().len();
            if !func.is_colon_define() && call_expr.is_colon_call() {
                // 不是冒号定义的函数, 但是是冒号调用
                call_expr_args_count += 1;
            }
            // 如果参数有可空参数, 则需要减去
            for (_, param_type) in func.get_params().iter() {
                if let Some(param_type) = param_type {
                    if param_type.is_optional() {
                        func_params_count -= 1;
                    }
                }
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
pub fn compare_function_types(
    semantic_model: &SemanticModel,
    call_function: &LuaFunctionType,
    func: &Arc<LuaFunctionType>,
    call_expr: &LuaCallExpr,
) -> Option<bool> {
    if func.contain_tpl() {
        let instantiated_func = instantiate_func_generic(
            semantic_model.get_db(),
            &mut semantic_model.get_cache().borrow_mut(),
            func,
            call_expr.clone(),
        )
        .ok()?;
        Some(call_function == &instantiated_func)
    } else {
        Some(call_function == func.as_ref())
    }
}
