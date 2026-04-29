use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaIndexExpr};

use crate::{
    DbIndex, DocTypeInferContext, GenericTplId, LuaFunctionType, LuaSemanticDeclId, LuaType,
    SemanticDeclLevel, SemanticModel, TypeOps, TypeSubstitutor, VariadicType, infer_doc_type,
};

// 泛型约束上下文
pub struct CallConstraintContext {
    pub params: Vec<(String, Option<LuaType>)>,
    pub arg_infos: Vec<LuaType>,
    pub substitutor: TypeSubstitutor,
}

pub fn build_call_constraint_context(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<(CallConstraintContext, Arc<LuaFunctionType>)> {
    let doc_func = infer_call_doc_function(semantic_model, call_expr)?;
    let mut params = doc_func.get_params().to_vec();
    let mut arg_infos = get_arg_infos(semantic_model, call_expr)?;
    let mut substitutor = TypeSubstitutor::new();

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
            arg_infos.insert(0, infer_call_source_type(semantic_model, call_expr)?);
        }
    }

    collect_generic_assignments(&mut substitutor, &params, &arg_infos);

    Some((
        CallConstraintContext {
            params,
            arg_infos,
            substitutor,
        },
        doc_func,
    ))
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
    arg_infos: &[LuaType],
) {
    for (idx, (_, param_type)) in params.iter().enumerate() {
        let Some(param_type) = param_type else {
            continue;
        };
        let Some(arg_type) = arg_infos.get(idx) else {
            continue;
        };
        record_generic_assignment(param_type, arg_type, substitutor);
    }
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
fn get_arg_infos(semantic_model: &SemanticModel, call_expr: &LuaCallExpr) -> Option<Vec<LuaType>> {
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let mut arg_infos = infer_expr_list_types(semantic_model, &arg_exprs);
    for (arg_type, _) in arg_infos.iter_mut() {
        let extend_type = get_constraint_type(semantic_model, arg_type, 0);
        if let Some(extend_type) = extend_type {
            *arg_type = extend_type;
        }
    }

    let arg_infos = arg_infos
        .into_iter()
        .map(|(arg_type, _)| arg_type)
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
