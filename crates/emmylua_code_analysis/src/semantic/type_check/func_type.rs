use crate::db_index::{
    DbIndex, LuaFunctionType, LuaOperatorMetaMethod, LuaSignatureId, LuaType, LuaTypeDeclId,
};

use super::{
    check_general_type_compact, check_type_compact, type_check_fail_reason::TypeCheckFailReason,
    type_check_guard::TypeCheckGuard, TypeCheckResult,
};

pub fn check_doc_func_type_compact(
    db: &DbIndex,
    source_func: &LuaFunctionType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::DocFunction(compact_func) => {
            check_doc_func_type_compact_for_params(db, source_func, compact_func, check_guard)
        }
        LuaType::Signature(signature_id) => {
            check_doc_func_type_compact_for_signature(db, source_func, signature_id, check_guard)
        }
        LuaType::Ref(type_id) => {
            check_doc_func_type_compact_for_custom_type(db, source_func, type_id, check_guard)
        }
        LuaType::Def(type_id) => {
            check_doc_func_type_compact_for_custom_type(db, source_func, type_id, check_guard)
        }
        LuaType::Union(union) => {
            for union_type in union.into_vec() {
                check_doc_func_type_compact(
                    db,
                    source_func,
                    &union_type,
                    check_guard.next_level()?,
                )?;
            }

            Ok(())
        }
        LuaType::Function => Ok(()),
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn check_doc_func_type_compact_for_params(
    db: &DbIndex,
    source_func: &LuaFunctionType,
    compact_func: &LuaFunctionType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let source_params = source_func.get_params();
    let mut compact_params: Vec<(String, Option<LuaType>)> =
        compact_func.get_params().iter().cloned().collect();

    if compact_func.is_colon_define() {
        compact_params.insert(0, ("self".to_string(), None));
    }

    let compact_len = compact_params.len();

    for i in 0..compact_len {
        let source_param = match source_params.get(i) {
            Some(p) => p,
            None => {
                break;
            }
        };
        let compact_param = &compact_params[i];

        let source_param_type = &source_param.1;
        // too many complex session to handle varargs
        if source_param.0 == "..." {
            check_doc_func_type_compact_for_varargs(db, source_param_type, &compact_params[i..])?;
        }

        if compact_param.0 == "..." {
            break;
        }

        let compact_param_type = &compact_param.1;

        match (source_param_type, compact_param_type) {
            (Some(source_type), Some(compact_type)) => {
                match check_general_type_compact(
                    db,
                    source_type,
                    compact_type,
                    check_guard.next_level()?,
                ) {
                    Ok(()) => {}
                    Err(e) if e.is_type_not_match() => {
                        if i == 0 && source_type.is_self_infer() && compact_param.0 == "self" {
                            continue;
                        }
                        // add error message
                        return Err(e);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
            _ => {}
        }
    }

    // todo check return type

    Ok(())
}

fn check_doc_func_type_compact_for_varargs(
    db: &DbIndex,
    varargs: &Option<LuaType>,
    compact_params: &[(String, Option<LuaType>)],
) -> TypeCheckResult {
    if let Some(varargs) = varargs {
        let varargs_len = compact_params.len();
        let varargs_type = varargs;
        for i in 0..varargs_len {
            let compact_param = &compact_params[i];
            let compact_param_type = &compact_param.1;
            if let Some(compact_param_type) = compact_param_type {
                check_type_compact(db, varargs_type, compact_param_type)?;
            }
        }
    }

    Ok(())
}

fn check_doc_func_type_compact_for_signature(
    db: &DbIndex,
    source_func: &LuaFunctionType,
    signature_id: &LuaSignatureId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let signature = db
        .get_signature_index()
        .get(signature_id)
        .ok_or(TypeCheckFailReason::TypeNotMatch)?;

    // dotnot check generic method
    if signature.is_generic() {
        return Ok(());
    }

    for overload_func in &signature.overloads {
        match check_doc_func_type_compact_for_params(
            db,
            source_func,
            overload_func,
            check_guard.next_level()?,
        ) {
            Ok(()) => {
                return Ok(());
            }
            Err(e) if e.is_type_not_match() => {
                // continue to check next overload
                continue;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    let fake_doc_func = signature.to_doc_func_type();

    check_doc_func_type_compact_for_params(
        db,
        &source_func,
        &fake_doc_func,
        check_guard.next_level()?,
    )
}

// check type is callable
fn check_doc_func_type_compact_for_custom_type(
    db: &DbIndex,
    source_func: &LuaFunctionType,
    custom_type_id: &LuaTypeDeclId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let type_decl = db
        .get_type_index()
        .get_type_decl(custom_type_id)
        .ok_or(TypeCheckFailReason::TypeNotMatch)?;

    if type_decl.is_class() {
        let call_operators = db
            .get_operator_index()
            .get_operators(&custom_type_id.clone().into(), LuaOperatorMetaMethod::Call)
            .ok_or(TypeCheckFailReason::TypeNotMatch)?;
        for operator_id in call_operators {
            let operator = db
                .get_operator_index()
                .get_operator(operator_id)
                .ok_or(TypeCheckFailReason::TypeNotMatch)?;
            let call_type = operator.get_operator_func(db);
            match call_type {
                LuaType::DocFunction(doc_func) => {
                    match check_doc_func_type_compact_for_params(
                        db,
                        source_func,
                        &doc_func,
                        check_guard.next_level()?,
                    ) {
                        Ok(()) => return Ok(()),
                        Err(e) if e.is_type_not_match() => continue,
                        Err(e) => return Err(e),
                    }
                }
                LuaType::Signature(signature_id) => {
                    let signature = db
                        .get_signature_index()
                        .get(&signature_id)
                        .ok_or(TypeCheckFailReason::TypeNotMatch)?;
                    let doc_f = signature.to_call_operator_func_type();
                    match check_doc_func_type_compact_for_params(
                        db,
                        source_func,
                        &doc_f,
                        check_guard.next_level()?,
                    ) {
                        Ok(()) => return Ok(()),
                        Err(e) if e.is_type_not_match() => continue,
                        Err(e) => return Err(e),
                    }
                }
                _ => {}
            }
        }
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

pub fn check_sig_type_compact(
    db: &DbIndex,
    sig_id: &LuaSignatureId,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let signature = db
        .get_signature_index()
        .get(sig_id)
        .ok_or(TypeCheckFailReason::TypeNotMatch)?;

    // cannot check generic method
    if signature.is_generic() {
        return Ok(());
    }

    let fake_doc_func = signature.to_doc_func_type();

    check_doc_func_type_compact(db, &fake_doc_func, compact_type, check_guard.next_level()?)
}
