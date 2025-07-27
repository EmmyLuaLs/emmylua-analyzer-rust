use emmylua_code_analysis::{
    LuaCompilation, LuaDeclId, LuaFunctionType, LuaSemanticDeclId, LuaSignature, LuaSignatureId,
    LuaType, SemanticDeclLevel, SemanticModel, instantiate_func_generic,
};
use emmylua_parser::{
    LuaAstNode, LuaCallExpr, LuaExpr, LuaLiteralToken, LuaSyntaxToken, LuaTokenKind,
};
use rowan::{NodeOrToken, TokenAtOffset};
use std::sync::Arc;

pub fn find_matching_function_definitions(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    trigger_token: &LuaSyntaxToken,
    semantic_decls: &Vec<LuaSemanticDeclId>,
) -> Option<Vec<LuaSemanticDeclId>> {
    let call_expr = LuaCallExpr::cast(trigger_token.parent()?.parent()?)?;
    let call_function = get_call_function(semantic_model, &call_expr)?;

    let member_decls: Vec<_> = semantic_decls
        .iter()
        .filter_map(|decl| match decl {
            LuaSemanticDeclId::Member(member_id) => Some((decl, member_id)),
            _ => None,
        })
        .collect();

    let mut result = Vec::new();
    let mut has_match = false;

    for (decl, member_id) in member_decls {
        let typ = semantic_model.get_type(member_id.clone().into());
        match typ {
            LuaType::DocFunction(func) => {
                if compare_function_types(semantic_model, &call_function, &func, &call_expr)
                    .unwrap_or(false)
                {
                    result.push(decl.clone());
                    has_match = true;
                }
            }
            LuaType::Signature(signature_id) => {
                let signature = match semantic_model
                    .get_db()
                    .get_signature_index()
                    .get(&signature_id)
                {
                    Some(sig) => sig,
                    None => continue,
                };
                let functions = get_signature_functions(signature);

                if functions.iter().any(|func| {
                    compare_function_types(semantic_model, &call_function, func, &call_expr)
                        .unwrap_or(false)
                }) {
                    has_match = true;
                }

                // 无论是否匹配, 都需要将真实的定义添加到结果中
                // 如果存在原始定义, 则优先使用原始定义
                let origin = extract_semantic_decl_from_signature(compilation, &signature_id);
                if let Some(origin) = origin {
                    result.insert(0, origin);
                } else {
                    result.insert(0, decl.clone());
                }
            }
            _ => continue,
        }
    }

    if has_match && !result.is_empty() {
        Some(result)
    } else {
        None
    }
}

pub fn find_function_call_origin(
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

    match_function_with_call(
        semantic_model,
        compilation,
        &call_function,
        &call_expr,
        decl_id,
    )
}

fn match_function_with_call(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    call_function: &Arc<LuaFunctionType>,
    call_expr: &LuaCallExpr,
    decl_id: &LuaDeclId,
) -> Option<LuaSemanticDeclId> {
    let typ = semantic_model.get_type(decl_id.clone().into());
    match typ {
        LuaType::DocFunction(func) => {
            if compare_function_types(semantic_model, call_function, &func, call_expr)
                .unwrap_or(false)
            {
                Some(decl_id.clone().into())
            } else {
                None
            }
        }
        LuaType::Signature(signature_id) => handle_signature_match(
            semantic_model,
            compilation,
            call_function,
            call_expr,
            &signature_id,
        ),
        _ => None,
    }
}

fn handle_signature_match(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    call_function: &Arc<LuaFunctionType>,
    call_expr: &LuaCallExpr,
    signature_id: &LuaSignatureId,
) -> Option<LuaSemanticDeclId> {
    let signature = semantic_model
        .get_db()
        .get_signature_index()
        .get(signature_id)?;
    let functions = get_signature_functions(signature);

    if functions.iter().any(|func| {
        compare_function_types(semantic_model, call_function, func, call_expr).unwrap_or(false)
    }) {
        extract_semantic_decl_from_signature(compilation, signature_id)
    } else {
        None
    }
}

/// 从函数签名中提取其定义的语义ID
pub fn extract_semantic_decl_from_signature(
    compilation: &LuaCompilation,
    signature_id: &LuaSignatureId,
) -> Option<LuaSemanticDeclId> {
    let semantic_model = compilation.get_semantic_model(signature_id.get_file_id())?;
    let root = semantic_model.get_root_by_file_id(signature_id.get_file_id())?;
    let token = match root.syntax().token_at_offset(signature_id.get_position()) {
        TokenAtOffset::Single(token) => Some(token),
        TokenAtOffset::Between(left, right) => {
            if left.kind() == LuaTokenKind::TkName.into() {
                Some(left)
            } else if left.kind() == LuaTokenKind::TkLeftBracket.into()
                && right.kind() == LuaTokenKind::TkInt.into()
            {
                Some(left)
            } else {
                Some(right)
            }
        }
        TokenAtOffset::None => None,
    }?;
    semantic_model.find_decl(NodeOrToken::Token(token), SemanticDeclLevel::default())
}

/// 获取最匹配的函数(并不能确保完全匹配)
fn get_call_function(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Arc<LuaFunctionType>> {
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None);
    if let Some(func) = func {
        if check_params_count_is_match(semantic_model, &func, call_expr.clone()).unwrap_or(false) {
            return Some(func);
        }
    }
    None
}

fn check_params_count_is_match(
    semantic_model: &SemanticModel,
    call_function: &LuaFunctionType,
    call_expr: LuaCallExpr,
) -> Option<bool> {
    let mut fake_params = call_function.get_params().to_vec();
    let call_args = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let mut call_args_count = call_args.len();
    let colon_call = call_expr.is_colon_call();
    let colon_define = call_function.is_colon_define();
    match (colon_call, colon_define) {
        (true, true) | (false, false) => {}
        (false, true) => {
            fake_params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
        }
        (true, false) => {
            call_args_count += 1;
        }
    }
    if call_args_count < fake_params.len() {
        // 调用参数包含 `...`
        for arg in call_args.iter() {
            if let LuaExpr::LiteralExpr(literal_expr) = arg {
                if let Some(literal_token) = literal_expr.get_literal() {
                    if let LuaLiteralToken::Dots(_) = literal_token {
                        return Some(true);
                    }
                }
            }
        }
        // 对调用参数的最后一个参数进行特殊处理
        if let Some(last_arg) = call_args.last() {
            if let Ok(LuaType::Variadic(variadic)) = semantic_model.infer_expr(last_arg.clone()) {
                let len = match variadic.get_max_len() {
                    Some(len) => len,
                    None => {
                        return Some(true);
                    }
                };
                call_args_count = call_args_count + len as usize - 1;
                if call_args_count >= fake_params.len() {
                    return Some(true);
                }
            }
        }

        for i in call_args_count..fake_params.len() {
            let param_info = fake_params.get(i)?;
            if param_info.0 == "..." {
                return Some(true);
            }

            let typ = param_info.1.clone();
            if let Some(typ) = typ {
                if !typ.is_optional() {
                    return Some(false);
                }
            }
        }
    } else if call_args_count > fake_params.len() {
        // 参数定义中最后一个参数是 `...`
        if fake_params.last().map_or(false, |(name, typ)| {
            name == "..."
                || if let Some(typ) = typ {
                    typ.is_variadic()
                } else {
                    false
                }
        }) {
            return Some(true);
        }

        let mut adjusted_index = 0;
        if colon_call != colon_define {
            adjusted_index = if colon_define && !colon_call { -1 } else { 1 };
        }

        for (i, _) in call_args.iter().enumerate() {
            let param_index = i as isize + adjusted_index;
            if param_index < 0 || param_index < fake_params.len() as isize {
                continue;
            }
            return Some(false);
        }
    }
    Some(true)
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
