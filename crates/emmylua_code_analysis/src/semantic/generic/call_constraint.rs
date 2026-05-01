use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaIndexExpr};
use hashbrown::HashSet;
use rowan::TextRange;

use crate::{
    DbIndex, DocTypeInferContext, GenericTpl, GenericTplId, LuaFunctionType, LuaSemanticDeclId,
    LuaType, LuaTypeNode, SemanticDeclLevel, SemanticModel, TypeOps, TypeSubstitutor, VariadicType,
    infer_doc_type,
};

// 泛型约束上下文
pub struct CallConstraintContext {
    pub params: Vec<(String, Option<LuaType>)>,
    pub args: Vec<CallConstraintArg>,
    pub substitutor: TypeSubstitutor,
}

pub struct CallConstraintArg {
    pub raw_type: LuaType,
    pub check_type: LuaType,
    pub range: TextRange,
}

pub fn build_call_constraint_context(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<CallConstraintContext> {
    let doc_func = infer_call_doc_function(semantic_model, call_expr)?;
    let mut params = doc_func.get_params().to_vec();
    let mut args = get_arg_infos(semantic_model, call_expr)?;
    let mut substitutor = TypeSubstitutor::new();
    let generic_tpls = collect_func_tpl_ids(&params);
    if !generic_tpls.is_empty() {
        substitutor.add_need_infer_tpls(generic_tpls);
    }

    // 读取显式传入的泛型实参
    if let Some(type_list) = call_expr.get_call_generic_type_list() {
        let doc_ctx =
            DocTypeInferContext::new(semantic_model.get_db(), semantic_model.get_file_id());
        for (idx, doc_type) in type_list.get_types().enumerate() {
            let ty = infer_doc_type(doc_ctx, &doc_type);
            substitutor.insert_type(GenericTplId::Func(idx as u32), ty, true);
        }
    }

    // 处理冒号调用与函数定义在 self 参数上的差异
    match (call_expr.is_colon_call(), doc_func.is_colon_define()) {
        (true, true) | (false, false) => {}
        (false, true) => {
            params.insert(0, ("self".into(), Some(LuaType::SelfInfer)));
        }
        (true, false) => {
            let source_type = infer_call_source_type(semantic_model, call_expr)?;
            args.insert(
                0,
                CallConstraintArg {
                    raw_type: source_type.clone(),
                    check_type: source_type,
                    range: call_expr.get_colon_token()?.get_range(),
                },
            );
        }
    }

    collect_generic_assignments(&mut substitutor, &params, &args);

    Some(CallConstraintContext {
        params,
        args,
        substitutor,
    })
}

// 将推导结果转换为更易比较的形式
pub fn normalize_constraint_type(db: &DbIndex, ty: LuaType) -> LuaType {
    match ty {
        LuaType::Tuple(tuple) if tuple.is_infer_resolve() => tuple.cast_down_array_base(db),
        _ => ty,
    }
}

// 收集各个参数对应的泛型推导
fn collect_generic_assignments(
    substitutor: &mut TypeSubstitutor,
    params: &[(String, Option<LuaType>)],
    args: &[CallConstraintArg],
) {
    for (idx, (_, param_type)) in params.iter().enumerate() {
        let Some(param_type) = param_type else {
            continue;
        };
        let Some(arg) = args.get(idx) else {
            continue;
        };
        record_generic_assignment(param_type, &arg.check_type, substitutor);
    }
}

fn collect_func_tpl_ids(params: &[(String, Option<LuaType>)]) -> HashSet<GenericTplId> {
    let mut generic_tpls = HashSet::new();
    for (_, param_type) in params {
        let Some(param_type) = param_type else {
            continue;
        };
        collect_func_tpls_from_param_type(param_type, &mut generic_tpls);
    }

    generic_tpls
}

fn collect_func_tpls_from_param_type(ty: &LuaType, generic_tpls: &mut HashSet<GenericTplId>) {
    collect_func_tpl_from_param_node(ty, generic_tpls);
    ty.visit_nested_types(&mut |ty| {
        collect_func_tpl_from_param_node(ty, generic_tpls);
    });
}

fn collect_func_tpl_from_param_node(ty: &LuaType, generic_tpls: &mut HashSet<GenericTplId>) {
    match ty {
        LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
            collect_func_tpl_with_fallback_deps(generic_tpl, generic_tpls);
        }
        LuaType::StrTplRef(str_tpl) => {
            let tpl_id = str_tpl.get_tpl_id();
            if tpl_id.is_func() {
                generic_tpls.insert(tpl_id);
                if let Some(constraint) = str_tpl.get_constraint() {
                    let mut constraint_deps = HashSet::new();
                    if collect_func_tpl_deps_from_fallback_type(
                        constraint,
                        &mut constraint_deps,
                        &mut HashSet::new(),
                    ) {
                        generic_tpls.extend(constraint_deps);
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_func_tpl_with_fallback_deps(
    generic_tpl: &GenericTpl,
    generic_tpls: &mut HashSet<GenericTplId>,
) {
    let tpl_id = generic_tpl.get_tpl_id();
    if !tpl_id.is_func() {
        return;
    }

    generic_tpls.insert(tpl_id);

    let Some(fallback_type) = generic_tpl
        .get_default_type()
        .or(generic_tpl.get_constraint())
    else {
        return;
    };

    let mut fallback_deps = HashSet::new();
    let mut visiting_fallbacks = HashSet::new();
    visiting_fallbacks.insert(tpl_id);
    if collect_func_tpl_deps_from_fallback_type(
        fallback_type,
        &mut fallback_deps,
        &mut visiting_fallbacks,
    ) {
        generic_tpls.extend(fallback_deps);
    }
}

fn collect_func_tpl_deps_from_fallback_type(
    ty: &LuaType,
    generic_tpls: &mut HashSet<GenericTplId>,
    visiting_fallbacks: &mut HashSet<GenericTplId>,
) -> bool {
    let mut no_fallback_cycle =
        collect_func_tpl_dep_from_fallback_type(ty, generic_tpls, visiting_fallbacks);
    ty.visit_nested_types(&mut |ty| {
        no_fallback_cycle &=
            collect_func_tpl_dep_from_fallback_type(ty, generic_tpls, visiting_fallbacks);
    });
    no_fallback_cycle
}

fn collect_func_tpl_dep_from_fallback_type(
    ty: &LuaType,
    generic_tpls: &mut HashSet<GenericTplId>,
    visiting_fallbacks: &mut HashSet<GenericTplId>,
) -> bool {
    match ty {
        LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
            collect_generic_tpl_from_fallback(generic_tpl, generic_tpls, visiting_fallbacks)
        }
        LuaType::StrTplRef(str_tpl) => {
            let tpl_id = str_tpl.get_tpl_id();
            if !tpl_id.is_func() {
                return true;
            }

            if !visiting_fallbacks.insert(tpl_id) {
                return false;
            }

            generic_tpls.insert(tpl_id);
            let no_fallback_cycle = match str_tpl.get_constraint() {
                Some(constraint) => collect_func_tpl_deps_from_fallback_type(
                    constraint,
                    generic_tpls,
                    visiting_fallbacks,
                ),
                None => true,
            };
            visiting_fallbacks.remove(&tpl_id);
            no_fallback_cycle
        }
        _ => true,
    }
}

fn collect_generic_tpl_from_fallback(
    generic_tpl: &GenericTpl,
    generic_tpls: &mut HashSet<GenericTplId>,
    visiting_fallbacks: &mut HashSet<GenericTplId>,
) -> bool {
    let tpl_id = generic_tpl.get_tpl_id();
    if !tpl_id.is_func() {
        return true;
    }

    if !visiting_fallbacks.insert(tpl_id) {
        return false;
    }

    generic_tpls.insert(tpl_id);
    let no_fallback_cycle = match generic_tpl
        .get_default_type()
        .or(generic_tpl.get_constraint())
    {
        Some(fallback_type) => collect_func_tpl_deps_from_fallback_type(
            fallback_type,
            generic_tpls,
            visiting_fallbacks,
        ),
        None => true,
    };
    visiting_fallbacks.remove(&tpl_id);
    no_fallback_cycle
}

// 实际写入泛型替换表
fn record_generic_assignment(
    param_type: &LuaType,
    arg_type: &LuaType,
    substitutor: &mut TypeSubstitutor,
) {
    match param_type {
        LuaType::TplRef(tpl_ref) => {
            if !tpl_ref.get_tpl_id().is_conditional_infer() {
                substitutor.insert_type(tpl_ref.get_tpl_id(), arg_type.clone(), true);
            }
        }
        LuaType::ConstTplRef(tpl_ref) => {
            if !tpl_ref.get_tpl_id().is_conditional_infer() {
                substitutor.insert_type(tpl_ref.get_tpl_id(), arg_type.clone(), false);
            }
        }
        LuaType::StrTplRef(str_tpl_ref) => {
            substitutor.insert_type(str_tpl_ref.get_tpl_id(), arg_type.clone(), true);
        }
        LuaType::Variadic(variadic) => {
            if let Some(inner) = variadic.get_type(0) {
                record_generic_assignment(inner, arg_type, substitutor);
            }
        }
        _ => {}
    }
}

// 解析冒号调用时调用者的具体类型
fn infer_call_source_type(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<LuaType> {
    match call_expr.get_prefix_expr()? {
        LuaExpr::IndexExpr(index_expr) => {
            let decl = semantic_model.find_decl(
                index_expr.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )?;

            if let LuaSemanticDeclId::Member(member_id) = decl
                && let Some(LuaSemanticDeclId::Member(member_id)) =
                    semantic_model.get_member_origin_owner(member_id)
            {
                let root = semantic_model
                    .get_db()
                    .get_vfs()
                    .get_syntax_tree(&member_id.file_id)?
                    .get_red_root();
                let cur_node = member_id.get_syntax_id().to_node_from_root(&root)?;
                let index_expr = LuaIndexExpr::cast(cur_node)?;

                return index_expr.get_prefix_expr().map(|prefix_expr| {
                    semantic_model
                        .infer_expr(prefix_expr.clone())
                        .unwrap_or(LuaType::SelfInfer)
                });
            }

            return if let Some(prefix_expr) = index_expr.get_prefix_expr() {
                let expr_type = semantic_model
                    .infer_expr(prefix_expr.clone())
                    .unwrap_or(LuaType::SelfInfer);
                Some(expr_type)
            } else {
                None
            };
        }
        LuaExpr::NameExpr(name_expr) => {
            let decl = semantic_model.find_decl(
                name_expr.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )?;
            if let LuaSemanticDeclId::Member(member_id) = decl {
                let root = semantic_model
                    .get_db()
                    .get_vfs()
                    .get_syntax_tree(&member_id.file_id)?
                    .get_red_root();
                let cur_node = member_id.get_syntax_id().to_node_from_root(&root)?;
                let index_expr = LuaIndexExpr::cast(cur_node)?;

                return index_expr.get_prefix_expr().map(|prefix_expr| {
                    semantic_model
                        .infer_expr(prefix_expr.clone())
                        .unwrap_or(LuaType::SelfInfer)
                });
            }

            return None;
        }
        _ => {}
    }

    None
}

// 推导每个实参类型
fn get_arg_infos(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Vec<CallConstraintArg>> {
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let arg_infos = infer_expr_list_types(semantic_model, &arg_exprs)
        .into_iter()
        .map(|(raw_type, expr)| {
            let check_type = get_constraint_type(semantic_model, &raw_type, 0)
                .unwrap_or_else(|| raw_type.clone());
            CallConstraintArg {
                raw_type,
                check_type,
                range: expr.get_range(),
            }
        })
        .collect();

    Some(arg_infos)
}

fn infer_call_doc_function(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<Arc<LuaFunctionType>> {
    let prefix_expr = call_expr.get_prefix_expr()?.clone();
    let function = semantic_model.infer_expr(prefix_expr).ok()?;
    match function {
        LuaType::Signature(signature_id) => {
            let signature = semantic_model
                .get_db()
                .get_signature_index()
                .get(&signature_id)?;
            if !signature.overloads.is_empty() {
                // When a signature has overloads, `to_doc_func_type()` merges all overload
                // parameter types into unions on the main signature. This produces incorrect
                // types for generic constraint checking (e.g. a merged `T | nil | integer`
                // would falsely trigger a constraint mismatch).
                // Instead, resolve the actual overload that matches the call arguments,
                // so that constraint checking runs against the correct parameter types.
                return semantic_model.infer_call_expr_func(call_expr.clone(), None);
            }
            Some(signature.to_doc_func_type())
        }
        LuaType::DocFunction(func) => Some(func),
        _ => None,
    }
}

// 获取约束类型
fn get_constraint_type(
    semantic_model: &SemanticModel,
    arg_type: &LuaType,
    depth: usize,
) -> Option<LuaType> {
    match arg_type {
        LuaType::TplRef(tpl_ref) | LuaType::ConstTplRef(tpl_ref) => {
            tpl_ref.get_constraint().cloned()
        }
        LuaType::StrTplRef(str_tpl_ref) => str_tpl_ref.get_constraint().cloned(),
        LuaType::Union(union_type) => {
            if depth > 1 {
                return None;
            }
            let mut result = LuaType::Never;
            for union_member_type in union_type.into_vec().iter() {
                let extend_type = get_constraint_type(semantic_model, union_member_type, depth + 1)
                    .unwrap_or(union_member_type.clone());
                result = TypeOps::Union.apply(semantic_model.get_db(), &result, &extend_type);
            }
            Some(result)
        }
        _ => None,
    }
}

// 将多个表达式推导为具体类型列表
fn infer_expr_list_types(
    semantic_model: &SemanticModel,
    exprs: &[LuaExpr],
) -> Vec<(LuaType, LuaExpr)> {
    let mut value_types = Vec::new();
    for expr in exprs.iter() {
        let expr_type = semantic_model
            .infer_expr(expr.clone())
            .unwrap_or(LuaType::Unknown);
        match expr_type {
            LuaType::Variadic(variadic) => match variadic.deref() {
                VariadicType::Base(base) => {
                    value_types.push((base.clone(), expr.clone()));
                }
                VariadicType::Multi(vecs) => {
                    for typ in vecs {
                        value_types.push((typ.clone(), expr.clone()));
                    }
                }
            },
            _ => value_types.push((expr_type.clone(), expr.clone())),
        }
    }
    value_types
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hashbrown::HashSet;
    use smol_str::SmolStr;

    use super::*;

    fn func_tpl(idx: u32, default_type: Option<LuaType>) -> Arc<GenericTpl> {
        Arc::new(GenericTpl::new(
            GenericTplId::Func(idx),
            SmolStr::new(format!("T{}", idx)).into(),
            None,
            default_type,
        ))
    }

    #[test]
    fn test_collect_func_tpl_with_fallback_deps_skips_cyclic_fallback_deps() {
        let t0 = func_tpl(0, None);
        let t1 = func_tpl(1, Some(LuaType::TplRef(t0.clone())));
        let t0 = GenericTpl::new(
            GenericTplId::Func(0),
            SmolStr::new("T0").into(),
            None,
            Some(LuaType::TplRef(t1)),
        );

        let mut generic_tpls = HashSet::new();
        collect_func_tpl_with_fallback_deps(&t0, &mut generic_tpls);

        assert!(generic_tpls.contains(&GenericTplId::Func(0)));
        assert!(!generic_tpls.contains(&GenericTplId::Func(1)));
    }
}
