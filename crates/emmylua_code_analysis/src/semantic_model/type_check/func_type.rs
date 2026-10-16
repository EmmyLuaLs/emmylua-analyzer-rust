//! Function type checks: DocFunction structure (params/variadic/colon definition); Signature goes through semantic signatures.

use std::rc::Rc;

use crate::LuaFunctionType;
use crate::LuaSignatureId;
use crate::LuaType;

use super::context::TypeCheckContext;
use super::{RequiredChecks, TypeCheckResult, check_all, check_general_type_compact};

pub fn check_doc_func_type_compact(
    context: &mut TypeCheckContext,
    source_func: &LuaFunctionType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    match compact_type {
        LuaType::DocFunction(compact_func) => {
            check_doc_func_type_compact_for_params(context, source_func, compact_func)
        }
        LuaType::Signature(signature_id) => {
            check_doc_func_type_compact_for_signature(context, source_func, signature_id)
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            // Only callable classes declaring `---@overload fun` accept function values; ordinary classes
            // should not accept arbitrary functions as compatible (otherwise flow branches like `fun() | B` would lose errors).
            if context
                .type_def_of(id)
                .is_some_and(|def| !def.call_overloads.is_empty())
            {
                Ok(())
            } else {
                Err(context.mismatch(
                    &LuaType::DocFunction(Rc::new(source_func.clone())),
                    compact_type,
                ))
            }
        }
        LuaType::Union(union) => check_all(union.iter().map(|union_type| {
            check_general_type_compact(
                context,
                &LuaType::DocFunction(Rc::new(source_func.clone())),
                union_type,
            )
        })),
        LuaType::Function => Ok(()),
        _ => Err(context.mismatch(
            &LuaType::DocFunction(Rc::new(source_func.clone())),
            compact_type,
        )),
    }
}

fn check_doc_func_type_compact_for_params(
    context: &mut TypeCheckContext,
    source_func: &LuaFunctionType,
    compact_func: &LuaFunctionType,
) -> TypeCheckResult {
    let source_params = source_func.get_params();
    let mut compact_params: Vec<(String, Option<LuaType>)> = compact_func.get_params().to_vec();

    if compact_func.is_colon_define() {
        compact_params.insert(0, ("self".to_string(), None));
    }

    let source_is_variadic = source_func.is_variadic();
    let compact_is_variadic = compact_func.is_variadic();
    let source_len = source_params.len();
    let compact_len = compact_params.len();
    let mut checks = RequiredChecks::default();
    for i in 0..compact_len {
        let Some(source_param) = source_params.get(i) else {
            break;
        };
        let compact_param = &compact_params[i];
        let source_param_type = &source_param.1;

        if source_is_variadic && i + 1 == source_len {
            checks.require(check_doc_func_type_compact_for_varargs(
                context,
                source_param_type,
                &compact_params[i..],
            ))?;
        }

        if compact_is_variadic && i + 1 == compact_len {
            break;
        }

        let compact_param_type = &compact_param.1;
        if let (Some(source_type), Some(compact_type)) = (source_param_type, compact_param_type) {
            // Parameters: when injecting values, the compact parameter type must be acceptable to the source.
            let result = check_general_type_compact(context, compact_type, source_type);
            if result.as_ref().is_err_and(|err| err.is_type_not_match())
                && i == 0
                && source_type.is_self_infer()
                && compact_param.0 == "self"
            {
                continue;
            }
            checks.require(result)?;
        }
    }

    // Return covariance (not checked in the old version; added here): the source return must be assignable to the compact return.
    checks.require(check_general_type_compact(
        context,
        source_func.get_ret(),
        compact_func.get_ret(),
    ))?;
    checks.finish()
}

fn check_doc_func_type_compact_for_varargs(
    context: &mut TypeCheckContext,
    varargs: &Option<LuaType>,
    compact_params: &[(String, Option<LuaType>)],
) -> TypeCheckResult {
    let Some(varargs_type) = varargs else {
        return Ok(());
    };
    check_all(
        compact_params
            .iter()
            .filter_map(|(_, ty)| ty.as_ref())
            .map(|compact_param_type| {
                check_general_type_compact(context, compact_param_type, varargs_type)
            }),
    )
}

fn check_doc_func_type_compact_for_signature(
    context: &mut TypeCheckContext,
    source_func: &LuaFunctionType,
    signature_id: &LuaSignatureId,
) -> TypeCheckResult {
    // M0: convert a semantic Signature (params/doc) into a DocFunction structure for comparison.
    let Some(signature) =
        context.require_complete(|| context.model.function_type_by_signature_id(signature_id))?
    else {
        return Err(context.mismatch(
            &LuaType::DocFunction(Rc::new(source_func.clone())),
            &LuaType::Signature(*signature_id),
        ));
    };
    check_doc_func_type_compact_for_params(context, source_func, &signature)
}

pub fn check_sig_type_compact(
    context: &mut TypeCheckContext,
    sig_id: &LuaSignatureId,
    compact_type: &LuaType,
) -> TypeCheckResult {
    let Some(signature) =
        context.require_complete(|| context.model.function_type_by_signature_id(sig_id))?
    else {
        return Err(context.mismatch(&LuaType::Signature(*sig_id), compact_type));
    };
    check_doc_func_type_compact(context, &signature, compact_type)
}
