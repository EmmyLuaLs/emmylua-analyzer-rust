use std::{collections::HashSet, sync::Arc};

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaLiteralToken};
use rowan::{NodeOrToken, TextRange};

use crate::{
    DbIndex, DiagnosticCode, LuaFunctionType, LuaType, RenderLevel, SemanticModel,
    TypeCheckFailReason, TypeCheckResult,
    diagnostic::checker::assign_type_mismatch::check_table_expr,
    humanize_type, infer_call_generic,
    semantic::{
        collect_callable_overload_groups, get_func_param_type, is_func_last_param_variadic,
    },
};

use super::{Checker, DiagnosticContext};

pub struct ParamTypeCheckChecker;

impl Checker for ParamTypeCheckChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::ParamTypeMismatch,
        DiagnosticCode::AssignTypeMismatch,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            if let LuaAst::LuaCallExpr(call_expr) = node {
                check_call_expr(context, semantic_model, call_expr);
            }
        }
    }
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let (arg_types, arg_ranges): (Vec<LuaType>, Vec<TextRange>) = semantic_model
        .infer_expr_list_types(&arg_exprs, None)
        .into_iter()
        .unzip();

    // 可诊断候选函数
    let funcs = collect_diagnostic_callables(semantic_model, &call_expr)?;
    let mut candidates = funcs
        .into_iter()
        .filter(|func| callable_arity_accepts_args(semantic_model, &call_expr, &arg_exprs, func))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        // 所有候选的参数数量都不匹配时, 交给参数数量诊断器报错.
        return Some(());
    }

    let source_type = semantic_model.infer_call_receiver_type(&call_expr);
    let colon_range = call_expr
        .get_colon_token()
        .map(|token| token.get_range())
        .or_else(|| call_expr.get_prefix_expr().map(|expr| expr.get_range()));
    let mut arg_index = 0;
    loop {
        let mut has_arg = false;
        let mut next_candidates = Vec::new();
        let mut failed_param_types = Vec::new();
        let mut failed_arg = None;

        // 按参数位置逐步收窄候选, 第一处全体失败的位置就是本次诊断的位置.
        for func in &candidates {
            let Some(arg) = get_diagnostic_arg(
                &call_expr,
                func,
                &arg_types,
                &arg_ranges,
                source_type.as_ref(),
                colon_range,
                arg_index,
            ) else {
                next_candidates.push(func.clone());
                continue;
            };
            has_arg = true;

            let Some(param_type) =
                get_diagnostic_param_type(func, &call_expr, source_type.as_ref(), arg_index)
            else {
                if failed_arg.is_none() {
                    failed_arg = Some(arg);
                }
                continue;
            };

            if param_accepts_arg(semantic_model, &param_type, &arg.typ) {
                next_candidates.push(func.clone());
            } else {
                failed_param_types.push(param_type);
                if failed_arg.is_none() {
                    failed_arg = Some(arg);
                }
            }
        }

        if !has_arg {
            break;
        }

        if next_candidates.is_empty() {
            let failed_arg = failed_arg?;
            let param_type = LuaType::from_vec(failed_param_types);
            let result = semantic_model.type_check_detail(&param_type, &failed_arg.typ);
            if result.is_ok() {
                return Some(());
            }

            // 表字段已经报错了, 则不添加参数不匹配的诊断避免干扰
            if failed_arg.typ.is_table()
                && let Some(arg_expr_idx) = failed_arg.expr_index
                && let Some(arg_expr) = arg_exprs.get(arg_expr_idx)
                && let Some(add_diagnostic) = check_table_expr(
                    context,
                    semantic_model,
                    NodeOrToken::Node(arg_expr.syntax().clone()),
                    arg_expr,
                    Some(&param_type),
                )
                && add_diagnostic
            {
                return Some(());
            }

            add_diagnostic(
                context,
                semantic_model,
                failed_arg.range,
                &param_type,
                &failed_arg.typ,
                result,
            );
            break;
        }

        candidates = next_candidates;
        arg_index += 1;
    }

    Some(())
}

// 收集所有可调用的候选.
fn collect_diagnostic_callables(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Vec<Arc<LuaFunctionType>>> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let prefix_type = semantic_model.infer_expr(prefix_expr).ok()?;
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(semantic_model.get_db(), &prefix_type, &mut overload_groups)
        .ok()?;
    let raw_funcs = overload_groups.into_iter().flatten().collect::<Vec<_>>();

    let mut funcs = Vec::new();
    for func in raw_funcs {
        let func = if func.contain_tpl() {
            infer_call_generic(
                semantic_model.get_db(),
                &mut semantic_model.get_cache().borrow_mut(),
                func.as_ref(),
                call_expr.clone(),
            )
            .map(Arc::new)
            .unwrap_or(func)
        } else {
            func
        };
        funcs.push(func);
    }

    (!funcs.is_empty()).then_some(funcs)
}

// 过滤参数数量不匹配的候选
fn callable_arity_accepts_args(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    arg_exprs: &[LuaExpr],
    func: &LuaFunctionType,
) -> bool {
    // 这里只做参数数量预过滤, 具体类型是否匹配由后面的逐参数收窄处理.
    let Some(call_count) = get_call_arg_count_range(semantic_model, call_expr, arg_exprs, func)
    else {
        // 如果调用数量无法确定, 保守保留候选, 避免误报 ParamTypeMismatch.
        return true;
    };
    let param_count = get_param_count_range(semantic_model.get_db(), call_expr, func);

    // 调用方最多能提供的参数数量, 必须覆盖被调方至少需要的参数数量.
    let enough_args = call_count.max.is_none_or(|max| max >= param_count.min);
    // 调用方至少会提供的参数数量, 不能超过被调方最多能接收的参数数量.
    let not_too_many_args = param_count.max.is_none_or(|max| call_count.min <= max);

    enough_args && not_too_many_args
}

#[derive(Clone)]
struct DiagnosticArg {
    typ: LuaType,
    range: TextRange,
    expr_index: Option<usize>,
}

fn get_diagnostic_arg(
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    arg_types: &[LuaType],
    arg_ranges: &[TextRange],
    source_type: Option<&LuaType>,
    colon_range: Option<TextRange>,
    arg_index: usize,
) -> Option<DiagnosticArg> {
    // 冒号调用到非冒号定义时, 隐式 receiver 作为第 0 个实参参与类型检查.
    if call_expr.is_colon_call() && !func.is_colon_define() {
        if arg_index == 0 {
            return Some(DiagnosticArg {
                typ: source_type.cloned()?,
                range: colon_range?,
                expr_index: None,
            });
        }

        let index = arg_index - 1;
        return Some(DiagnosticArg {
            typ: arg_types.get(index)?.clone(),
            range: *arg_ranges.get(index)?,
            expr_index: Some(index),
        });
    }

    let typ = arg_types.get(arg_index)?.clone();
    Some(DiagnosticArg {
        typ,
        range: *arg_ranges.get(arg_index)?,
        expr_index: Some(arg_index),
    })
}

fn get_diagnostic_param_type(
    func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
    source_type: Option<&LuaType>,
    arg_index: usize,
) -> Option<LuaType> {
    // 点调用到冒号定义时, self 是第 0 个形参, 后续形参整体右移.
    if !call_expr.is_colon_call() && func.is_colon_define() {
        if arg_index == 0 {
            return source_type.cloned().or(Some(LuaType::SelfInfer));
        }

        return get_func_param_type(func, arg_index - 1);
    }

    get_func_param_type(func, arg_index)
}

fn param_accepts_arg(
    semantic_model: &SemanticModel,
    param_type: &LuaType,
    arg_type: &LuaType,
) -> bool {
    if param_type.is_any()
        || matches!((param_type, arg_type), (LuaType::Integer, LuaType::FloatConst(f)) if f.fract() == 0.0)
    {
        return true;
    }

    semantic_model
        .type_check_detail(param_type, arg_type)
        .is_ok()
}

#[derive(Clone, Copy)]
struct CountRange {
    min: usize,
    max: Option<usize>,
}

fn get_call_arg_count_range(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    arg_exprs: &[LuaExpr],
    func: &LuaFunctionType,
) -> Option<CountRange> {
    if arg_exprs.iter().any(|expr| {
        let LuaExpr::LiteralExpr(literal_expr) = expr else {
            return false;
        };
        matches!(literal_expr.get_literal(), Some(LuaLiteralToken::Dots(_)))
    }) {
        // `...` 无法精确给出数量范围, 交给后续类型检查和参数数量检查处理.
        return None;
    }

    let mut count = CountRange {
        min: arg_exprs.len(),
        max: Some(arg_exprs.len()),
    };

    if let Some(last_arg) = arg_exprs.last()
        && let Ok(LuaType::Variadic(variadic)) = semantic_model.infer_expr(last_arg.clone())
    {
        let base = arg_exprs.len().saturating_sub(1);
        count.min = base + variadic.get_min_len().unwrap_or(0);
        count.max = variadic.get_max_len().map(|len| base + len);
    }

    if call_expr.is_colon_call() && !func.is_colon_define() {
        count.min += 1;
        count.max = count.max.map(|max| max + 1);
    }

    Some(count)
}

fn get_param_count_range(
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
) -> CountRange {
    let mut params = func.get_params().to_vec();
    if !call_expr.is_colon_call() && func.is_colon_define() {
        params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
    }

    let mut min = 0;
    // 最小数量取最后一个非 nullable 形参, 因为前面的可选参数可以省略.
    for (idx, (name, typ)) in params.iter().enumerate() {
        if name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic()) {
            break;
        }

        if typ.as_ref().is_some_and(|typ| !is_nullable(db, typ)) {
            min = idx + 1;
        }
    }

    let max = if func.is_variadic()
        || is_func_last_param_variadic(func)
        || params
            .last()
            .is_some_and(|(_, typ)| typ.as_ref().is_some_and(|typ| typ.is_variadic()))
    {
        None
    } else {
        Some(params.len())
    };

    CountRange { min, max }
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

fn add_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    param_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    if let (LuaType::Integer, LuaType::FloatConst(f)) = (param_type, expr_type)
        && f.fract() == 0.0
    {
        return;
    }
    let db = semantic_model.get_db();
    match result {
        Ok(_) => (),
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason,
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DonotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => "type recursion".to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                range,
                t!(
                    "expected `%{source}` but found `%{found}`. %{reason}",
                    source = humanize_type(db, param_type, RenderLevel::Simple),
                    found = humanize_type(db, expr_type, RenderLevel::Simple),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}
