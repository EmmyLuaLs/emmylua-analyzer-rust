use std::{
    cell::{OnceCell, RefCell},
    collections::HashSet,
    sync::Arc,
};

use emmylua_parser::{LuaCallExpr, LuaExpr, LuaLiteralToken};
use rowan::TextRange;

use crate::{
    DbIndex, LuaFunctionType, LuaType, SemanticModel, infer_call_generic,
    semantic::{collect_callable_overload_groups, is_func_last_param_variadic},
};

pub(super) struct CallFacts {
    pub(super) call_expr: LuaCallExpr,
    pub(super) arg_exprs: Vec<LuaExpr>,
    funcs: Vec<Arc<LuaFunctionType>>,
    arg_types_and_ranges: RefCell<Option<Vec<(LuaType, TextRange)>>>,
    base_call_arg_count_range: OnceCell<Option<CountRange>>,
}

impl CallFacts {
    pub(super) fn new(semantic_model: &SemanticModel, call_expr: LuaCallExpr) -> Option<Self> {
        let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
        let funcs = collect_diagnostic_callables(semantic_model, &call_expr)?;

        Some(Self {
            call_expr,
            arg_exprs,
            funcs,
            arg_types_and_ranges: RefCell::new(None),
            base_call_arg_count_range: OnceCell::new(),
        })
    }

    pub(super) fn funcs(&self) -> &[Arc<LuaFunctionType>] {
        &self.funcs
    }

    pub(super) fn arg_types_and_ranges(
        &self,
        semantic_model: &SemanticModel,
    ) -> (Vec<LuaType>, Vec<TextRange>) {
        self.inferred_arg_types_and_ranges(semantic_model)
            .into_iter()
            .unzip()
    }

    pub(super) fn arg_types(&self, semantic_model: &SemanticModel) -> Vec<LuaType> {
        self.inferred_arg_types_and_ranges(semantic_model)
            .into_iter()
            .map(|(typ, _)| typ)
            .collect()
    }

    pub(super) fn param_count_compatible_funcs(
        &self,
        semantic_model: &SemanticModel,
    ) -> Vec<Arc<LuaFunctionType>> {
        self.funcs
            .iter()
            .filter(|func| self.callable_accepts_arg_count(semantic_model, func))
            .cloned()
            .collect()
    }

    pub(super) fn callable_accepts_arg_count(
        &self,
        semantic_model: &SemanticModel,
        func: &LuaFunctionType,
    ) -> bool {
        let Some(call_count) = self.call_arg_count_range(semantic_model, func) else {
            // 如果调用数量无法确定, 保守保留候选, 避免误报 ParamTypeMismatch.
            return true;
        };
        let param_count = get_param_count_range(semantic_model.get_db(), &self.call_expr, func);

        count_ranges_overlap(call_count, param_count)
    }

    pub(super) fn call_arg_count_range(
        &self,
        semantic_model: &SemanticModel,
        func: &LuaFunctionType,
    ) -> Option<CountRange> {
        let mut count = self.base_call_arg_count_range(semantic_model)?;
        if self.call_expr.is_colon_call() && !func.is_colon_define() {
            count.min += 1;
            count.max = count.max.map(|max| max + 1);
        }

        Some(count)
    }

    fn inferred_arg_types_and_ranges(
        &self,
        semantic_model: &SemanticModel,
    ) -> Vec<(LuaType, TextRange)> {
        let mut cached = self.arg_types_and_ranges.borrow_mut();
        if cached.is_none() {
            *cached = Some(semantic_model.infer_expr_list_types(&self.arg_exprs, None));
        }

        cached.as_ref().cloned().unwrap_or_default()
    }

    fn base_call_arg_count_range(&self, semantic_model: &SemanticModel) -> Option<CountRange> {
        *self
            .base_call_arg_count_range
            .get_or_init(|| get_base_call_arg_count_range(semantic_model, &self.arg_exprs))
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

pub(super) fn count_ranges_overlap(call_count: CountRange, param_count: CountRange) -> bool {
    // 调用方最多能提供的参数数量, 必须覆盖被调方至少需要的参数数量.
    let enough_args = call_count.max.is_none_or(|max| max >= param_count.min);
    // 调用方至少会提供的参数数量, 不能超过被调方最多能接收的参数数量.
    let not_too_many_args = param_count.max.is_none_or(|max| call_count.min <= max);

    enough_args && not_too_many_args
}

#[derive(Clone, Copy)]
pub(super) struct CountRange {
    pub(super) min: usize,
    pub(super) max: Option<usize>,
}

fn get_base_call_arg_count_range(
    semantic_model: &SemanticModel,
    arg_exprs: &[LuaExpr],
) -> Option<CountRange> {
    if arg_exprs.iter().any(is_dots_expr) {
        // `...` 无法精确给出数量范围, 交给后续类型检查处理.
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

pub(super) fn get_param_count_range(
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
) -> CountRange {
    let params = func.get_params();
    let self_offset = usize::from(needs_implicit_self_param(call_expr, func));

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

pub(super) fn adjusted_params(
    call_expr: &LuaCallExpr,
    func: &LuaFunctionType,
) -> Vec<(String, Option<LuaType>)> {
    let mut params = func.get_params().to_vec();
    if needs_implicit_self_param(call_expr, func) {
        params.insert(0, ("self".to_string(), Some(LuaType::SelfInfer)));
    }

    params
}

fn needs_implicit_self_param(call_expr: &LuaCallExpr, func: &LuaFunctionType) -> bool {
    !call_expr.is_colon_call() && func.is_colon_define()
}

pub(super) fn is_dots_expr(expr: &LuaExpr) -> bool {
    if let LuaExpr::LiteralExpr(literal_expr) = expr
        && let Some(LuaLiteralToken::Dots(_)) = literal_expr.get_literal()
    {
        return true;
    }
    false
}

pub(super) fn is_nullable(db: &DbIndex, typ: &LuaType) -> bool {
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
