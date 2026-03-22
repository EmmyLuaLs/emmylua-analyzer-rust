use std::collections::HashSet;

use emmylua_parser::{LuaAstPtr, LuaCallExpr, LuaChunk};

use crate::{
    DbIndex, FlowId, FlowNode, FlowTree, InferFailReason, LuaDeclId, LuaFunctionType,
    LuaInferCache, LuaType, TypeOps, infer_expr, instantiate_func_generic,
    semantic::infer::{
        VarRefId,
        narrow::{ResultTypeOrContinue, get_single_antecedent, get_type_at_flow::get_type_at_flow},
    },
};

#[allow(clippy::too_many_arguments)]
pub(in crate::semantic::infer::narrow::condition_flow) fn narrow_var_from_return_overload_condition(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    discriminant_decl_id: LuaDeclId,
    condition_position: rowan::TextSize,
    narrowed_discriminant_type: &LuaType,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(target_decl_id) = var_ref_id.get_decl_id_ref() else {
        return Ok(ResultTypeOrContinue::Continue);
    };
    if !tree.has_decl_multi_return_refs(&discriminant_decl_id)
        || !tree.has_decl_multi_return_refs(&target_decl_id)
    {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
    let search_root_flow_ids = tree.get_decl_multi_return_search_roots(
        &discriminant_decl_id,
        &target_decl_id,
        condition_position,
        antecedent_flow_id,
    );
    let mut matching_target_types = Vec::new();
    let mut uncorrelated_target_types = Vec::new();
    for search_root_flow_id in search_root_flow_ids {
        let (root_matching_target_types, root_uncorrelated_target_type) =
            collect_correlated_types_from_search_root(
                db,
                tree,
                cache,
                root,
                var_ref_id,
                discriminant_decl_id,
                target_decl_id,
                condition_position,
                search_root_flow_id,
                narrowed_discriminant_type,
            )?;
        matching_target_types.extend(root_matching_target_types);
        if let Some(root_uncorrelated_target_type) = root_uncorrelated_target_type {
            uncorrelated_target_types.push(root_uncorrelated_target_type);
        }
    }

    if matching_target_types.is_empty() {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let matching_target_type = LuaType::from_vec(matching_target_types);
    let antecedent_type = get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
    let narrowed_correlated_type =
        TypeOps::Intersect.apply(db, &antecedent_type, &matching_target_type);
    if narrowed_correlated_type.is_never() {
        return Ok(ResultTypeOrContinue::Continue);
    }

    if uncorrelated_target_types.is_empty() {
        return Ok(if narrowed_correlated_type == antecedent_type {
            ResultTypeOrContinue::Continue
        } else {
            ResultTypeOrContinue::Result(narrowed_correlated_type)
        });
    }

    let uncorrelated_target_type = LuaType::from_vec(uncorrelated_target_types);
    let merged_type = if uncorrelated_target_type.is_never() {
        narrowed_correlated_type
    } else {
        LuaType::from_vec(vec![narrowed_correlated_type, uncorrelated_target_type])
    };

    Ok(if merged_type == antecedent_type {
        ResultTypeOrContinue::Continue
    } else {
        ResultTypeOrContinue::Result(merged_type)
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_correlated_types_from_search_root(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    discriminant_decl_id: LuaDeclId,
    target_decl_id: LuaDeclId,
    condition_position: rowan::TextSize,
    search_root_flow_id: FlowId,
    narrowed_discriminant_type: &LuaType,
) -> Result<(Vec<LuaType>, Option<LuaType>), InferFailReason> {
    let (discriminant_refs, discriminant_has_non_reference_origin) = tree
        .get_decl_multi_return_ref_summary_at(
            &discriminant_decl_id,
            condition_position,
            search_root_flow_id,
        );
    let (target_refs, target_has_non_reference_origin) = tree.get_decl_multi_return_ref_summary_at(
        &target_decl_id,
        condition_position,
        search_root_flow_id,
    );
    if discriminant_refs.is_empty() || target_refs.is_empty() {
        return Ok((
            Vec::new(),
            get_type_at_flow(db, tree, cache, root, var_ref_id, search_root_flow_id).ok(),
        ));
    }

    let (
        root_matching_target_types,
        root_correlated_candidate_types,
        has_unmatched_correlated_origin,
    ) = collect_matching_correlated_types(
        db,
        cache,
        root,
        &discriminant_refs,
        &target_refs,
        narrowed_discriminant_type,
    )?;
    if root_matching_target_types.is_empty() {
        return Ok((
            Vec::new(),
            get_type_at_flow(db, tree, cache, root, var_ref_id, search_root_flow_id).ok(),
        ));
    }

    let root_uncorrelated_target_type = if discriminant_has_non_reference_origin
        || target_has_non_reference_origin
        || has_unmatched_correlated_origin
    {
        get_type_at_flow(db, tree, cache, root, var_ref_id, search_root_flow_id)
            .ok()
            .and_then(|root_type| {
                subtract_correlated_candidate_types(db, root_type, &root_correlated_candidate_types)
            })
    } else {
        None
    };

    Ok((root_matching_target_types, root_uncorrelated_target_type))
}

fn subtract_correlated_candidate_types(
    db: &DbIndex,
    source_type: LuaType,
    correlated_candidate_types: &[LuaType],
) -> Option<LuaType> {
    let remaining_types = match source_type {
        LuaType::Union(union) => union
            .into_vec()
            .into_iter()
            .filter(|member| {
                !correlated_candidate_types.iter().any(|correlated_type| {
                    TypeOps::Union.apply(db, correlated_type, member) == *correlated_type
                })
            })
            .collect::<Vec<_>>(),
        source_type => (!correlated_candidate_types.iter().any(|correlated_type| {
            TypeOps::Union.apply(db, correlated_type, &source_type) == *correlated_type
        }))
        .then_some(source_type)
        .into_iter()
        .collect(),
    };

    (!remaining_types.is_empty()).then_some(LuaType::from_vec(remaining_types))
}

#[allow(clippy::too_many_arguments)]
fn collect_matching_correlated_types(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    discriminant_refs: &[crate::DeclMultiReturnRef],
    target_refs: &[crate::DeclMultiReturnRef],
    narrowed_discriminant_type: &LuaType,
) -> Result<(Vec<LuaType>, Vec<LuaType>, bool), InferFailReason> {
    let mut matching_target_types = Vec::new();
    let mut correlated_candidate_types = Vec::new();
    let mut correlated_discriminant_call_expr_ids = HashSet::new();
    let mut correlated_target_call_expr_ids = HashSet::new();

    for discriminant_ref in discriminant_refs {
        let Some((call_expr, signature)) =
            infer_signature_for_call_ptr(db, cache, root, &discriminant_ref.call_expr)?
        else {
            continue;
        };
        if signature.return_overloads.is_empty() {
            continue;
        }

        let overload_rows = instantiate_return_overload_rows(db, cache, call_expr, signature);
        let discriminant_call_expr_id = discriminant_ref.call_expr.get_syntax_id();

        for target_ref in target_refs {
            if target_ref.call_expr.get_syntax_id() != discriminant_call_expr_id {
                continue;
            }
            correlated_discriminant_call_expr_ids.insert(discriminant_call_expr_id);
            correlated_target_call_expr_ids.insert(target_ref.call_expr.get_syntax_id());
            correlated_candidate_types.extend(overload_rows.iter().map(|overload| {
                crate::LuaSignature::get_overload_row_slot(overload, target_ref.return_index)
            }));
            matching_target_types.extend(overload_rows.iter().filter_map(|overload| {
                let discriminant_type = crate::LuaSignature::get_overload_row_slot(
                    overload,
                    discriminant_ref.return_index,
                );
                if !TypeOps::Intersect
                    .apply(db, &discriminant_type, narrowed_discriminant_type)
                    .is_never()
                {
                    return Some(crate::LuaSignature::get_overload_row_slot(
                        overload,
                        target_ref.return_index,
                    ));
                }

                None
            }));
        }
    }

    let has_unmatched_correlated_origin = discriminant_refs.iter().any(|discriminant_ref| {
        !correlated_discriminant_call_expr_ids.contains(&discriminant_ref.call_expr.get_syntax_id())
    }) || target_refs.iter().any(|target_ref| {
        !correlated_target_call_expr_ids.contains(&target_ref.call_expr.get_syntax_id())
    });
    Ok((
        matching_target_types,
        correlated_candidate_types,
        has_unmatched_correlated_origin,
    ))
}

fn infer_signature_for_call_ptr<'a>(
    db: &'a DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    call_expr_ptr: &LuaAstPtr<LuaCallExpr>,
) -> Result<Option<(LuaCallExpr, &'a crate::LuaSignature)>, InferFailReason> {
    let Some(call_expr) = call_expr_ptr.to_node(root) else {
        return Ok(None);
    };
    let Some(prefix_expr) = call_expr.get_prefix_expr() else {
        return Ok(None);
    };
    let signature_id = match infer_expr(db, cache, prefix_expr)? {
        LuaType::Signature(signature_id) => signature_id,
        _ => return Ok(None),
    };
    let Some(signature) = db.get_signature_index().get(&signature_id) else {
        return Ok(None);
    };

    Ok(Some((call_expr, signature)))
}

fn instantiate_return_overload_rows(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
    signature: &crate::LuaSignature,
) -> Vec<Vec<LuaType>> {
    let mut rows = Vec::with_capacity(signature.return_overloads.len());
    for overload in &signature.return_overloads {
        let type_refs = &overload.type_refs;
        let overload_return_type = crate::LuaSignature::row_to_return_type(type_refs.to_vec());
        let instantiated_return_type = if overload_return_type.contain_tpl() {
            let overload_func = LuaFunctionType::new(
                signature.async_state,
                signature.is_colon_define,
                signature.is_vararg,
                signature.get_type_params(),
                overload_return_type.clone(),
            );
            match instantiate_func_generic(db, cache, &overload_func, call_expr.clone()) {
                Ok(instantiated) => instantiated.get_ret().clone(),
                Err(_) => overload_return_type,
            }
        } else {
            overload_return_type
        };

        rows.push(crate::LuaSignature::return_type_to_row(
            instantiated_return_type,
        ));
    }

    rows
}
