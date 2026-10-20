//! Function type checks: DocFunction structure (params/variadic/colon definition); Signature goes through semantic signatures.

use std::rc::Rc;

use crate::LuaFunctionType;
use crate::LuaSignatureId;
use crate::LuaType;

use super::context::TypeCheckContext;
use super::{RequiredChecks, TypeCheckResult, check_general_type_compact};

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
    let mut checks = RequiredChecks::default();
    for (source, target) in function_param_pairs(source_func, compact_func) {
        checks.require(check_general_type_compact(context, &source, &target))?;
    }

    // Return covariance (not checked in the old version; added here): the source return must be assignable to the compact return.
    checks.require(check_general_type_compact(
        context,
        source_func.get_ret(),
        compact_func.get_ret(),
    ))?;
    checks.finish()
}

/// Parameter constraints in value-flow order: the caller's argument type must
/// fit the implementation's parameter type. Both checkers use the same slot rules.
pub(crate) fn function_param_pairs(
    source_func: &LuaFunctionType,
    compact_func: &LuaFunctionType,
) -> Vec<(LuaType, LuaType)> {
    let source_params = source_func.get_variadic_params();
    let target_params = compact_func.get_variadic_params();
    // After both fixed prefixes, one comparison covers repeated parameter tails.
    let count = source_params
        .get_max_len()
        .unwrap_or_else(|| source_params.get_min_len().unwrap_or(0) + 1)
        .max(
            target_params
                .get_max_len()
                .unwrap_or_else(|| target_params.get_min_len().unwrap_or(0) + 1),
        );
    // Expanding tuple tails contain fixed slots; repeated tails contain none.
    let fixed_source_count = source_params.get_min_len().unwrap_or(0);
    let fixed_target_count = target_params.get_min_len().unwrap_or(0);
    let mut pairs = Vec::new();
    for index in 0..count {
        let Some(source_param) = source_params.get_type(index) else {
            // The function ignores arguments beyond its own parameters.
            break;
        };
        let target_param = match target_params.get_type(index) {
            Some(ty) => ty,
            None if index < fixed_source_count => &LuaType::Nil,
            None => break,
        };
        if source_param.is_self_infer() || target_param.is_self_infer() {
            continue;
        }
        // A repeated caller tail can be empty. A fixed implementation slot
        // beyond the caller's prefix must also accept a missing argument.
        let target_param = if index < fixed_source_count && index >= fixed_target_count {
            LuaType::from_vec(vec![target_param.clone(), LuaType::Nil])
        } else {
            target_param.clone()
        };
        pairs.push((target_param, source_param.clone()));
    }

    pairs
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
