//! Function type checks: DocFunction structure (params/variadic/colon definition); Signature goes through salsa signatures.

use crate::LuaFunctionType;
use crate::LuaSignatureId;
use crate::LuaType;

use super::context::TypeCheckContext;
use super::guard::TypeCheckGuard;
use super::{TypeCheckResult, check_general_type_compact};

pub fn check_doc_func_type_compact(
    context: &mut TypeCheckContext,
    source_func: &LuaFunctionType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::DocFunction(compact_func) => {
            check_doc_func_type_compact_for_params(context, source_func, compact_func, check_guard)
        }
        LuaType::Signature(signature_id) => check_doc_func_type_compact_for_signature(
            context,
            source_func,
            signature_id,
            check_guard,
        ),
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
                    &LuaType::DocFunction(std::sync::Arc::new(source_func.clone())),
                    compact_type,
                ))
            }
        }
        LuaType::Union(union) => {
            for union_type in union.into_vec() {
                check_doc_func_type_compact(
                    context,
                    source_func,
                    &union_type,
                    check_guard.next_level()?,
                )?;
            }
            Ok(())
        }
        LuaType::Function => Ok(()),
        _ => Err(context.mismatch(
            &LuaType::DocFunction(std::sync::Arc::new(source_func.clone())),
            compact_type,
        )),
    }
}

fn check_doc_func_type_compact_for_params(
    context: &mut TypeCheckContext,
    source_func: &LuaFunctionType,
    compact_func: &LuaFunctionType,
    check_guard: TypeCheckGuard,
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
    for i in 0..compact_len {
        let Some(source_param) = source_params.get(i) else {
            break;
        };
        let compact_param = &compact_params[i];
        let source_param_type = &source_param.1;

        if source_is_variadic && i + 1 == source_len {
            check_doc_func_type_compact_for_varargs(
                context,
                source_param_type,
                &compact_params[i..],
                check_guard.next_level()?,
            )?;
        }

        if compact_is_variadic && i + 1 == compact_len {
            break;
        }

        let compact_param_type = &compact_param.1;
        if let (Some(source_type), Some(compact_type)) = (source_param_type, compact_param_type) {
            // Parameters: when injecting values, the compact parameter type must be acceptable to the source.
            match check_general_type_compact(
                context,
                compact_type,
                source_type,
                check_guard.next_level()?,
            ) {
                Ok(()) => {}
                Err(e) if e.is_type_not_match() => {
                    if i == 0 && source_type.is_self_infer() && compact_param.0 == "self" {
                        continue;
                    }
                    return Err(e);
                }
                Err(e) => return Err(e),
            }
        }
    }

    // Return covariance (not checked in the old version; added here): the source return must be assignable to the compact return.
    check_general_type_compact(
        context,
        source_func.get_ret(),
        compact_func.get_ret(),
        check_guard.next_level()?,
    )
}

fn check_doc_func_type_compact_for_varargs(
    context: &mut TypeCheckContext,
    varargs: &Option<LuaType>,
    compact_params: &[(String, Option<LuaType>)],
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if let Some(varargs_type) = varargs {
        for compact_param in compact_params {
            if let Some(compact_param_type) = &compact_param.1 {
                check_general_type_compact(
                    context,
                    compact_param_type,
                    varargs_type,
                    check_guard.next_level()?,
                )?;
            }
        }
    }
    Ok(())
}

fn check_doc_func_type_compact_for_signature(
    context: &mut TypeCheckContext,
    source_func: &LuaFunctionType,
    signature_id: &LuaSignatureId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // M0: convert a salsa Signature (params/doc) into a DocFunction structure for comparison.
    let Some(signature) = context.model.signature_lua_by_legacy_id(signature_id) else {
        return Err(context.mismatch(
            &LuaType::DocFunction(std::sync::Arc::new(source_func.clone())),
            &LuaType::Signature(*signature_id),
        ));
    };
    check_doc_func_type_compact_for_params(
        context,
        source_func,
        &signature,
        check_guard.next_level()?,
    )
}

pub fn check_sig_type_compact(
    context: &mut TypeCheckContext,
    sig_id: &LuaSignatureId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let Some(signature) = context.model.signature_lua_by_legacy_id(sig_id) else {
        return Err(context.mismatch(&LuaType::Signature(*sig_id), compact_type));
    };
    check_doc_func_type_compact(context, &signature, compact_type, check_guard.next_level()?)
}
