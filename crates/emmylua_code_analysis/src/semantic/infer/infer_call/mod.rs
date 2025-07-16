use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaSyntaxKind};
use rowan::TextRange;

use super::{
    super::{
        generic::{instantiate_func_generic, TypeSubstitutor},
        instantiate_type_generic, resolve_signature, InferGuard, LuaInferCache,
    },
    InferFailReason, InferResult,
};
use crate::semantic::{
    generic::instantiate_doc_function, infer::narrow::get_type_at_call_expr_inline_cast,
};
use crate::{build_self_type, infer_self_type, semantic::infer_expr};
use crate::{
    CacheEntry, DbIndex, InFiled, LuaFunctionType, LuaGenericType, LuaInstanceType,
    LuaOperatorMetaMethod, LuaOperatorOwner, LuaSignatureId, LuaType, LuaTypeDeclId, LuaUnionType,
};
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
    infer_guard: &mut InferGuard,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let syntax_id = call_expr.get_syntax_id();
    let key = (syntax_id, args_count, call_expr_type.clone());
    match cache.call_cache.get(&key) {
        Some(cache) => match cache {
            CacheEntry::Cache(ty) => return Ok(ty.clone()),
            _ => return Err(InferFailReason::RecursiveInfer),
        },
        None => {}
    }

    cache.call_cache.insert(key.clone(), CacheEntry::Ready);
    let result = match &call_expr_type {
        LuaType::DocFunction(func) => {
            infer_doc_function(db, cache, &func, call_expr.clone(), args_count)
        }
        LuaType::Signature(signature_id) => infer_signature_doc_function(
            db,
            cache,
            signature_id.clone(),
            call_expr.clone(),
            args_count,
        ),
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
            &generic,
            call_expr.clone(),
            infer_guard,
            args_count,
        ),
        LuaType::Instance(inst) => infer_instance_type_doc_function(
            db,
            cache,
            &inst,
            call_expr.clone(),
            infer_guard,
            args_count,
        ),
        LuaType::TableConst(meta_table) => infer_table_type_doc_function(db, meta_table.clone()),
        LuaType::Union(union) => {
            // 此时我们将其视为泛型实例化联合体
            if union
                .into_vec()
                .iter()
                .all(|t| matches!(t, LuaType::DocFunction(_)))
            {
                infer_generic_doc_function_union(db, cache, &union, call_expr.clone(), args_count)
            } else {
                infer_union(db, cache, &union, call_expr.clone(), args_count)
            }
        }
        _ => Err(InferFailReason::None),
    };

    let result = if let Ok(func_ty) = result {
        let func_ret = func_ty.get_ret();
        match func_ret {
            LuaType::TypeGuard(_) => Ok(func_ty),
            _ => unwrapp_return_type(db, cache, func_ret.clone(), call_expr).map(|new_ret| {
                LuaFunctionType::new(
                    func_ty.is_async(),
                    func_ty.is_colon_define(),
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
        _ => {}
    }

    result
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

    return Ok(func.clone().into());
}

fn infer_generic_doc_function_union(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union: &LuaUnionType,
    call_expr: LuaCallExpr,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let overloads = union
        .into_vec()
        .iter()
        .filter_map(|typ| match typ {
            LuaType::DocFunction(f) => Some(f.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    resolve_signature(db, cache, overloads, call_expr.clone(), false, args_count)
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
    let overloads = &signature.overloads;
    if overloads.is_empty() {
        let mut fake_doc_function = LuaFunctionType::new(
            signature.is_async,
            signature.is_colon_define,
            signature.get_type_params(),
            signature.get_return_type(),
        );
        if signature.is_generic() {
            fake_doc_function = instantiate_func_generic(db, cache, &fake_doc_function, call_expr)?;
        }

        Ok(fake_doc_function.into())
    } else {
        let mut new_overloads = signature.overloads.clone();
        let fake_doc_function = Arc::new(LuaFunctionType::new(
            signature.is_async,
            signature.is_colon_define,
            signature.get_type_params(),
            signature.get_return_type(),
        ));
        new_overloads.push(fake_doc_function);

        resolve_signature(
            db,
            cache,
            new_overloads,
            call_expr.clone(),
            signature.is_generic(),
            args_count,
        )
    }
}

fn infer_type_doc_function(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    type_id: LuaTypeDeclId,
    call_expr: LuaCallExpr,
    call_expr_type: &LuaType,
    infer_guard: &mut InferGuard,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    infer_guard.check(&type_id)?;
    let type_decl = db
        .get_type_index()
        .get_type_decl(&type_id)
        .ok_or(InferFailReason::None)?;
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
        .get_operators(&type_id.into(), LuaOperatorMetaMethod::Call)
        .ok_or(InferFailReason::None)?;
    let mut overloads = Vec::new();
    for overload_id in operator_ids {
        let operator = operator_index
            .get_operator(overload_id)
            .ok_or(InferFailReason::None)?;
        let func = operator.get_operator_func(db);
        match func {
            LuaType::DocFunction(f) => {
                if f.contain_self() {
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
    infer_guard: &mut InferGuard,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let type_id = generic.get_base_type_id();
    infer_guard.check(&type_id)?;
    let generic_params = generic.get_params();
    let substitutor = TypeSubstitutor::from_type_array(generic_params.clone());

    let type_decl = db
        .get_type_index()
        .get_type_decl(&type_id)
        .ok_or(InferFailReason::None)?;
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
                // todo: support oveload?
            }
            _ => {}
        }
    }

    resolve_signature(db, cache, overloads, call_expr.clone(), false, args_count)
}

fn infer_instance_type_doc_function(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    instance: &LuaInstanceType,
    call_expr: LuaCallExpr,
    infer_guard: &mut InferGuard,
    args_count: Option<usize>,
) -> InferCallFuncResult {
    let base = instance.get_base();
    let base_table = match &base {
        LuaType::TableConst(meta_table) => meta_table.clone(),
        LuaType::Instance(inst) => {
            return infer_instance_type_doc_function(
                db,
                cache,
                inst,
                call_expr,
                infer_guard,
                args_count,
            );
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
                return Ok(func.into());
            }
            LuaType::Signature(signature_id) => {
                let signature = db
                    .get_signature_index()
                    .get(&signature_id)
                    .ok_or(InferFailReason::None)?;
                if !signature.is_resolve_return() {
                    return Err(InferFailReason::UnResolveSignatureReturn(signature_id));
                }

                return Ok(signature.to_call_operator_func_type().into());
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
    // 此时一般是 signature + doc_function 的联合体
    let mut all_overloads = Vec::new();
    let mut base_signatures = Vec::new();

    for ty in union.into_vec() {
        match ty {
            LuaType::Signature(signature_id) => {
                if let Some(signature) = db.get_signature_index().get(&signature_id) {
                    // 处理 overloads
                    let overloads = if signature.is_generic() {
                        signature
                            .overloads
                            .iter()
                            .map(|func| {
                                Ok(Arc::new(instantiate_func_generic(
                                    db,
                                    cache,
                                    func,
                                    call_expr.clone(),
                                )?))
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    } else {
                        signature.overloads.clone()
                    };
                    all_overloads.extend(overloads);

                    // 处理 signature 本身的函数类型
                    let mut fake_doc_function = LuaFunctionType::new(
                        signature.is_async,
                        signature.is_colon_define,
                        signature.get_type_params(),
                        signature.get_return_type(),
                    );
                    if signature.is_generic() {
                        fake_doc_function = instantiate_func_generic(
                            db,
                            cache,
                            &fake_doc_function,
                            call_expr.clone(),
                        )?;
                    }
                    base_signatures.push(Arc::new(fake_doc_function));
                }
            }
            LuaType::DocFunction(func) => {
                let func_to_push = if func.contain_tpl() {
                    Arc::new(instantiate_func_generic(
                        db,
                        cache,
                        &func,
                        call_expr.clone(),
                    )?)
                } else {
                    func.clone()
                };
                base_signatures.push(func_to_push);
            }
            _ => {}
        }
    }

    all_overloads.extend(base_signatures);
    if all_overloads.is_empty() {
        return Err(InferFailReason::None);
    }
    resolve_signature(db, cache, all_overloads, call_expr, false, args_count)
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

        LuaType::Variadic(variadic) => {
            if is_last_call_expr(&call_expr) {
                return Ok(return_type);
            }

            return match variadic.get_type(0) {
                Some(ty) => Ok(ty.clone()),
                None => Ok(LuaType::Nil),
            };
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

    return !call_expr.get_range().contains(inst.value.start());
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
        &mut InferGuard::new(),
        None,
    )?
    .get_ret()
    .clone();

    if let Some(tree) = db.get_flow_index().get_flow_tree(&cache.get_file_id()) {
        if let Some(flow_id) = tree.get_flow_id(call_expr.get_syntax_id()) {
            if let Some(flow_ret_type) = get_type_at_call_expr_inline_cast(
                db,
                cache,
                tree,
                call_expr,
                flow_id,
                ret_type.clone(),
            ) {
                return Ok(flow_ret_type);
            }
        }
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
