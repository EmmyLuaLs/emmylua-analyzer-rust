use std::collections::HashSet;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaGeneralToken,
    LuaLiteralToken,
};

use crate::{DbIndex, DiagnosticCode, LuaSignatureId, LuaType, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct CheckParamCountChecker;

impl Checker for CheckParamCountChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::MissingParameter,
        DiagnosticCode::RedundantParameter,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        for node in semantic_model.get_root().descendants::<LuaAst>() {
            match node {
                LuaAst::LuaCallExpr(call_expr) => {
                    check_call_expr(context, semantic_model, call_expr);
                }
                LuaAst::LuaClosureExpr(closure_expr) => {
                    check_closure_expr(context, semantic_model, &closure_expr);
                }
                _ => {}
            }
        }
    }
}

/// 处理左值已绑定类型但右值为匿名函数的情况
fn check_closure_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) -> Option<()> {
    let current_signature = context
        .db
        .get_signature_index()
        .get(&LuaSignatureId::from_closure(
            semantic_model.get_file_id(),
            closure_expr,
        ))?;

    let source_typ = semantic_model.infer_bind_value_type(closure_expr.clone().into())?;

    let source_params_len = match &source_typ {
        LuaType::DocFunction(func_type) => {
            let params = func_type.get_params();
            get_params_len(params)
        }
        LuaType::Signature(signature_id) => {
            let signature = context.db.get_signature_index().get(signature_id)?;
            let params = signature.get_type_params();
            get_params_len(&params)
        }
        _ => return Some(()),
    }?;

    // 只检查右值参数多于左值参数的情况, 右值参数少于左值参数的情况是能够接受的
    if source_params_len > current_signature.params.len() {
        return Some(());
    }
    let params = closure_expr
        .get_params_list()?
        .get_params()
        .collect::<Vec<_>>();

    for param in params[source_params_len..].iter() {
        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            param.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = source_params_len,
                found_num = current_signature.params.len(),
            )
            .to_string(),
            None,
        );
    }

    Some(())
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let mut fake_params = func.get_params().to_vec();
    let call_args = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let mut call_args_count = call_args.len();
    let last_arg_is_dots = call_args.last().is_some_and(is_dots_expr);
    // 根据冒号定义与冒号调用的情况来调整调用参数的数量
    let colon_call = call_expr.is_colon_call();
    let colon_define = func.is_colon_define();
    match (colon_call, colon_define) {
        (true, true) | (false, false) => {}
        (false, true) => {
            fake_params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
        }
        (true, false) => {
            call_args_count += 1;
        }
    }

    // Check for missing parameters
    if call_args_count < fake_params.len() {
        // 调用参数包含 `...`
        for arg in call_args.iter() {
            if let LuaExpr::LiteralExpr(literal_expr) = arg
                && let Some(LuaLiteralToken::Dots(_)) = literal_expr.get_literal()
            {
                return Some(());
            }
        }
        // 对调用参数的最后一个参数进行特殊处理
        if let Some(last_arg) = call_args.last()
            && let Ok(LuaType::Variadic(variadic)) = semantic_model.infer_expr(last_arg.clone())
        {
            let len = match variadic.get_max_len() {
                Some(len) => len,
                None => {
                    return Some(());
                }
            };
            call_args_count = call_args_count + len - 1;
            if call_args_count >= fake_params.len() {
                return Some(());
            }
        }

        let mut miss_parameter_info = Vec::new();

        for i in call_args_count..fake_params.len() {
            let param_info = fake_params.get(i)?;
            if param_info.0 == "..." {
                break;
            }

            let typ = param_info.1.clone();
            if let Some(typ) = typ
                && !is_nullable(context.db, &typ)
            {
                miss_parameter_info.push(t!("missing parameter: %{name}", name = param_info.0,));
            }
        }

        if !miss_parameter_info.is_empty() {
            let right_paren = call_expr
                .get_args_list()?
                .tokens::<LuaGeneralToken>()
                .last()?;
            context.add_diagnostic(
                DiagnosticCode::MissingParameter,
                right_paren.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}. %{infos}",
                    num = fake_params.len(),
                    found_num = call_args_count,
                    infos = miss_parameter_info.join(" \n ")
                )
                .to_string(),
                None,
            );
        }
    }
    // Check for redundant parameters
    else {
        let mut min_call_args_count = call_args_count;
        if last_arg_is_dots {
            min_call_args_count = min_call_args_count.saturating_sub(1);
        }

        if min_call_args_count <= fake_params.len() {
            return Some(());
        }

        // 参数定义中最后一个参数是 `...`
        if fake_params.last().is_some_and(|(name, typ)| {
            name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic())
        }) {
            return Some(());
        }

        let mut adjusted_index = 0;
        if colon_call != colon_define {
            adjusted_index = if colon_define && !colon_call { -1 } else { 1 };
        }

        for (i, arg) in call_args.iter().enumerate() {
            if last_arg_is_dots && i + 1 == call_args.len() {
                continue;
            }

            let param_index = i as isize + adjusted_index;

            if param_index < 0 || param_index < fake_params.len() as isize {
                continue;
            }

            context.add_diagnostic(
                DiagnosticCode::RedundantParameter,
                arg.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}",
                    num = fake_params.len(),
                    found_num = min_call_args_count,
                )
                .to_string(),
                None,
            );
        }
    }

    Some(())
}

fn is_dots_expr(expr: &LuaExpr) -> bool {
    if let LuaExpr::LiteralExpr(literal_expr) = expr
        && let Some(LuaLiteralToken::Dots(_)) = literal_expr.get_literal()
    {
        return true;
    }
    false
}

fn get_params_len(params: &[(String, Option<LuaType>)]) -> Option<usize> {
    if let Some((name, typ)) = params.last() {
        // 如果最后一个参数是可变参数, 则直接返回, 不需要检查
        if name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic()) {
            return None;
        }
    }
    Some(params.len())
}

fn is_nullable(db: &DbIndex, typ: &LuaType) -> bool {
    let mut stack: Vec<LuaType> = Vec::new();
    stack.push(typ.clone());
    let mut visited = HashSet::new();
    while let Some(typ) = stack.pop() {
        if visited.contains(&typ) {
            continue;
        }
        visited.insert(typ.clone());
        match typ {
            LuaType::Any | LuaType::Unknown | LuaType::Nil => return true,
            LuaType::Ref(decl_id) => {
                if let Some(decl) = db.get_type_index().get_type_decl(&decl_id)
                    && decl.is_alias()
                    && let Some(alias_origin) = decl.get_alias_ref()
                {
                    stack.push(alias_origin.clone());
                }
            }
            LuaType::Union(u) => {
                for t in u.into_vec() {
                    stack.push(t);
                }
            }
            LuaType::MultiLineUnion(m) => {
                for (t, _) in m.get_unions() {
                    stack.push(t.clone());
                }
            }
            _ => {}
        }
    }
    false
}
