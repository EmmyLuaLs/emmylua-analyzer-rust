use std::collections::HashSet;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaGeneralToken, LuaLiteralToken,
};

use crate::{
    DbIndex, DiagnosticCode, LuaFunctionType, LuaSignatureId, LuaType, SemanticModel,
    semantic::is_func_last_param_variadic,
};

use super::{super::DiagnosticContext, call_analysis::CallAnalysis};

pub(super) struct ArityAnalysis {
    // 调用侧参数数量是否可确定. 出现无法展开的 `...` 时为 false.
    pub(super) count_is_known: bool,
    // 参数数量兼容的候选索引.
    pub(super) compatible_candidates: Vec<usize>,
    best_missing: Option<ArityDiagnosticCandidate>,
    best_redundant: Option<ArityDiagnosticCandidate>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArityDiagnosticKind {
    Missing,
    Redundant,
}

#[derive(Clone, Copy)]
struct ArityDiagnosticCandidate {
    kind: ArityDiagnosticKind,
    candidate_index: usize,
    mismatch: usize,
    expected_count: usize,
    found_count: usize,
}

impl ArityDiagnosticCandidate {
    fn is_better_than(&self, other: &Self) -> bool {
        match self.mismatch.cmp(&other.mismatch) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => match (self.kind, other.kind) {
                (ArityDiagnosticKind::Missing, ArityDiagnosticKind::Missing) => {
                    self.expected_count < other.expected_count
                }
                (ArityDiagnosticKind::Redundant, ArityDiagnosticKind::Redundant) => {
                    self.expected_count > other.expected_count
                }
                (ArityDiagnosticKind::Missing, ArityDiagnosticKind::Redundant) => true,
                (ArityDiagnosticKind::Redundant, ArityDiagnosticKind::Missing) => false,
            },
        }
    }
}

pub(super) fn analyze_call_arity(
    semantic_model: &SemanticModel,
    call: &CallAnalysis,
) -> ArityAnalysis {
    let Some(base_call_count) = get_base_call_arg_count_range(semantic_model, &call.arg_exprs)
    else {
        // `...` 无法给出确定数量范围, 类型检查必须保留全部候选.
        return ArityAnalysis {
            count_is_known: false,
            compatible_candidates: Vec::new(),
            best_missing: None,
            best_redundant: None,
        };
    };

    let db = semantic_model.get_db();
    let mut analysis = ArityAnalysis {
        count_is_known: true,
        compatible_candidates: Vec::with_capacity(call.candidates().len()),
        best_missing: None,
        best_redundant: None,
    };
    for (candidate_index, candidate) in call.candidates().iter().enumerate() {
        let func = &candidate.instantiated;
        let mut call_count = base_call_count;
        if call.call_expr.is_colon_call() && !func.is_colon_define() {
            // 冒号调用普通函数时, receiver 会占用一个实参槽位.
            call_count.min += 1;
            call_count.max = call_count.max.map(|max| max + 1);
        }

        let param_count = get_param_count_range(db, func, &candidate.original, &call.call_expr);
        let enough_args = call_count.max.is_none_or(|max| max >= param_count.min);
        let not_too_many_args = param_count.max.is_none_or(|max| call_count.min <= max);
        if enough_args && not_too_many_args {
            analysis.compatible_candidates.push(candidate_index);
            continue;
        }

        if let Some(max_call_count) = call_count.max
            && max_call_count < param_count.min
        {
            update_best_candidate(
                &mut analysis.best_missing,
                ArityDiagnosticCandidate {
                    kind: ArityDiagnosticKind::Missing,
                    candidate_index,
                    mismatch: param_count.min - max_call_count,
                    expected_count: param_count.min,
                    found_count: max_call_count,
                },
            );
            continue;
        }

        if let Some(max_param_count) = param_count.max
            && call_count.min > max_param_count
        {
            update_best_candidate(
                &mut analysis.best_redundant,
                ArityDiagnosticCandidate {
                    kind: ArityDiagnosticKind::Redundant,
                    candidate_index,
                    mismatch: call_count.min - max_param_count,
                    expected_count: max_param_count,
                    found_count: call_count.min,
                },
            );
        }
    }

    analysis
}

fn update_best_candidate(
    best_candidate: &mut Option<ArityDiagnosticCandidate>,
    candidate: ArityDiagnosticCandidate,
) {
    if best_candidate
        .as_ref()
        .is_none_or(|current| candidate.is_better_than(current))
    {
        *best_candidate = Some(candidate);
    }
}

pub(super) fn add_call_arity_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call: &CallAnalysis,
    analysis: &ArityAnalysis,
    missing_enabled: bool,
    redundant_enabled: bool,
) -> bool {
    let missing = analysis.best_missing.filter(|_| missing_enabled);
    let redundant = analysis.best_redundant.filter(|_| redundant_enabled);
    let candidate = match (missing, redundant) {
        (Some(missing), Some(redundant)) => {
            if missing.is_better_than(&redundant) {
                missing
            } else {
                redundant
            }
        }
        (Some(candidate), None) | (None, Some(candidate)) => candidate,
        (None, None) => return false,
    };
    let Some(call_candidate) = call.candidates().get(candidate.candidate_index) else {
        return false;
    };

    match candidate.kind {
        ArityDiagnosticKind::Missing => add_missing_parameter_diagnostic(
            context,
            semantic_model.get_db(),
            &call.call_expr,
            candidate.expected_count,
            candidate.found_count,
            &call_candidate.instantiated,
            &call_candidate.original,
        ),
        ArityDiagnosticKind::Redundant => add_redundant_parameter_diagnostic(
            context,
            &call.call_expr,
            &call.arg_exprs,
            candidate.expected_count,
            candidate.found_count,
            &call_candidate.instantiated,
        ),
    }
}

fn add_missing_parameter_diagnostic(
    context: &mut DiagnosticContext,
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    expected_count: usize,
    found_count: usize,
    func: &LuaFunctionType,
    original_func: &LuaFunctionType,
) -> bool {
    let mut missing_parameter_info = Vec::new();

    for param_index in found_count..expected_count {
        add_missing_parameter_info(
            db,
            call_expr,
            func,
            original_func,
            param_index,
            &mut missing_parameter_info,
        );
    }

    if missing_parameter_info.is_empty() {
        return false;
    }
    let Some(args_list) = call_expr.get_args_list() else {
        return false;
    };
    let Some(right_paren) = args_list.tokens::<LuaGeneralToken>().last() else {
        return false;
    };
    context.add_diagnostic(
        DiagnosticCode::MissingParameter,
        right_paren.get_range(),
        t!(
            "expected %{num} parameters but found %{found_num}. %{infos}",
            num = expected_count,
            found_num = found_count,
            infos = missing_parameter_info.join(" \n ")
        )
        .to_string(),
        None,
    )
}

fn add_redundant_parameter_diagnostic(
    context: &mut DiagnosticContext,
    call_expr: &LuaCallExpr,
    call_args: &[LuaExpr],
    expected_count: usize,
    found_count: usize,
    func: &LuaFunctionType,
) -> bool {
    let implicit_receiver_offset =
        usize::from(call_expr.is_colon_call() && !func.is_colon_define());
    let mut diagnostic_reported = false;
    for (index, arg) in call_args.iter().enumerate() {
        if index + implicit_receiver_offset < expected_count {
            continue;
        }

        diagnostic_reported |= context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            arg.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = expected_count,
                found_num = found_count,
            )
            .to_string(),
            None,
        );
    }
    diagnostic_reported
}

fn add_missing_parameter_info(
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
    original_func: &LuaFunctionType,
    adjusted_index: usize,
    missing_parameter_info: &mut Vec<String>,
) {
    if !call_expr.is_colon_call() && func.is_colon_define() {
        if adjusted_index == 0 {
            if !is_nullable(db, &LuaType::SelfInfer, None) {
                missing_parameter_info
                    .push(t!("missing parameter: %{name}", name = "self",).to_string());
            }
            return;
        }
        let Some((name, typ)) = func.get_params().get(adjusted_index - 1) else {
            return;
        };
        let original_typ = original_func
            .get_params()
            .get(adjusted_index - 1)
            .and_then(|(_, typ)| typ.as_ref());
        if let Some(typ) = typ
            && !is_nullable(db, typ, original_typ)
        {
            missing_parameter_info.push(t!("missing parameter: %{name}", name = name,).to_string());
        }
        return;
    }

    let Some((name, typ)) = func.get_params().get(adjusted_index) else {
        return;
    };
    let original_typ = original_func
        .get_params()
        .get(adjusted_index)
        .and_then(|(_, typ)| typ.as_ref());
    if let Some(typ) = typ
        && !is_nullable(db, typ, original_typ)
    {
        missing_parameter_info.push(t!("missing parameter: %{name}", name = name,).to_string());
    }
}

#[derive(Clone, Copy)]
struct CountRange {
    // 数量下界: 调用侧至少提供多少, 或函数侧至少要求多少.
    min: usize,
    // 数量上界: 调用侧最多提供多少, 或函数侧最多接受多少; None 表示无上限.
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

// 计算当前候选签名能够接受的形参槽位范围.
fn get_param_count_range(
    db: &DbIndex,
    func: &LuaFunctionType,
    original_func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
) -> CountRange {
    let params = func.get_params();
    let original_params = original_func.get_params();
    let self_offset = usize::from(!call_expr.is_colon_call() && func.is_colon_define());

    let mut min = self_offset;
    for (index, (name, typ)) in params.iter().enumerate() {
        if name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic()) {
            break;
        }

        let original_typ = original_params.get(index).and_then(|(_, typ)| typ.as_ref());
        if typ
            .as_ref()
            .is_some_and(|typ| !is_nullable(db, typ, original_typ))
        {
            min = index + self_offset + 1;
        }
    }

    let max = if func.is_variadic() || is_func_last_param_variadic(func) {
        None
    } else {
        get_params_len(params).map(|len| len + self_offset)
    };

    CountRange { min, max }
}

fn is_nullable(db: &DbIndex, typ: &LuaType, original_typ: Option<&LuaType>) -> bool {
    match typ {
        LuaType::Any | LuaType::Nil => true,
        LuaType::Unknown => {
            if let Some(original_typ) = original_typ
                && original_typ.contain_tpl()
            {
                return is_nullable(db, original_typ, None);
            }
            true
        }
        LuaType::Ref(_) | LuaType::Union(_) | LuaType::MultiLineUnion(_) => {
            is_composite_nullable(db, typ, original_typ)
        }
        _ => false,
    }
}

fn is_composite_nullable(db: &DbIndex, typ: &LuaType, original_typ: Option<&LuaType>) -> bool {
    let mut stack = vec![typ.clone()];
    let mut visited = HashSet::new();
    while let Some(typ) = stack.pop() {
        if !visited.insert(typ.clone()) {
            continue;
        }
        match typ {
            LuaType::Any | LuaType::Nil => return true,
            LuaType::Unknown => {
                if let Some(original_typ) = original_typ
                    && original_typ.contain_tpl()
                {
                    return is_nullable(db, original_typ, None);
                }
                return true;
            }
            LuaType::Ref(decl_id) => {
                if let Some(decl) = db.get_type_index().get_type_decl(&decl_id)
                    && decl.is_alias()
                    && let Some(alias_origin) = decl.get_alias_ref()
                {
                    stack.push(alias_origin.clone());
                }
            }
            LuaType::Union(union) => stack.extend(union.into_vec()),
            LuaType::MultiLineUnion(union) => {
                stack.extend(union.get_unions().iter().map(|(typ, _)| typ.clone()));
            }
            _ => {}
        }
    }
    false
}

fn get_params_len(params: &[(String, Option<LuaType>)]) -> Option<usize> {
    if let Some((name, typ)) = params.last()
        && (name == "..." || typ.as_ref().is_some_and(|typ| typ.is_variadic()))
    {
        return None;
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

    // 只检查右值参数多于左值参数的情况, 右值参数较少时可以接受.
    if source_params_len > current_signature.params.len() {
        return;
    }
    let found_num = current_signature.params.len();
    let Some(params_list) = closure_expr.get_params_list() else {
        return;
    };
    let params = params_list.get_params().collect::<Vec<_>>();

    for param in &params[source_params_len..] {
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
