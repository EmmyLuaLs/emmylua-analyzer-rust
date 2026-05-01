use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaSyntaxKind};
use hashbrown::HashSet;
use rowan::TextRange;

use super::{
    super::{InferGuard, LuaInferCache, instantiate_type_generic, resolve_signature},
    InferFailReason, InferResult,
};
use crate::semantic::overload_resolve::callable_accepts_args;
use crate::{
    CacheEntry, DbIndex, InFiled, LuaFunctionType, LuaGenericType, LuaInstanceType,
    LuaIntersectionType, LuaOperatorMetaMethod, LuaOperatorOwner, LuaSignature, LuaSignatureId,
    LuaType, LuaTypeDeclId, LuaUnionType, TypeVisitTrait, VariadicType,
};
use crate::{
    InferGuardRef,
    semantic::{
        generic::{
            TypeSubstitutor, collect_callable_overload_groups, get_tpl_ref_extend_type,
            instantiate_doc_function,
        },
        infer::narrow::get_type_at_call_expr_inline_cast,
    },
};
use crate::{build_self_type, infer_self_type, instantiate_func_generic, semantic::infer_expr};
use infer_require::infer_require_call;
use infer_setmetatable::infer_setmetatable_call;

mod infer_require;
mod infer_setmetatable;

pub type InferCallFuncResult = Result<Arc<LuaFunctionType>, InferFailReason>;

pub fn infer_call_expr_func(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
    call_expr_type: LuaType,
    infer_guard: &InferGuardRef,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let syntax_id = call_expr.get_syntax_id();
    let key = (syntax_id, args_count, call_expr_type.clone());
    if let Some(cache) = cache.call_cache.get(&key) {
        match cache {
            CacheEntry::Cache(ty) => return Ok(ty.clone()),
            _ => return Err(InferFailReason::RecursiveInfer),
        }
    }

    cache.call_cache.insert(key.clone(), CacheEntry::Ready);
    let result = match &call_expr_type {
        LuaType::DocFunction(func) => {
            infer_doc_function(db, cache, func, call_expr.clone(), args_count)
        }
        LuaType::Signature(signature_id) => {
            infer_signature_doc_function(db, cache, *signature_id, call_expr.clone(), args_count)
        }
        LuaType::Def(type_def_id) => infer_type_doc_function(
            db,
            cache,
            type_def_id.clone(),
            call_expr.clone(),
            &call_expr_type,
            infer_guard,
            args_count,
        ),
        LuaType::Ref(type_ref_id) => infer_type_doc_function(
            db,
            cache,
            type_ref_id.clone(),
            call_expr.clone(),
            &call_expr_type,
            infer_guard,
            args_count,
        ),
        LuaType::Generic(generic) => infer_generic_type_doc_function(
            db,
            cache,
            generic,
            call_expr.clone(),
            infer_guard,
            args_count,
        ),
        LuaType::Instance(inst) => infer_instance_type_doc_function(db, inst),
        LuaType::TableConst(meta_table) => infer_table_type_doc_function(db, meta_table.clone()),
        LuaType::TplRef(_) | LuaType::ConstTplRef(_) | LuaType::StrTplRef(_) => infer_tpl_ref_call(
            db,
            cache,
            call_expr.clone(),
            &call_expr_type,
            infer_guard,
            args_count,
        ),
        LuaType::Function => Ok(Arc::new(LuaFunctionType::new(
            crate::AsyncState::None,
            false,
            true,
            vec![("...".to_string(), Some(LuaType::Unknown))],
            LuaType::Variadic(VariadicType::Base(LuaType::Unknown).into()),
        ))),
        LuaType::Intersection(intersection) => infer_intersection(
            db,
            cache,
            intersection,
            call_expr.clone(),
            infer_guard,
            args_count,
        ),
        LuaType::Union(union) => infer_union(db, cache, union, call_expr.clone(), args_count),
        _ => Err(InferFailReason::None),
    };

    let result = if let Ok(func_ty) = result {
        let func_ty = match func_ty.get_ret() {
            LuaType::Call(_) => {
                match instantiate_func_generic(db, cache, func_ty.as_ref(), call_expr.clone()) {
                    Ok(func_ty) => Arc::new(func_ty),
                    Err(_) => func_ty,
                }
            }
            _ => func_ty,
        };

        let func_ret = func_ty.get_ret();
        match func_ret {
            LuaType::TypeGuard(_) => Ok(func_ty),
            _ => unwrapp_return_type(db, cache, func_ret.clone(), call_expr).map(|new_ret| {
                LuaFunctionType::new(
                    func_ty.get_async_state(),
                    func_ty.is_colon_define(),
                    func_ty.is_variadic(),
                    func_ty.get_params().to_vec(),
                    new_ret,
                )
                .into()
            }),
        }
    } else {
        result
    };

    match &result {
        Ok(func_ty) => {
            cache
                .call_cache
                .insert(key, CacheEntry::Cache(func_ty.clone()));
        }
        Err(r) if r.is_need_resolve() => {
            cache.call_cache.remove(&key);
        }
        Err(InferFailReason::None) => {
            cache.call_cache.remove(&key);
        }
        _ => {}
    }

    result
}

fn infer_tpl_ref_call(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
    call_expr_type: &LuaType,
    infer_guard: &InferGuardRef,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let extend_type = get_tpl_ref_extend_type(db, cache, call_expr_type, prefix_expr, 0)
        .ok_or(InferFailReason::None)?;
    if &extend_type == call_expr_type {
        return Err(InferFailReason::None);
    }
    infer_call_expr_func(db, cache, call_expr, extend_type, infer_guard, args_count)
}

fn infer_doc_function(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    func: &LuaFunctionType,
    call_expr: LuaCallExpr,
    _: Option<usize>,
) -> InferCallFuncResult {
    if func.contain_tpl() {
        let result = instantiate_func_generic(db, cache, func, call_expr)?;
        return Ok(Arc::new(result));
    }

    Ok(func.clone().into())
}

fn filter_callable_overloads_by_call_args(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    overloads: Vec<Arc<LuaFunctionType>>,
    call_expr: &LuaCallExpr,
    args_count: Option<usize>,
    strict_arg_filter: bool,
) -> Result<Vec<Arc<LuaFunctionType>>, InferFailReason> {
    let args = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    let expr_types = super::infer_expr_list_types(
        db,
        cache,
        &args.get_args().collect::<Vec<_>>(),
        args_count,
        |db, cache, expr| Ok(infer_expr(db, cache, expr).unwrap_or(LuaType::Unknown)),
    )?
    .into_iter()
    .map(|(ty, _)| ty)
    .collect::<Vec<_>>();
    let is_colon_call = call_expr.is_colon_call();

    Ok(overloads
        .into_iter()
        .filter(|func| {
            let mut callable_tpls = HashSet::new();
            func.visit_type(&mut |ty| match ty {
                LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
                    callable_tpls.insert(generic_tpl.get_tpl_id());
                }
                LuaType::StrTplRef(str_tpl) => {
                    callable_tpls.insert(str_tpl.get_tpl_id());
                }
                _ => {}
            });

            if callable_tpls.is_empty() && !strict_arg_filter {
                return true;
            }

            let has_tpls = !callable_tpls.is_empty();
            let mut substitutor = TypeSubstitutor::new();
            substitutor.add_need_infer_tpls(callable_tpls);
            let match_func = if has_tpls {
                match instantiate_doc_function(db, func, &substitutor) {
                    LuaType::DocFunction(doc_func) => doc_func,
                    _ => func.clone(),
                }
            } else {
                func.clone()
            };

            callable_accepts_args(db, &match_func, &expr_types, is_colon_call, args_count)
        })
        .collect())
}

fn infer_signature_doc_function(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    signature_id: LuaSignatureId,
    call_expr: LuaCallExpr,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let signature = db
        .get_signature_index()
        .get(&signature_id)
        .ok_or(InferFailReason::None)?;
    if !signature.is_resolve_return() {
        return Err(InferFailReason::UnResolveSignatureReturn(signature_id));
    }
    let is_generic = signature_is_generic(db, cache, &signature, &call_expr).unwrap_or(false);
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(db, &LuaType::Signature(signature_id), &mut overload_groups)?;
    let overloads = overload_groups.into_iter().flatten().collect::<Vec<_>>();

    resolve_signature(db, cache, overloads, call_expr, is_generic, args_count)
}

fn infer_type_doc_function(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    type_id: LuaTypeDeclId,
    call_expr: LuaCallExpr,
    call_expr_type: &LuaType,
    infer_guard: &InferGuardRef,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    infer_guard.check(&type_id)?;
    let type_decl = db
        .get_type_index()
        .get_type_decl(&type_id)
        .ok_or_else(|| InferFailReason::UnResolveTypeDecl(type_id.clone()))?;
    if type_decl.is_alias() {
        let origin_type = type_decl
            .get_alias_origin(db, None)
            .ok_or(InferFailReason::None)?;
        return infer_call_expr_func(
            db,
            cache,
            call_expr,
            origin_type.clone(),
            infer_guard,
            args_count,
        );
    } else if type_decl.is_enum() {
        return Err(InferFailReason::None);
    }

    let operator_index = db.get_operator_index();
    let operator_ids = operator_index
        .get_operators(&type_id.clone().into(), LuaOperatorMetaMethod::Call)
        .ok_or(InferFailReason::UnResolveOperatorCall)?;
    let mut overloads = Vec::new();
    for overload_id in operator_ids {
        let operator = operator_index
            .get_operator(overload_id)
            .ok_or(InferFailReason::None)?;
        let func = operator.get_operator_func(db);
        match func {
            LuaType::DocFunction(f) => {
                let has_generic_tpl = {
                    let mut has_generic_tpl = false;
                    f.visit_type(&mut |t| {
                        has_generic_tpl |= matches!(
                            t,
                            LuaType::TplRef(_) | LuaType::ConstTplRef(_) | LuaType::StrTplRef(_)
                        );
                    });
                    has_generic_tpl
                };

                if has_generic_tpl {
                    let result = instantiate_func_generic(db, cache, &f, call_expr.clone())?;
                    overloads.push(Arc::new(result));
                } else if f.contain_self() {
                    let mut substitutor = TypeSubstitutor::new();
                    let self_type = build_self_type(db, call_expr_type);
                    substitutor.add_self_type(self_type);
                    if let LuaType::DocFunction(f) = instantiate_doc_function(db, &f, &substitutor)
                    {
                        overloads.push(f);
                    }
                } else {
                    overloads.push(f.clone());
                }
            }
            LuaType::Signature(signature_id) => {
                let signature = db
                    .get_signature_index()
                    .get(&signature_id)
                    .ok_or(InferFailReason::None)?;
                if !signature.is_resolve_return() {
                    return Err(InferFailReason::UnResolveSignatureReturn(signature_id));
                }

                overloads.push(signature.to_call_operator_func_type());
            }
            _ => {}
        }
    }

    resolve_signature(db, cache, overloads, call_expr.clone(), false, args_count)
}

fn infer_generic_type_doc_function(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    generic: &LuaGenericType,
    call_expr: LuaCallExpr,
    infer_guard: &InferGuardRef,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let type_id = generic.get_base_type_id();
    infer_guard.check(&type_id)?;
    let generic_params = generic.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());

    let type_decl = db
        .get_type_index()
        .get_type_decl(&type_id)
        .ok_or_else(|| InferFailReason::UnResolveTypeDecl(type_id.clone()))?;
    if type_decl.is_alias() {
        let origin_type = type_decl
            .get_alias_origin(db, Some(&substitutor))
            .ok_or(InferFailReason::None)?;
        return infer_call_expr_func(
            db,
            cache,
            call_expr,
            origin_type.clone(),
            infer_guard,
            args_count,
        );
    } else if type_decl.is_enum() {
        return Err(InferFailReason::None);
    }

    let operator_index = db.get_operator_index();
    let operator_ids = operator_index
        .get_operators(&type_id.into(), LuaOperatorMetaMethod::Call)
        .ok_or(InferFailReason::None)?;
    let mut overloads = Vec::new();
    for overload_id in operator_ids {
        let operator = operator_index
            .get_operator(overload_id)
            .ok_or(InferFailReason::None)?;
        let func = operator.get_operator_func(db);
        match func {
            LuaType::DocFunction(_) => {
                let new_f = instantiate_type_generic(db, &func, &substitutor);
                if let LuaType::DocFunction(f) = new_f {
                    overloads.push(f.clone());
                }
            }
            LuaType::Signature(signature_id) => {
                let signature = db
                    .get_signature_index()
                    .get(&signature_id)
                    .ok_or(InferFailReason::None)?;
                if !signature.is_resolve_return() {
                    return Err(InferFailReason::UnResolveSignatureReturn(signature_id));
                }

                let typ = LuaType::DocFunction(signature.to_call_operator_func_type());
                let new_f = instantiate_type_generic(db, &typ, &substitutor);
                if let LuaType::DocFunction(f) = new_f {
                    overloads.push(f.clone());
                }
                // todo: support overload?
            }
            _ => {}
        }
    }

    resolve_signature(db, cache, overloads, call_expr.clone(), false, args_count)
}

fn infer_instance_type_doc_function(
    db: &DbIndex,
    instance: &LuaInstanceType,
) -> InferCallFuncResult {
    let base = instance.get_base();
    let base_table = match &base {
        LuaType::TableConst(meta_table) => meta_table.clone(),
        LuaType::Instance(inst) => {
            return infer_instance_type_doc_function(db, inst);
        }
        _ => return Err(InferFailReason::None),
    };

    infer_table_type_doc_function(db, base_table)
}

fn infer_table_type_doc_function(db: &DbIndex, table: InFiled<TextRange>) -> InferCallFuncResult {
    let meta_table = db
        .get_metatable_index()
        .get(&table)
        .ok_or(InferFailReason::None)?;

    let meta_table_owner = LuaOperatorOwner::Table(meta_table.clone());

    let call_operators = db
        .get_operator_index()
        .get_operators(&meta_table_owner, LuaOperatorMetaMethod::Call)
        .ok_or(InferFailReason::None)?;

    // only first one is valid
    for operator_id in call_operators {
        let operator = db
            .get_operator_index()
            .get_operator(operator_id)
            .ok_or(InferFailReason::None)?;
        let func = operator.get_operator_func(db);
        match func {
            LuaType::DocFunction(func) => {
                return Ok(func);
            }
            LuaType::Signature(signature_id) => {
                let signature = db
                    .get_signature_index()
                    .get(&signature_id)
                    .ok_or(InferFailReason::None)?;
                if !signature.is_resolve_return() {
                    return Err(InferFailReason::UnResolveSignatureReturn(signature_id));
                }

                return Ok(signature.to_call_operator_func_type());
            }
            _ => {}
        }
    }

    Err(InferFailReason::None)
}

fn infer_union(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union: &LuaUnionType,
    call_expr: LuaCallExpr,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let mut returns = Vec::new();
    let mut first_func = None;
    let mut fallback_overloads = Vec::new();
    let mut need_resolve = None;

    for ty in union.into_vec() {
        let mut overload_groups = Vec::new();
        collect_callable_overload_groups(db, &ty, &mut overload_groups)?;
        for overloads in overload_groups {
            let compatible_overloads = filter_callable_overloads_by_call_args(
                db,
                cache,
                overloads.clone(),
                &call_expr,
                args_count,
                true,
            )?;
            if compatible_overloads.is_empty() {
                fallback_overloads.extend(overloads);
                continue;
            }

            let contains_tpl = compatible_overloads.iter().any(|func| func.contain_tpl());
            match resolve_signature(
                db,
                cache,
                compatible_overloads,
                call_expr.clone(),
                contains_tpl,
                args_count,
            ) {
                Ok(func) => {
                    returns.push(func.get_ret().clone());
                    if first_func.is_none() {
                        first_func = Some(func);
                    }
                }
                Err(InferFailReason::RecursiveInfer) => {
                    return Err(InferFailReason::RecursiveInfer);
                }
                Err(reason) if reason.is_need_resolve() => {
                    if need_resolve.is_none() {
                        need_resolve = Some(reason);
                    }
                }
                Err(_) => {}
            }
        }
    }

    let Some(first_func) = first_func else {
        if !fallback_overloads.is_empty() {
            let contains_tpl = fallback_overloads.iter().any(|func| func.contain_tpl());
            let fallback_overloads = filter_callable_overloads_by_call_args(
                db,
                cache,
                fallback_overloads,
                &call_expr,
                args_count,
                false,
            )?;
            return resolve_signature(
                db,
                cache,
                fallback_overloads,
                call_expr,
                contains_tpl,
                args_count,
            );
        }

        return Err(need_resolve.unwrap_or(InferFailReason::None));
    };

    Ok(Arc::new(LuaFunctionType::new(
        first_func.get_async_state(),
        first_func.is_colon_define(),
        first_func.is_variadic(),
        first_func.get_params().to_vec(),
        LuaType::from_vec(returns),
    )))
}

fn infer_intersection(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    intersection: &LuaIntersectionType,
    call_expr: LuaCallExpr,
    infer_guard: &InferGuardRef,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let mut overloads = Vec::new();
    let mut need_resolve = None;

    for ty in intersection.get_types() {
        match infer_call_expr_func(
            db,
            cache,
            call_expr.clone(),
            ty.clone(),
            infer_guard,
            args_count,
        ) {
            Ok(func) => overloads.push(func),
            Err(InferFailReason::RecursiveInfer) => return Err(InferFailReason::RecursiveInfer),
            Err(reason) if reason.is_need_resolve() => {
                if need_resolve.is_none() {
                    need_resolve = Some(reason);
                }
            }
            Err(_) => {}
        }
    }

    if overloads.is_empty() {
        return Err(need_resolve.unwrap_or(InferFailReason::None));
    }

    if overloads.len() == 1 {
        return Ok(overloads.pop().expect("single callable member"));
    }

    resolve_signature(db, cache, overloads, call_expr, false, args_count)
}

pub(crate) fn unwrapp_return_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    return_type: LuaType,
    call_expr: LuaCallExpr,
) -> InferResult {
    match &return_type {
        LuaType::Table => {
            let id = InFiled {
                file_id: cache.get_file_id(),
                value: call_expr.get_range(),
            };

            return Ok(LuaType::TableConst(id));
        }
        LuaType::TableConst(inst) => {
            if is_need_wrap_instance(cache, &call_expr, inst) {
                let id = InFiled {
                    file_id: cache.get_file_id(),
                    value: call_expr.get_range(),
                };

                return Ok(LuaType::Instance(
                    LuaInstanceType::new(return_type.clone(), id).into(),
                ));
            }

            return Ok(return_type);
        }
        LuaType::Instance(inst) => {
            if is_need_wrap_instance(cache, &call_expr, inst.get_range()) {
                let id = InFiled {
                    file_id: cache.get_file_id(),
                    value: call_expr.get_range(),
                };

                return Ok(LuaType::Instance(
                    LuaInstanceType::new(return_type.clone(), id).into(),
                ));
            }

            return Ok(return_type);
        }

        ty if ty.contain_multi_return() => {
            if is_last_call_expr(&call_expr) {
                return Ok(ty.clone());
            }

            return Ok(ty.get_result_slot_type(0).unwrap_or(LuaType::Nil));
        }
        LuaType::SelfInfer => {
            if let Some(self_type) = infer_self_type(db, cache, &call_expr) {
                return Ok(self_type);
            }
        }
        LuaType::TypeGuard(_) => return Ok(LuaType::Boolean),
        _ => {}
    }

    Ok(return_type)
}

fn is_need_wrap_instance(
    cache: &mut LuaInferCache,
    call_expr: &LuaCallExpr,
    inst: &InFiled<TextRange>,
) -> bool {
    if cache.get_file_id() != inst.file_id {
        return true;
    }

    !call_expr.get_range().contains(inst.value.start())
}

fn is_last_call_expr(call_expr: &LuaCallExpr) -> bool {
    let mut opt_parent = call_expr.syntax().parent();
    while let Some(parent) = &opt_parent {
        match parent.kind().into() {
            LuaSyntaxKind::AssignStat
            | LuaSyntaxKind::LocalStat
            | LuaSyntaxKind::ReturnStat
            | LuaSyntaxKind::TableArrayExpr
            | LuaSyntaxKind::CallArgList => {
                let next_expr = call_expr.syntax().next_sibling();
                return next_expr.is_none();
            }
            LuaSyntaxKind::TableFieldValue => {
                opt_parent = parent.parent();
            }
            LuaSyntaxKind::ForRangeStat => return true,
            _ => return false,
        }
    }

    false
}

pub fn infer_call_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
) -> InferResult {
    if call_expr.is_require() {
        return infer_require_call(db, cache, call_expr);
    } else if call_expr.is_setmetatable() {
        return infer_setmetatable_call(db, cache, call_expr);
    }

    check_can_infer(db, cache, &call_expr)?;

    let prefix_expr = call_expr.get_prefix_expr().ok_or(InferFailReason::None)?;
    let prefix_type = infer_expr(db, cache, prefix_expr)?;
    let ret_type = infer_call_expr_func(
        db,
        cache,
        call_expr.clone(),
        prefix_type,
        &InferGuard::new(),
        None,
    )?
    .get_ret()
    .clone();

    if let Some(tree) = db.get_flow_index().get_flow_tree(&cache.get_file_id())
        && let Some(flow_id) = tree.get_flow_id(call_expr.get_syntax_id())
        && let Some(flow_ret_type) =
            get_type_at_call_expr_inline_cast(db, cache, tree, call_expr, flow_id, ret_type.clone())
    {
        return Ok(flow_ret_type);
    }

    Ok(ret_type)
}

fn check_can_infer(
    db: &DbIndex,
    cache: &LuaInferCache,
    call_expr: &LuaCallExpr,
) -> Result<(), InferFailReason> {
    let call_args = call_expr
        .get_args_list()
        .ok_or(InferFailReason::None)?
        .get_args();
    for arg in call_args {
        if let LuaExpr::ClosureExpr(closure) = arg {
            let sig_id = LuaSignatureId::from_closure(cache.get_file_id(), &closure);
            let signature = db
                .get_signature_index()
                .get(&sig_id)
                .ok_or(InferFailReason::None)?;
            if !signature.is_resolve_return() {
                return Err(InferFailReason::UnResolveSignatureReturn(sig_id));
            }
        }
    }

    Ok(())
}

fn signature_is_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    signature: &LuaSignature,
    call_expr: &LuaCallExpr,
) -> Option<bool> {
    if signature.is_generic() {
        return Some(true);
    }
    let LuaExpr::IndexExpr(index_expr) = call_expr.get_prefix_expr()? else {
        return None;
    };
    let prefix_type = infer_expr(db, cache, index_expr.get_prefix_expr()?).ok()?;
    match prefix_type {
        // 对于 Generic 直接认为是泛型
        LuaType::Generic(_) => Some(true),
        _ => Some(prefix_type.contain_tpl()),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        InferFailReason, InferGuard, LuaType, VirtualWorkspace, semantic::infer_call_expr_func,
    };

    #[test]
    fn test_call_cache_non_callable_not_sticky() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def("local i = 1\n i()\n");
        let call_expr = ws.get_node::<emmylua_parser::LuaCallExpr>(file_id);
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        let db = semantic_model.get_db();
        let mut cache = semantic_model.get_cache().borrow_mut();
        let call_expr_type = LuaType::IntegerConst(1);

        let _ = infer_call_expr_func(
            db,
            &mut cache,
            call_expr.clone(),
            call_expr_type.clone(),
            &InferGuard::new(),
            None,
        );
        let second = infer_call_expr_func(
            db,
            &mut cache,
            call_expr,
            call_expr_type,
            &InferGuard::new(),
            None,
        );

        assert!(!matches!(second, Err(InferFailReason::RecursiveInfer)));
    }

    #[test]
    fn test_higher_order_call_with_unresolved_remaining_arg_should_not_hard_fail() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, R
            ---@param f fun(...: T...): R...
            ---@param ... T...
            ---@return boolean, R...
            local function wrap(f, ...) end

            ---@generic U: string
            ---@param x U
            ---@return U
            local function id(x) end

            ---@class Box
            ---@field value integer
            ---@type Box
            local box

            ok, payload = wrap(id, box.missing)
            "#,
        );

        assert_eq!(ws.expr_ty("ok"), ws.ty("boolean"));
        assert_eq!(ws.expr_ty("payload"), ws.ty("string"));
    }

    #[test]
    fn test_union_call_ignores_unresolved_alias_member() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type MissingAlias | fun(): integer
            local run

            result = run()
            "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("integer"));
    }

    #[test]
    fn test_union_call_breaks_recursive_alias_cycle() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias A A | fun(): integer
            ---@type A
            local run

            result = run()
            "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("integer"));
    }
}
