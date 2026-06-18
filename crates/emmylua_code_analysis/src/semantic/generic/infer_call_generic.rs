use emmylua_parser::{LuaAstNode, LuaDocTypeList};
use emmylua_parser::{LuaCallExpr, LuaExpr};
use hashbrown::HashSet;
use std::{ops::Deref, sync::Arc};

use crate::semantic::infer::{InferResult, infer_expr_list_types};
use crate::{
    DocTypeInferContext, FileId, GenericParam, GenericTplId, LuaFunctionType, LuaGenericType,
    LuaTypeNode,
    db_index::{DbIndex, LuaType},
    infer_doc_type,
    semantic::{
        LuaInferCache,
        generic::{
            tpl_context::TplContext,
            tpl_pattern::{
                multi_param_tpl_pattern_match_multi_return, return_type_pattern_match_target_type,
                tpl_pattern_match, variadic_tpl_pattern_match,
            },
        },
        infer::InferFailReason,
        infer_expr,
        overload_resolve::{callable_accepts_args, resolve_signature_by_args},
    },
};
use crate::{
    LuaMemberOwner, LuaSemanticDeclId, LuaTypeOwner, SemanticDeclLevel, TypeVisitTrait,
    collect_callable_overload_groups, infer_node_semantic_decl,
    tpl_pattern_match_args_skip_unknown,
};

use crate::semantic::generic::{TypeSubstitutor, instantiate_type::instantiate_type_generic};

pub fn infer_call_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    func: &LuaFunctionType,
    call_expr: LuaCallExpr,
) -> Result<LuaFunctionType, InferFailReason> {
    let substitutor = build_call_generic_substitutor(db, cache, func, &call_expr)?;

    let func_type = LuaType::DocFunction(func.clone().into());
    if let LuaType::DocFunction(f) = instantiate_type_generic(db, &func_type, &substitutor) {
        Ok(f.deref().clone())
    } else {
        Ok(func.clone())
    }
}

pub fn build_call_generic_substitutor(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
) -> Result<TypeSubstitutor, InferFailReason> {
    let file_id = cache.get_file_id().clone();

    let mut substitutor = TypeSubstitutor::new();
    {
        let mut context = TplContext {
            db,
            cache,
            substitutor: &mut substitutor,
            call_expr: Some(call_expr.clone()),
        };
        // 填充前缀类型可能存在的泛型
        fill_call_prefix_substitutor(&mut context, call_expr);

        let has_func_generic = func
            .get_generic_params()
            .iter()
            .any(|generic_tpl| generic_tpl.get_tpl_id().is_func());
        if has_func_generic {
            let generic_tpls = func
                .get_generic_params()
                .iter()
                .map(|generic_tpl| generic_tpl.get_tpl_id())
                .filter(GenericTplId::is_func)
                .collect::<HashSet<_>>();
            context.substitutor.add_need_infer_tpls(generic_tpls);

            if let Some(type_list) = call_expr.get_call_generic_type_list() {
                // 如果使用了`obj:abc--[[@<string>]]("abc")`强制指定了泛型, 那么我们只需要直接应用
                apply_call_generic_type_list(db, file_id, &mut context, &type_list);
            } else {
                // 如果没有指定泛型, 则需要从调用参数中推断
                let origin_params = func.get_params();
                let mut func_params: Vec<LuaType> = origin_params
                    .iter()
                    .map(|(_, t)| t.clone().unwrap_or(LuaType::Unknown))
                    .collect();
                infer_generic_types_from_call(db, &mut context, func, call_expr, &mut func_params)?;
            }
        }
    }

    let contain_self = func.any_nested_type(|ty| matches!(ty, LuaType::SelfInfer));
    if contain_self && let Some(self_type) = infer_self_type(db, cache, call_expr) {
        substitutor.add_self_type(self_type);
    }

    Ok(substitutor)
}

fn apply_call_generic_type_list(
    db: &DbIndex,
    file_id: FileId,
    context: &mut TplContext,
    type_list: &LuaDocTypeList,
) {
    let doc_ctx = DocTypeInferContext::new(db, file_id);
    for (i, doc_type) in type_list.get_types().enumerate() {
        let typ = infer_doc_type(doc_ctx, &doc_type);
        context
            .substitutor
            .insert_type(GenericTplId::Func(i as u32), typ, true);
    }
}

pub fn as_doc_function_type(
    db: &DbIndex,
    callable_type: &LuaType,
) -> Result<Option<Arc<LuaFunctionType>>, InferFailReason> {
    Ok(match callable_type {
        LuaType::DocFunction(doc_func) => Some(doc_func.clone()),
        LuaType::Signature(sig_id) => Some(
            db.get_signature_index()
                .get(sig_id)
                .ok_or(InferFailReason::None)?
                .to_doc_func_type(),
        ),
        _ => None,
    })
}

fn infer_callable_return_from_arg_types(
    context: &mut TplContext,
    callable_type: &LuaType,
    call_arg_types: &[LuaType],
) -> Result<Option<LuaType>, InferFailReason> {
    let mut overload_groups = Vec::new();
    collect_callable_overload_groups(context.db, callable_type, &mut overload_groups)?;
    if overload_groups.is_empty() {
        return Ok(None);
    }

    let mut member_returns = Vec::new();
    for overloads in &overload_groups {
        let instantiated_overloads = overloads
            .iter()
            .filter_map(|callable| {
                instantiate_callable_from_arg_types(context, callable, call_arg_types)
            })
            .collect::<Vec<_>>();
        if instantiated_overloads.is_empty() {
            continue;
        }

        let structural_overloads = instantiated_overloads
            .iter()
            .filter(|callable| !uses_erased_function_param(callable, call_arg_types))
            .cloned()
            .collect::<Vec<_>>();
        let overloads_to_resolve = if structural_overloads.is_empty() {
            &instantiated_overloads
        } else {
            &structural_overloads
        };

        let unresolved_arg_match = overloads_to_resolve.len() > 1
            && call_arg_types
                .iter()
                .any(|arg_type| arg_type.is_any() || arg_type.is_unknown());
        if unresolved_arg_match {
            member_returns.push(LuaType::from_vec(
                overloads_to_resolve
                    .iter()
                    .map(|callable| callable.get_ret().clone())
                    .collect(),
            ));
            continue;
        }

        let callable = resolve_signature_by_args(
            context.db,
            overloads_to_resolve,
            call_arg_types,
            false,
            None,
            &[],
        );
        member_returns.push(callable?.get_ret().clone());
    }
    if member_returns.is_empty() {
        return Ok(None);
    }

    Ok(Some(LuaType::from_vec(member_returns)))
}

fn uses_erased_function_param(callable: &LuaFunctionType, call_arg_types: &[LuaType]) -> bool {
    callable
        .get_params()
        .iter()
        .zip(call_arg_types)
        .any(|((_, param_type), arg_type)| {
            matches!(param_type, Some(LuaType::Function))
                && arg_type
                    .any_type(|ty| matches!(ty, LuaType::DocFunction(_) | LuaType::Signature(_)))
        })
}

pub fn infer_callable_return_from_remaining_args(
    context: &mut TplContext,
    callable_type: &LuaType,
    arg_exprs: &[LuaExpr],
) -> Result<Option<LuaType>, InferFailReason> {
    let call_arg_types = if arg_exprs.is_empty() {
        Vec::new()
    } else {
        match infer_expr_list_types(
            context.db,
            context.cache,
            arg_exprs,
            None,
            infer_call_arg_type,
        ) {
            Ok(types) => types.into_iter().map(|(ty, _)| ty).collect::<Vec<_>>(),
            Err(_) => arg_exprs
                .iter()
                .map(|arg_expr| {
                    infer_call_arg_type(context.db, context.cache, arg_expr.clone())
                        .unwrap_or(LuaType::Unknown)
                })
                .collect::<Vec<_>>(),
        }
    };

    // Preserve any known remaining-arg shape, including arity, even when some later arguments
    // collapse to `unknown`. This avoids unioning returns from overloads that are impossible
    // for the current call.
    infer_callable_return_from_arg_types(context, callable_type, &call_arg_types)
}

fn infer_call_arg_type(db: &DbIndex, cache: &mut LuaInferCache, arg_expr: LuaExpr) -> InferResult {
    if !cache.is_no_flow() || !matches!(&arg_expr, LuaExpr::TableExpr(_)) {
        return infer_expr(db, cache, arg_expr);
    }

    // Generic call matching stays no-flow, but direct table literal arguments
    // are local shapes and do not need flow replay.
    let table_exprs = [arg_expr.get_syntax_id()];
    cache.with_replay_overlay(&[], &table_exprs, |cache| infer_expr(db, cache, arg_expr))
}

fn instantiate_callable_from_arg_types(
    context: &mut TplContext,
    callable: &Arc<LuaFunctionType>,
    call_arg_types: &[LuaType],
) -> Option<Arc<LuaFunctionType>> {
    if !callable_accepts_args(context.db, callable, call_arg_types, false, None) {
        return None;
    }

    let has_callable_tpls = callable
        .get_generic_params()
        .iter()
        .any(|generic_tpl| generic_tpl.get_tpl_id().is_func());
    if !has_callable_tpls {
        return Some(callable.clone());
    }

    let callable_tpls = callable
        .get_generic_params()
        .iter()
        .map(|generic_tpl| generic_tpl.get_tpl_id())
        .filter(GenericTplId::is_func)
        .collect::<HashSet<_>>();

    let callable_param_types = callable
        .get_params()
        .iter()
        .map(|(_, ty)| ty.clone().unwrap_or(LuaType::Unknown))
        .collect::<Vec<_>>();
    let mut callable_substitutor = TypeSubstitutor::new();
    callable_substitutor.add_need_infer_tpls(callable_tpls.clone());
    let mut callable_context = TplContext {
        db: context.db,
        cache: context.cache,
        substitutor: &mut callable_substitutor,
        call_expr: context.call_expr.clone(),
    };
    if tpl_pattern_match_args_skip_unknown(
        &mut callable_context,
        &callable_param_types,
        call_arg_types,
    )
    .is_err()
    {
        return None;
    }

    let callable_type = LuaType::DocFunction(callable.clone());
    let instantiated =
        match instantiate_type_generic(context.db, &callable_type, &callable_substitutor) {
            LuaType::DocFunction(func) => func,
            _ => callable.clone(),
        };
    let unresolved_return_tpls = {
        let mut tpl_ids = HashSet::new();
        instantiated.get_ret().visit_type(&mut |ty| {
            if let LuaType::TplRef(generic_tpl) = ty
                && callable_tpls.contains(&generic_tpl.get_tpl_id())
            {
                tpl_ids.insert(generic_tpl.get_tpl_id());
            }
        });
        if tpl_ids.is_empty() {
            return Some(instantiated);
        }
        tpl_ids
    };

    let callback_return_tpls = collect_callback_return_tpls(
        context.db,
        &callable_param_types,
        call_arg_types,
        &unresolved_return_tpls,
    );
    if callback_return_tpls != unresolved_return_tpls {
        return None;
    }

    for tpl_id in callback_return_tpls {
        callable_substitutor.insert_type(tpl_id, LuaType::Unknown, true);
    }
    match instantiate_type_generic(context.db, &callable_type, &callable_substitutor) {
        LuaType::DocFunction(func) => Some(func),
        _ => None,
    }
}

/// Finds callback return templates that are unresolved for this call.
///
/// ```lua
/// ---@generic A, R
/// ---@param f fun(x: A): R
/// ---@param x A
/// ---@return R
/// local function apply(f, x) end
///
/// ---@type table
/// local source
/// apply(function(x) return source.missing end, 1)
/// ```
///
/// In this call, the callback return is unresolved, so this returns `R` from
/// the `f` parameter.
fn collect_callback_return_tpls(
    db: &DbIndex,
    callable_param_types: &[LuaType],
    call_arg_types: &[LuaType],
    unresolved_return_tpls: &HashSet<GenericTplId>,
) -> HashSet<GenericTplId> {
    let mut callback_return_tpls = HashSet::new();
    for (param_type, arg_type) in callable_param_types.iter().zip(call_arg_types) {
        let arg_return_unresolved = arg_type.any_type(|ty| {
            let LuaType::Signature(signature_id) = ty else {
                return false;
            };
            db.get_signature_index()
                .get(signature_id)
                .is_some_and(|signature| !signature.is_resolve_return())
        });
        if !arg_return_unresolved {
            continue;
        }

        let Ok(Some(param_func)) = as_doc_function_type(db, param_type) else {
            continue;
        };
        param_func.get_ret().visit_type(&mut |ty| {
            if let LuaType::TplRef(generic_tpl) = ty {
                let tpl_id = generic_tpl.get_tpl_id();
                if unresolved_return_tpls.contains(&tpl_id) {
                    callback_return_tpls.insert(tpl_id);
                }
            }
        });
    }

    callback_return_tpls
}

fn infer_generic_types_from_call(
    db: &DbIndex,
    context: &mut TplContext,
    func: &LuaFunctionType,
    call_expr: &LuaCallExpr,
    func_params: &mut Vec<LuaType>,
) -> Result<(), InferFailReason> {
    let colon_call = call_expr.is_colon_call();
    let colon_define = func.is_colon_define();
    match (colon_define, colon_call) {
        (true, false) => {
            func_params.insert(0, LuaType::Any);
        }
        (false, true) => {
            if !func_params.is_empty() {
                func_params.remove(0);
            }
        }
        _ => {}
    }

    let mut unresolve_tpls = vec![];
    let arg_exprs = call_expr
        .get_args_list()
        .ok_or(InferFailReason::None)?
        .get_args()
        .collect::<Vec<_>>();
    for i in 0..func_params.len() {
        if i >= arg_exprs.len() {
            if let LuaType::Variadic(variadic) = &func_params[i] {
                variadic_tpl_pattern_match(context, variadic, &[])?;
            }
            break;
        }

        if context.substitutor.is_infer_all_tpl() {
            break;
        }

        let func_param_type = &func_params[i];
        let call_arg_expr = &arg_exprs[i];
        if !func_param_type.contains_tpl_node() {
            continue;
        }

        let doc_param_func = as_doc_function_type(db, func_param_type)?;

        if !func_param_type.is_variadic()
            && check_expr_can_later_infer_with_doc_func(doc_param_func.as_deref(), call_arg_expr)
        {
            // 如果参数不能被后续推断, 那么我们先不处理
            unresolve_tpls.push((func_param_type.clone(), call_arg_expr.clone()));
            continue;
        }

        let arg_type = match infer_call_arg_type(db, context.cache, call_arg_expr.clone()) {
            Ok(t) => t,
            Err(InferFailReason::FieldNotFound) => LuaType::Nil, // 对于未找到的字段, 我们认为是 nil 以执行后续推断
            Err(e) => return Err(e),
        };

        if let Some(doc_func) = &doc_param_func {
            let return_pattern = doc_func.get_ret();
            if let Some(inferred_return_type) =
                infer_callable_return_from_remaining_args(context, &arg_type, &arg_exprs[i + 1..])?
            {
                return_type_pattern_match_target_type(
                    context,
                    return_pattern,
                    &inferred_return_type,
                )?;
            } else if arg_type.is_any() || arg_type.is_unknown() {
                return_type_pattern_match_target_type(context, return_pattern, &LuaType::Unknown)?;
            }
        }

        match (func_param_type, &arg_type) {
            (LuaType::Variadic(variadic), _) => {
                let mut arg_types = vec![];
                for arg_expr in &arg_exprs[i..] {
                    let arg_type = infer_call_arg_type(db, context.cache, arg_expr.clone())?;
                    arg_types.push(arg_type);
                }
                variadic_tpl_pattern_match(context, variadic, &arg_types)?;
                break;
            }
            (_, LuaType::Variadic(variadic)) => {
                let func_param_types = func_params[i..].to_vec();
                multi_param_tpl_pattern_match_multi_return(context, &func_param_types, variadic)?;
                break;
            }
            _ => {
                tpl_pattern_match(context, func_param_type, &arg_type)?;
            }
        }
    }

    if !context.substitutor.is_infer_all_tpl() {
        for (func_param_type, call_arg_expr) in unresolve_tpls {
            let closure_type = infer_expr(db, context.cache, call_arg_expr)?;

            tpl_pattern_match(context, &func_param_type, &closure_type)?;
        }
    }

    Ok(())
}

pub fn build_self_type(db: &DbIndex, self_type: &LuaType) -> LuaType {
    match self_type {
        LuaType::Def(id) | LuaType::Ref(id) => {
            if let Some(generic) = db.get_type_index().get_generic_params(id) {
                let mut params = Vec::with_capacity(generic.len());
                let mut substitutor = TypeSubstitutor::new();
                for (i, generic_param) in generic.iter().enumerate() {
                    let tpl_id = GenericTplId::Type(i as u32);
                    let param = build_self_generic_arg(db, generic_param, &substitutor);
                    substitutor.insert_type(tpl_id, param.clone(), true);
                    params.push(param);
                }
                let generic = LuaGenericType::new(id.clone(), params);
                return LuaType::Generic(Arc::new(generic));
            }
        }
        _ => {}
    };
    self_type.clone()
}

fn build_self_generic_arg(
    db: &DbIndex,
    generic_param: &GenericParam,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let Some(arg) = generic_param
        .default
        .as_ref()
        .or(generic_param.constraint.as_ref())
    else {
        return LuaType::Unknown;
    };

    instantiate_type_generic(db, arg, substitutor)
}

pub fn infer_self_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: &LuaCallExpr,
) -> Option<LuaType> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    match prefix_expr {
        LuaExpr::IndexExpr(index) => {
            let self_expr = index.get_prefix_expr()?;
            let self_type = infer_expr(db, cache, self_expr).ok()?;
            let self_type = build_self_type(db, &self_type);
            return Some(self_type);
        }
        LuaExpr::NameExpr(name) => {
            let semantic_decl_id = infer_node_semantic_decl(
                db,
                cache,
                name.syntax().clone(),
                SemanticDeclLevel::default(),
            )?;
            match semantic_decl_id {
                LuaSemanticDeclId::Member(member_id) => {
                    let owner = db.get_member_index().get_current_owner(&member_id)?;
                    if let LuaMemberOwner::Type(id) = owner {
                        let typ = LuaType::Ref(id.clone());
                        let self_type = build_self_type(db, &typ);
                        return Some(self_type);
                    }
                    return None;
                }
                LuaSemanticDeclId::LuaDecl(decl_id) => {
                    let typ = db
                        .get_type_index()
                        .get_type_cache(&LuaTypeOwner::Decl(decl_id))
                        .map(|cache| cache.as_type())
                        .unwrap_or(&LuaType::Unknown)
                        .clone();
                    let self_type = build_self_type(db, &typ);
                    return Some(self_type);
                }
                _ => return None,
            }
        }
        _ => {}
    }

    None
}

fn check_expr_can_later_infer_with_doc_func(
    doc_function: Option<&LuaFunctionType>,
    call_arg_expr: &LuaExpr,
) -> bool {
    let Some(doc_function) = doc_function else {
        return false;
    };

    if let LuaExpr::ClosureExpr(_) = call_arg_expr {
        return true;
    }

    let doc_params = doc_function.get_params();
    let variadic_count = doc_params
        .iter()
        .filter(|(_, t)| matches!(t, Some(LuaType::Variadic(_))))
        .count();

    variadic_count > 1
}

fn fill_call_prefix_substitutor(context: &mut TplContext, call_expr: &LuaCallExpr) -> Option<()> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    if let LuaExpr::IndexExpr(index_expr) = prefix_expr {
        let self_expr = index_expr.get_prefix_expr()?;
        let self_type = infer_expr(context.db, context.cache, self_expr).ok()?;
        if let LuaType::Generic(generic) = self_type {
            for (i, param) in generic.get_params().iter().enumerate() {
                context
                    .substitutor
                    .insert_type(GenericTplId::Type(i as u32), param.clone(), true);
            }
            return Some(());
        }
    }
    None
}
