use emmylua_parser::{LuaAstToken, LuaExpr, LuaForRangeStat};

use crate::{
    DbIndex, InferFailReason, LuaDeclId, LuaInferCache, LuaOperatorMetaMethod, LuaType,
    LuaTypeCache, TplContext, TypeOps, TypeSubstitutor, VariadicType,
    compilation::analyzer::unresolve::UnResolveIterVar, infer_expr_root, instantiate_doc_function,
    tpl_pattern_match_args,
};

use super::LuaAnalyzer;

pub fn analyze_for_range_stat(
    analyzer: &mut LuaAnalyzer,
    for_range_stat: LuaForRangeStat,
) -> Option<()> {
    let var_name_list = for_range_stat.get_var_name_list();
    let iter_exprs = for_range_stat.get_expr_list().collect::<Vec<_>>();
    let cache = analyzer
        .context
        .infer_manager
        .get_infer_cache(analyzer.file_id);
    let iter_var_types = infer_for_range_iter_expr_func(analyzer.db, cache, &iter_exprs);

    match iter_var_types {
        Ok(iter_var_types) => {
            for (idx, var_name) in var_name_list.enumerate() {
                let position = var_name.get_position();
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                let ret_type = iter_var_types
                    .get_type(idx)
                    .cloned()
                    .unwrap_or(LuaType::Unknown);
                let ret_type = TypeOps::Remove.apply(analyzer.db, &ret_type, &LuaType::Nil);
                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(decl_id.into(), LuaTypeCache::InferType(ret_type));
            }
        }
        Err(InferFailReason::None) => {
            for var_name in var_name_list {
                let position = var_name.get_position();
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(decl_id.into(), LuaTypeCache::InferType(LuaType::Unknown));
            }
        }
        Err(reason) => {
            let unresolved = UnResolveIterVar {
                file_id: analyzer.file_id,
                iter_exprs: iter_exprs.clone(),
                iter_vars: var_name_list.collect::<Vec<_>>(),
            };

            analyzer
                .context
                .add_unresolve(unresolved.into(), reason.clone());
        }
    }

    Some(())
}

pub fn infer_for_range_iter_expr_func(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    iter_exprs: &[LuaExpr],
) -> Result<VariadicType, InferFailReason> {
    if iter_exprs.is_empty() {
        return Err(InferFailReason::None);
    }

    let mut status_param = None;
    if iter_exprs.len() > 1 {
        let status_param_expr = iter_exprs[1].clone();
        status_param = Some(infer_expr_root(db, cache, status_param_expr)?);
    }

    let iter_func_expr = iter_exprs[0].clone();
    let first_expr_type = infer_expr_root(db, cache, iter_func_expr)?;
    let doc_function = match first_expr_type {
        LuaType::DocFunction(func) => func,
        LuaType::Signature(sig_id) => {
            let sig = db
                .get_signature_index()
                .get(&sig_id)
                .ok_or(InferFailReason::None)?;
            if !sig.is_resolve_return() {
                return Err(InferFailReason::UnResolveSignatureReturn(sig_id));
            }
            sig.to_doc_func_type()
        }
        LuaType::Ref(type_decl_id) => {
            let type_decl = db
                .get_type_index()
                .get_type_decl(&type_decl_id)
                .ok_or(InferFailReason::None)?;
            if type_decl.is_alias() {
                let alias_origin = crate::semantic::type_queries::get_alias_origin(
                    db,
                    type_decl,
                    None,
                )
                    .ok_or(InferFailReason::None)?;
                match alias_origin {
                    LuaType::DocFunction(doc_func) => doc_func,
                    _ => return Err(InferFailReason::None),
                }
            } else if type_decl.is_class() {
                let operator_index = db.get_operator_index();
                let operator_ids = operator_index
                    .get_operators(&type_decl_id.into(), LuaOperatorMetaMethod::Call)
                    .ok_or(InferFailReason::None)?;
                operator_ids
                    .iter()
                    .filter_map(|overload_id| {
                        let operator = operator_index.get_operator(overload_id)?;
                        let func = operator.get_operator_func(db);
                        match func {
                            LuaType::DocFunction(f) => Some(f.clone()),
                            _ => None,
                        }
                    })
                    .next()
                    .ok_or(InferFailReason::None)?
            } else {
                return Err(InferFailReason::None);
            }
        }
        LuaType::Variadic(multi) => {
            let first_type = multi.get_type(0).cloned().unwrap_or(LuaType::Unknown);
            let second_type = multi.get_type(1).cloned().unwrap_or(LuaType::Unknown);
            if !second_type.is_unknown() {
                status_param = Some(second_type);
            }

            match first_type {
                LuaType::DocFunction(func) => func,
                _ => return Err(InferFailReason::None),
            }
        }
        _ => return Err(InferFailReason::None),
    };

    let Some(status_param) = status_param else {
        return Ok(doc_function.get_variadic_ret());
    };
    let mut substitutor = TypeSubstitutor::new();
    let mut context = TplContext::from_db(db, cache, &mut substitutor, None);
    let params = doc_function
        .get_params()
        .iter()
        .map(|(_, opt_ty)| opt_ty.clone().unwrap_or(LuaType::Any))
        .collect::<Vec<_>>();

    tpl_pattern_match_args(&mut context, &params, &[status_param])?;

    let instantiate_func = if let LuaType::DocFunction(f) =
        instantiate_doc_function(db, &doc_function, &substitutor)
    {
        f
    } else {
        doc_function
    };

    Ok(instantiate_func.get_variadic_ret())
}
