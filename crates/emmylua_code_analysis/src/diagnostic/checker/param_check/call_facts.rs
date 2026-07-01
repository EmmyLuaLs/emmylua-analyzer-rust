use std::sync::Arc;

use emmylua_parser::{LuaCallExpr, LuaExpr};

use crate::{
    LuaFunctionType, SemanticModel, infer_call_generic, semantic::collect_callable_overload_groups,
};

pub(super) struct CallFacts {
    pub(super) call_expr: LuaCallExpr,
    pub(super) arg_exprs: Vec<LuaExpr>,
    callables: Vec<DiagnosticCallable>,
}

pub(super) struct DiagnosticCallable {
    pub(super) func: Arc<LuaFunctionType>,
    pub(super) origin_func: Arc<LuaFunctionType>,
}

impl CallFacts {
    pub(super) fn new(semantic_model: &SemanticModel, call_expr: LuaCallExpr) -> Option<Self> {
        let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
        let callables = collect_diagnostic_callables(semantic_model, &call_expr)?;

        Some(Self {
            call_expr,
            arg_exprs,
            callables,
        })
    }

    pub(super) fn callables(&self) -> &[DiagnosticCallable] {
        &self.callables
    }
}

// 收集所有可调用的候选.
fn collect_diagnostic_callables(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Vec<DiagnosticCallable>> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let prefix_type = semantic_model.infer_expr(prefix_expr).ok()?;
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(semantic_model.get_db(), &prefix_type, &mut overload_groups)
        .ok()?;
    let mut callables = Vec::new();
    for func in overload_groups.into_iter().flatten() {
        let origin_func = func.clone();
        let func = if origin_func.contain_tpl() {
            infer_call_generic(
                semantic_model.get_db(),
                &mut semantic_model.get_cache().borrow_mut(),
                origin_func.as_ref(),
                call_expr.clone(),
            )
            .map(Arc::new)
            .unwrap_or_else(|_| origin_func.clone())
        } else {
            origin_func.clone()
        };
        callables.push(DiagnosticCallable { func, origin_func });
    }

    (!callables.is_empty()).then_some(callables)
}
