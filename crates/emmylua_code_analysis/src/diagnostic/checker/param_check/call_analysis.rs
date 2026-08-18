use std::sync::Arc;

use emmylua_parser::{LuaCallExpr, LuaExpr};

use crate::{
    LuaFunctionType, SemanticModel, infer_call_generic, semantic::collect_callable_overload_groups,
};

pub(super) struct CallAnalysis {
    pub(super) call_expr: LuaCallExpr,
    pub(super) arg_exprs: Vec<LuaExpr>,
    candidates: Vec<CallCandidate>,
}

pub(super) struct CallCandidate {
    pub(super) instantiated: Arc<LuaFunctionType>,
    pub(super) original: Arc<LuaFunctionType>,
}

impl CallAnalysis {
    pub(super) fn analyze(semantic_model: &SemanticModel, call_expr: LuaCallExpr) -> Option<Self> {
        let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
        let candidates = collect_call_candidates(semantic_model, &call_expr)?;

        Some(Self {
            call_expr,
            arg_exprs,
            candidates,
        })
    }

    pub(super) fn candidates(&self) -> &[CallCandidate] {
        &self.candidates
    }
}

// 收集所有可调用候选, 并保留泛型实例化前后的签名.
fn collect_call_candidates(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Vec<CallCandidate>> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let prefix_type = semantic_model.infer_expr(prefix_expr).ok()?;
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(semantic_model.get_db(), &prefix_type, &mut overload_groups)
        .ok()?;
    let mut candidates = Vec::new();
    for original in overload_groups.into_iter().flatten() {
        let instantiated = if original.contain_tpl() {
            infer_call_generic(
                semantic_model.get_db(),
                &mut semantic_model.get_cache().borrow_mut(),
                original.as_ref(),
                call_expr.clone(),
            )
            .map(Arc::new)
            .unwrap_or_else(|_| original.clone())
        } else {
            original.clone()
        };
        candidates.push(CallCandidate {
            instantiated,
            original,
        });
    }

    (!candidates.is_empty()).then_some(candidates)
}
