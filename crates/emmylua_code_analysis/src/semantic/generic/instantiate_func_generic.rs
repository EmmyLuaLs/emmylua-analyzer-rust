use std::{collections::HashSet, ops::Deref, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr};
use internment::ArcIntern;
use smol_str::SmolStr;

use super::{TypeSubstitutor, instantiate_type_generic::instantiate_doc_function};
use crate::semantic::generic::instantiate_type_generic::collapse_variadics_in_vec;
use crate::{
    GenericTpl, GenericTplId, LuaFunctionType, LuaGenericType, TypeVisitTrait,
    db_index::{DbIndex, LuaType},
    semantic::{
        LuaInferCache,
        generic::{
            tpl_context::TplContext,
            tpl_pattern::{
                multi_param_tpl_pattern_match_multi_return, tpl_pattern_match,
                variadic_tpl_pattern_match,
            },
        },
        infer::InferFailReason,
        infer_expr,
    },
};

pub fn instantiate_func_generic(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    func: &LuaFunctionType,
    call_expr: LuaCallExpr,
) -> Result<LuaFunctionType, InferFailReason> {
    let mut generic_tpls = HashSet::new();
    let mut contain_self = false;
    func.visit_type(&mut |t| match t {
        LuaType::TplRef(generic_tpl) | LuaType::ConstTplRef(generic_tpl) => {
            let tpl_id = generic_tpl.get_tpl_id();
            if tpl_id.is_func() {
                generic_tpls.insert(tpl_id);
            }
        }
        LuaType::StrTplRef(str_tpl) => {
            generic_tpls.insert(str_tpl.get_tpl_id());
        }
        LuaType::SelfInfer => {
            contain_self = true;
        }
        _ => {}
    });

    let origin_params = func.get_params();
    let mut func_param_types: Vec<_> = origin_params
        .iter()
        .map(|(_, t)| t.clone().unwrap_or(LuaType::Unknown))
        .collect();
    let mut func_param_names: Vec<_> = origin_params
        .iter()
        .map(|(name, _)| name.as_ref())
        .collect();

    let arg_exprs = call_expr
        .get_args_list()
        .ok_or(InferFailReason::None)?
        .get_args()
        .collect::<Vec<_>>();
    let mut substitutor = TypeSubstitutor::new();
    let mut context = TplContext {
        db,
        cache,
        substitutor: &mut substitutor,
        root: call_expr.get_root(),
        call_expr: Some(call_expr.clone()),
    };
    if !generic_tpls.is_empty() {
        context.substitutor.add_need_infer_tpls(generic_tpls);

        let colon_call = call_expr.is_colon_call();
        let colon_define = func.is_colon_define();
        match (colon_define, colon_call) {
            (true, false) => {
                func_param_types.insert(0, LuaType::Any);
                func_param_names.insert(0, "self");
            }
            (false, true) => {
                if !func_param_types.is_empty() {
                    func_param_types.remove(0);
                    func_param_names.remove(0);
                }
            }
            _ => {}
        }

        let mut unresolve_tpls = vec![];
        for i in 0..func_param_types.len() {
            if context.substitutor.is_infer_all_tpl() {
                break;
            }

            let func_param_type = &func_param_types[i];
            if !func_param_type.contain_tpl() {
                continue;
            }

            if !func_param_type.is_variadic() && i < arg_exprs.len() {
                let call_arg_expr = &arg_exprs[i];
                if check_expr_can_later_infer(&mut context, func_param_type, call_arg_expr)? {
                    // Special case for closure types that have more than one variadic param.
                    // I.e. if we see `fun(_: T1..., _: T2...)`, we'll try to infer `T1` or `T2`
                    // from other arguments, and resolve the other later.
                    unresolve_tpls.push((func_param_type.clone(), call_arg_expr.clone()));
                    continue;
                }
            }

            if let LuaType::Variadic(func_param_variadic) = func_param_type {
                // Match the rest of the args with a variadic parameter.
                let mut arg_types = vec![];
                for j in i..arg_exprs.len() {
                    let arg_type = infer_expr(db, context.cache, arg_exprs[j].clone())?;
                    arg_types.push(arg_type);
                }

                // The type of the last argument can be a variadic. That is, if we're matching
                // an argument of `foo(...)`, and call expression is something like
                // `foo(A, B, unpack({C, D, unpack(E[])}))`, then `arg_types` will end up
                // looking like `[A, B, Multi([C, D, Base(E)])]`. We need to flatten it into
                // `[A, B, C, D, Base(E)]` before matching.
                let arg_types = collapse_variadics_in_vec(arg_types);

                variadic_tpl_pattern_match(&mut context, func_param_variadic, &arg_types)?;
                break;
            } /* else if func_param_names[i] == "..." {
            // Match the rest of the args with a variadic parameter.
            let mut arg_types = vec![];
            for j in i..arg_exprs.len() {
            let arg_type = infer_expr(db, context.cache, arg_exprs[j].clone())?;
            arg_types.push(arg_type);
            }

            // The type of the last argument can be a variadic. That is, if we're matching
            // an argument of `foo(...)`, and call expression is something like
            // `foo(A, B, unpack({C, D, unpack(E[])}))`, then `arg_types` will end up
            // looking like `[A, B, Multi([C, D, Base(E)])]`. We need to flatten it into
            // `[A, B, C, D, Base(E)]` before matching.
            let arg_types = collapse_variadics_in_vec(arg_types);

            variadic_tpl_pattern_match(&mut context, func_param_variadic, &arg_types)?;
            break;
            }*/

            let arg_type = if let Some(arg_expr) = arg_exprs.get(i) {
                infer_expr(db, context.cache, arg_expr.clone())?
            } else {
                LuaType::Nil
            };

            if let LuaType::Variadic(arg_type_variadic) = &arg_type {
                // Match a variadic argument with the rest of the parameters.
                multi_param_tpl_pattern_match_multi_return(
                    &mut context,
                    &func_param_types[i..],
                    arg_type_variadic,
                )?;
                break;
            }

            // Match one argument with one parameter.
            tpl_pattern_match(&mut context, func_param_type, &arg_type)?;
        }

        if !context.substitutor.is_infer_all_tpl() {
            for (func_param_type, call_arg_expr) in unresolve_tpls {
                let closure_type = infer_expr(db, context.cache, call_arg_expr)?;

                tpl_pattern_match(&mut context, &func_param_type, &closure_type)?;
            }
        }
    }

    if contain_self {
        if let Some(self_type) = infer_self_type(db, cache, &call_expr) {
            substitutor.add_self_type(self_type);
        }
    }

    if let LuaType::DocFunction(f) = instantiate_doc_function(db, func, &substitutor) {
        Ok(f.deref().clone())
    } else {
        Ok(func.clone())
    }
}

pub fn build_self_type(db: &DbIndex, self_type: &LuaType) -> LuaType {
    match self_type {
        LuaType::Def(id) | LuaType::Ref(id) => {
            if let Some(generic) = db.get_type_index().get_generic_params(id) {
                let mut params = Vec::new();
                for (i, ty) in generic.iter().enumerate() {
                    if let Some(t) = &ty.1 {
                        params.push(t.clone());
                    } else {
                        params.push(LuaType::TplRef(Arc::new(GenericTpl::new(
                            GenericTplId::Type(i as u32),
                            ArcIntern::new(SmolStr::from(ty.0.clone())),
                        ))));
                    }
                }
                let generic = LuaGenericType::new(id.clone(), params);
                return LuaType::Generic(Arc::new(generic));
            }
        }
        _ => {}
    };
    self_type.clone()
}

pub fn infer_self_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: &LuaCallExpr,
) -> Option<LuaType> {
    let prefix_expr = call_expr.get_prefix_expr();
    if let Some(prefix_expr) = prefix_expr {
        if let LuaExpr::IndexExpr(index) = prefix_expr {
            let self_expr = index.get_prefix_expr();
            if let Some(self_expr) = self_expr {
                let self_type = infer_expr(db, cache, self_expr.into()).ok()?;
                let self_type = build_self_type(db, &self_type);
                return Some(self_type);
            }
        }
    }

    None
}

fn check_expr_can_later_infer(
    context: &mut TplContext,
    func_param_type: &LuaType,
    call_arg_expr: &LuaExpr,
) -> Result<bool, InferFailReason> {
    let doc_function = match func_param_type {
        LuaType::DocFunction(doc_func) => doc_func.clone(),
        LuaType::Signature(sig_id) => {
            let sig = context
                .db
                .get_signature_index()
                .get(&sig_id)
                .ok_or(InferFailReason::None)?;

            sig.to_doc_func_type()
        }
        _ => return Ok(false),
    };

    if let LuaExpr::ClosureExpr(_) = call_arg_expr {
        return Ok(true);
    }

    let doc_params = doc_function.get_params();
    let variadic_count = doc_params
        .iter()
        .filter_map(|(_, t)| {
            if let Some(LuaType::Variadic(_)) = t {
                Some(())
            } else {
                None
            }
        })
        .count();

    Ok(variadic_count > 1)
}
