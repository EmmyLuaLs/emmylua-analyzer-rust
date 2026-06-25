use std::{collections::HashSet, sync::Arc};

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaGeneralToken, LuaLiteralToken,
};

use crate::{
    DbIndex, DiagnosticCode, LuaFunctionType, LuaSignatureId, LuaType, SemanticModel,
    semantic::is_func_last_param_variadic,
};

use super::super::DiagnosticContext;
use super::{ParamCountDiagnosticRanges, call_facts::CallFacts};

pub(super) fn check_call_param_count(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    facts: &CallFacts,
    param_count_diagnostic_ranges: &mut ParamCountDiagnosticRanges,
) -> Vec<Arc<LuaFunctionType>> {
    let Some(base_call_count) = get_base_call_arg_count_range(semantic_model, &facts.arg_exprs)
    else {
        // `...` 无法精确给出数量范围, 类型检查仍然需要保留所有候选.
        return facts.funcs().to_vec();
    };

    let db = semantic_model.get_db();
    let mut count_compatible_funcs = Vec::new();
    let mut best_candidate = None;
    for func in facts.funcs() {
        let mut call_count = base_call_count;
        if facts.call_expr.is_colon_call() && !func.is_colon_define() {
            // 冒号调用普通函数时, `obj:foo(x)` 等价于 `obj.foo(obj, x)`.
            // 这里要把 receiver 计入调用侧的实参槽位.
            call_count.min += 1;
            call_count.max = call_count.max.map(|max| max + 1);
        }

        let param_count = get_param_count_range(db, func, &facts.call_expr);
        let enough_args = call_count.max.is_none_or(|max| max >= param_count.min);
        let not_too_many_args = param_count.max.is_none_or(|max| call_count.min <= max);

        if enough_args && not_too_many_args {
            count_compatible_funcs.push(func.clone());
            continue;
        }

        if let Some(max_call_count) = call_count.max
            && max_call_count < param_count.min
        {
            update_best_candidate(
                &mut best_candidate,
                CountDiagnosticCandidate::Missing {
                    mismatch: param_count.min - max_call_count,
                    expected_count: param_count.min,
                    found_count: max_call_count,
                    func,
                },
            );
            continue;
        }

        if let Some(max_param_count) = param_count.max
            && call_count.min > max_param_count
        {
            update_best_candidate(
                &mut best_candidate,
                CountDiagnosticCandidate::Redundant {
                    mismatch: call_count.min - max_param_count,
                    expected_count: max_param_count,
                    found_count: call_count.min,
                    func,
                },
            );
        }
    }

    if !count_compatible_funcs.is_empty() {
        return count_compatible_funcs;
    }

    let Some(candidate) = best_candidate else {
        return count_compatible_funcs;
    };

    match candidate {
        CountDiagnosticCandidate::Missing {
            expected_count,
            found_count,
            func,
            ..
        } => emit_missing_parameter(
            context,
            db,
            &facts.call_expr,
            expected_count,
            found_count,
            func,
            param_count_diagnostic_ranges,
        ),
        CountDiagnosticCandidate::Redundant {
            expected_count,
            found_count,
            func,
            ..
        } => {
            emit_redundant_parameter(
                context,
                &facts.call_expr,
                &facts.arg_exprs,
                expected_count,
                found_count,
                func,
                param_count_diagnostic_ranges,
            );
        }
    }

    count_compatible_funcs
}

enum CountDiagnosticCandidate<'a> {
    Missing {
        mismatch: usize,
        expected_count: usize,
        found_count: usize,
        func: &'a Arc<LuaFunctionType>,
    },
    Redundant {
        mismatch: usize,
        expected_count: usize,
        found_count: usize,
        func: &'a Arc<LuaFunctionType>,
    },
}

fn update_best_candidate<'a>(
    best_candidate: &mut Option<CountDiagnosticCandidate<'a>>,
    candidate: CountDiagnosticCandidate<'a>,
) {
    if best_candidate
        .as_ref()
        .is_none_or(|current| candidate.is_better_than(current))
    {
        *best_candidate = Some(candidate);
    }
}

impl CountDiagnosticCandidate<'_> {
    fn is_better_than(&self, other: &Self) -> bool {
        match self.mismatch().cmp(&other.mismatch()) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => self.is_better_tie_than(other),
        }
    }

    fn mismatch(&self) -> usize {
        match self {
            CountDiagnosticCandidate::Missing { mismatch, .. }
            | CountDiagnosticCandidate::Redundant { mismatch, .. } => *mismatch,
        }
    }

    fn is_better_tie_than(&self, other: &Self) -> bool {
        match (self, other) {
            (
                CountDiagnosticCandidate::Missing {
                    expected_count: left,
                    ..
                },
                CountDiagnosticCandidate::Missing {
                    expected_count: right,
                    ..
                },
            ) => left < right,
            (
                CountDiagnosticCandidate::Redundant {
                    expected_count: left,
                    ..
                },
                CountDiagnosticCandidate::Redundant {
                    expected_count: right,
                    ..
                },
            ) => left > right,
            (
                CountDiagnosticCandidate::Missing { .. },
                CountDiagnosticCandidate::Redundant { .. },
            ) => true,
            (
                CountDiagnosticCandidate::Redundant { .. },
                CountDiagnosticCandidate::Missing { .. },
            ) => false,
        }
    }
}

fn emit_missing_parameter(
    context: &mut DiagnosticContext,
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    expected_count: usize,
    found_count: usize,
    func: &Arc<LuaFunctionType>,
    param_count_diagnostic_ranges: &mut ParamCountDiagnosticRanges,
) {
    let mut miss_parameter_info = Vec::new();

    for param_index in found_count..expected_count {
        add_missing_parameter_info(db, call_expr, func, param_index, &mut miss_parameter_info);
    }

    if !miss_parameter_info.is_empty() {
        let Some(args_list) = call_expr.get_args_list() else {
            return;
        };
        let Some(right_paren) = args_list.tokens::<LuaGeneralToken>().last() else {
            return;
        };
        let range = right_paren.get_range();
        param_count_diagnostic_ranges.insert(range);
        context.add_diagnostic(
            DiagnosticCode::MissingParameter,
            range,
            t!(
                "expected %{num} parameters but found %{found_num}. %{infos}",
                num = expected_count,
                found_num = found_count,
                infos = miss_parameter_info.join(" \n ")
            )
            .to_string(),
            None,
        );
    }
}

fn emit_redundant_parameter(
    context: &mut DiagnosticContext,
    call_expr: &LuaCallExpr,
    call_args: &[LuaExpr],
    expected_count: usize,
    found_count: usize,
    func: &Arc<LuaFunctionType>,
    param_count_diagnostic_ranges: &mut ParamCountDiagnosticRanges,
) {
    let implicit_receiver_offset =
        usize::from(call_expr.is_colon_call() && !func.is_colon_define());
    for (i, arg) in call_args.iter().enumerate() {
        if i + implicit_receiver_offset < expected_count {
            continue;
        }

        let range = arg.get_range();
        param_count_diagnostic_ranges.insert(range);
        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            range,
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = expected_count,
                found_num = found_count,
            )
            .to_string(),
            None,
        );
    }
}

fn add_missing_parameter_info(
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    adjusted_index: usize,
    miss_parameter_info: &mut Vec<String>,
) {
    if !call_expr.is_colon_call() && func.is_colon_define() {
        if adjusted_index == 0 {
            if !is_nullable(db, &LuaType::SelfInfer) {
                miss_parameter_info
                    .push(t!("missing parameter: %{name}", name = "self",).to_string());
            }
            return;
        }
        let Some((name, typ)) = func.get_params().get(adjusted_index - 1) else {
            return;
        };
        if let Some(typ) = typ
            && !is_nullable(db, typ)
        {
            miss_parameter_info.push(t!("missing parameter: %{name}", name = name,).to_string());
        }
        return;
    }

    let Some((name, typ)) = func.get_params().get(adjusted_index) else {
        return;
    };
    if let Some(typ) = typ
        && !is_nullable(db, typ)
    {
        miss_parameter_info.push(t!("missing parameter: %{name}", name = name,).to_string());
    }
}

#[derive(Clone, Copy)]
struct CountRange {
    // 数量下界: 调用侧至少会提供多少, 或函数侧至少要求多少.
    min: usize,
    // 数量上界: 调用侧最多会提供多少, 或函数侧最多接受多少; None 表示无上限.
    max: Option<usize>,
}

fn get_base_call_arg_count_range(
    semantic_model: &SemanticModel,
    arg_exprs: &[LuaExpr],
) -> Option<CountRange> {
    if arg_exprs.iter().any(|expr| {
        if let LuaExpr::LiteralExpr(literal_expr) = expr
            && let Some(LuaLiteralToken::Dots(_)) = literal_expr.get_literal()
        {
            return true;
        }

        false
    }) {
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
    Some(count)
}

// 计算当前候选函数签名能接受多少个形参槽位.
fn get_param_count_range(
    db: &DbIndex,
    func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
) -> CountRange {
    let params = func.get_params();
    // 如果以点调用但函数是冒号定义, 则表示需要传入 self 参数.
    let self_offset = usize::from(!call_expr.is_colon_call() && func.is_colon_define());

    let mut min = self_offset;
    // 最小数量取最后一个非 nullable 形参, 因为前面的可选参数可以省略.
    for (idx, (name, typ)) in params.iter().enumerate() {
        if name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic()) {
            break;
        }

        if typ.as_ref().is_some_and(|typ| !is_nullable(db, typ)) {
            min = idx + self_offset + 1;
        }
    }

    let adjusted_len = params.len() + self_offset;
    let max = if func.is_variadic()
        || is_func_last_param_variadic(func)
        || params
            .last()
            .is_some_and(|(_, typ)| typ.as_ref().is_some_and(|typ| typ.is_variadic()))
    {
        None
    } else {
        Some(adjusted_len)
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

fn get_params_len(params: &[(String, Option<LuaType>)]) -> Option<usize> {
    if let Some((name, typ)) = params.last() {
        // 如果最后一个参数是可变参数, 则直接返回, 不需要检查.
        if name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic()) {
            return None;
        }
    }
    Some(params.len())
}

pub(super) fn check_closure_param_count(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure_expr: &LuaClosureExpr,
) {
    let Some(current_signature) =
        context
            .get_db()
            .get_signature_index()
            .get(&LuaSignatureId::from_closure(
                semantic_model.get_file_id(),
                closure_expr,
            ))
    else {
        return;
    };

    let Some(source_typ) = semantic_model.infer_bind_value_type(closure_expr.clone().into()) else {
        return;
    };

    let Some(source_params_len) = (match &source_typ {
        LuaType::DocFunction(func_type) => get_params_len(func_type.get_params()),
        LuaType::Signature(signature_id) => {
            let Some(signature) = context.get_db().get_signature_index().get(signature_id) else {
                return;
            };
            let params = signature.get_type_params();
            get_params_len(&params)
        }
        _ => return,
    }) else {
        return;
    };

    // 只检查右值参数多于左值参数的情况, 右值参数少于左值参数的情况是能够接受的.
    if source_params_len > current_signature.params.len() {
        return;
    }
    let found_num = current_signature.params.len();
    let Some(params_list) = closure_expr.get_params_list() else {
        return;
    };
    let params = params_list.get_params().collect::<Vec<_>>();

    for param in params[source_params_len..].iter() {
        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            param.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = source_params_len,
                found_num = found_num,
            )
            .to_string(),
            None,
        );
    }
}
