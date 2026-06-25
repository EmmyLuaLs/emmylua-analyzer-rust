use std::sync::Arc;

use emmylua_parser::{LuaCallExpr, LuaExpr};

use crate::{
    LuaFunctionType, SemanticModel, infer_call_generic, semantic::collect_callable_overload_groups,
};

pub(super) struct CallFacts {
    pub(super) call_expr: LuaCallExpr,
    pub(super) arg_exprs: Vec<LuaExpr>,
    funcs: Vec<Arc<LuaFunctionType>>,
}

impl CallFacts {
    pub(super) fn new(semantic_model: &SemanticModel, call_expr: LuaCallExpr) -> Option<Self> {
        let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
        let funcs = collect_diagnostic_callables(semantic_model, &call_expr)?;

        Some(Self {
            call_expr,
            arg_exprs,
            funcs,
        })
    }

    pub(super) fn funcs(&self) -> &[Arc<LuaFunctionType>] {
        &self.funcs
    }
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
    let mut funcs = Vec::new();
    for func in overload_groups.into_iter().flatten() {
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
