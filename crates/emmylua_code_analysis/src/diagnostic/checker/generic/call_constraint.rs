use std::sync::Arc;

use emmylua_parser::{LuaAstToken, LuaCallExpr};
use rowan::TextRange;

use crate::{
    DbIndex, LuaAliasCallKind, LuaFunctionType, LuaType, LuaTypeNode, SemanticModel,
    TypeSubstitutor, build_call_generic_substitutor, collect_callable_overload_groups,
    instantiate_type_generic,
};

// 泛型约束上下文
pub(super) struct CallConstraintContext {
    pub params: Vec<(String, Option<LuaType>)>,
    pub args: Vec<CallConstraintArg>,
    pub substitutor: TypeSubstitutor,
}

pub(super) struct CallConstraintArg {
    pub raw_type: LuaType,
    pub range: TextRange,
}

struct CallConstraintCandidate {
    doc_func: Arc<LuaFunctionType>,
    substitutor: Option<TypeSubstitutor>,
    generic_arg_count: usize,
}

pub(super) fn build_call_constraint_context(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<CallConstraintContext> {
    let mut args = get_arg_infos(semantic_model, call_expr)?;
    let call_arg_types = args
        .iter()
        .map(|arg| arg.raw_type.clone())
        .collect::<Vec<_>>();
    let CallConstraintCandidate {
        doc_func,
        substitutor,
        ..
    } = get_call_doc_func(semantic_model, call_expr, &call_arg_types)?;

    let mut params = doc_func.get_params().to_vec();
    let substitutor = substitutor.or_else(|| {
        build_call_generic_substitutor(
            semantic_model.get_db(),
            &mut semantic_model.get_cache().borrow_mut(),
            &doc_func,
            call_expr,
        )
        .ok()
    })?;

    // 处理冒号调用与函数定义在 self 参数上的差异
    match (call_expr.is_colon_call(), doc_func.is_colon_define()) {
        (true, true) | (false, false) => {}
        (false, true) => {
            params.insert(0, ("self".into(), Some(LuaType::SelfInfer)));
        }
        (true, false) => {
            let self_type = semantic_model.resolve_call_self_type(call_expr)?;
            args.insert(
                0,
                CallConstraintArg {
                    raw_type: self_type,
                    range: call_expr.get_colon_token()?.get_range(),
                },
            );
        }
    }

    Some(CallConstraintContext {
        params,
        args,
        substitutor,
    })
}

fn get_call_doc_func(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
    call_arg_types: &[LuaType],
) -> Option<CallConstraintCandidate> {
    let prefix_expr = call_expr.get_prefix_expr()?.clone();
    let callable_type = semantic_model.infer_expr(prefix_expr).ok()?;
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(
        semantic_model.get_db(),
        &callable_type,
        &mut overload_groups,
    )
    .ok()?;

    let mut selected = None;
    for func in overload_groups.into_iter().flatten() {
        let substitutor = if func.contain_tpl() {
            build_call_generic_substitutor(
                semantic_model.get_db(),
                &mut semantic_model.get_cache().borrow_mut(),
                &func,
                call_expr,
            )
            .ok()
        } else {
            None
        };
        let match_func = if let Some(substitutor) = substitutor.as_ref() {
            let func_type = LuaType::DocFunction(func.clone());
            match instantiate_type_generic(semantic_model.get_db(), &func_type, substitutor) {
                LuaType::DocFunction(func) => func,
                _ => func.clone(),
            }
        } else {
            func.clone()
        };

        if !semantic_model.callable_accepts_args(
            &match_func,
            call_arg_types,
            call_expr.is_colon_call(),
            None,
        ) {
            continue;
        }

        let generic_arg_count = generic_arg_count(func.as_ref(), call_expr, call_arg_types);
        // 诊断阶段会遍历可匹配候选, 但优先选择当前实参直接命中具体参数类型的 overload.
        if selected
            .as_ref()
            .is_none_or(|selected: &CallConstraintCandidate| {
                generic_arg_count < selected.generic_arg_count
            })
        {
            selected = Some(CallConstraintCandidate {
                doc_func: func,
                substitutor,
                generic_arg_count,
            });
        }
    }

    selected
}

fn generic_arg_count(
    func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
    call_arg_types: &[LuaType],
) -> usize {
    call_arg_types
        .iter()
        .enumerate()
        .filter(|(arg_index, _)| {
            let mut param_index = *arg_index;
            match (func.is_colon_define(), call_expr.is_colon_call()) {
                (true, false) => {
                    if param_index == 0 {
                        return false;
                    }
                    param_index -= 1;
                }
                (false, true) => param_index += 1,
                _ => {}
            }

            let param_type = func
                .get_params()
                .get(param_index)
                .or_else(|| {
                    func.get_params()
                        .last()
                        .filter(|last_param| last_param.0 == "...")
                })
                .and_then(|(_, param_type)| param_type.as_ref());
            param_type.is_some_and(|param_type| {
                param_type.any_type(|ty| match ty {
                    LuaType::TplRef(tpl) => tpl.get_tpl_id().is_func(),
                    LuaType::StrTplRef(tpl) => tpl.get_tpl_id().is_func(),
                    _ => false,
                })
            })
        })
        .count()
}

// 将推导结果转换为更易比较的形式
pub fn normalize_constraint_type(db: &DbIndex, ty: LuaType) -> LuaType {
    match ty {
        LuaType::Tuple(tuple) if tuple.is_infer_resolve() => tuple.collapse_to_union(db),
        LuaType::Call(alias_call)
            if alias_call.get_call_kind() == LuaAliasCallKind::KeyOf
                && !LuaType::Call(alias_call.clone()).contains_tpl_node() =>
        {
            let call_type = LuaType::Call(alias_call);
            normalize_constraint_type(
                db,
                instantiate_type_generic(db, &call_type, &TypeSubstitutor::new()),
            )
        }
        _ => ty,
    }
}

// 推导每个实参类型
fn get_arg_infos(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Vec<CallConstraintArg>> {
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let arg_infos = semantic_model
        .infer_expr_list_types(&arg_exprs, None)
        .into_iter()
        .map(|(raw_type, range)| CallConstraintArg { raw_type, range })
        .collect();

    Some(arg_infos)
}
