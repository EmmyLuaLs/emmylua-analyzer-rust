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
        let mut cached = self.arg_types_and_ranges.borrow_mut();
        if cached.is_none() {
            *cached = Some(semantic_model.infer_expr_list_types(&self.arg_exprs, None));
        }

        cached
            .as_ref()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .unzip()
    }

    // 计算当前调用表达式的实参列表能提供多少个实参槽位.
    pub(super) fn call_arg_count_range(
        &self,
        semantic_model: &SemanticModel,
        func: &LuaFunctionType,
    ) -> Option<CountRange> {
        let mut count = self
            .base_call_arg_count_range
            .get_or_init(|| get_base_call_arg_count_range(semantic_model, &self.arg_exprs))
            .as_ref()
            .copied()?;
        if self.call_expr.is_colon_call() && !func.is_colon_define() {
            // 冒号调用普通函数时, `obj:foo(x)` 等价于 `obj.foo(obj, x)`.
            // 这里要把 receiver 计入调用侧的实参槽位.
            count.min += 1;
            count.max = count.max.map(|max| max + 1);
        }

        Some(count)
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

// 比较调用侧能提供的实参数量, 和函数侧能接受的形参数量是否有交集.
pub(super) fn count_ranges_overlap(call_count: CountRange, param_count: CountRange) -> bool {
    let enough_args = call_count.max.is_none_or(|max| max >= param_count.min);
    let not_too_many_args = param_count.max.is_none_or(|max| call_count.min <= max);

    enough_args && not_too_many_args
}

#[derive(Clone, Copy)]
pub(super) struct CountRange {
    // 数量下界: 调用侧至少会提供多少, 或函数侧至少要求多少.
    pub(super) min: usize,
    // 数量上界: 调用侧最多会提供多少, 或函数侧最多接受多少; None 表示无上限.
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

// 计算当前候选函数签名能接受多少个形参槽位.
pub(super) fn get_param_count_range(
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
