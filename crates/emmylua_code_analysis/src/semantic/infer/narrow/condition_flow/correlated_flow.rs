use std::collections::HashSet;

use emmylua_parser::{LuaAstPtr, LuaCallExpr, LuaChunk};

use crate::{
    DbIndex, FlowId, FlowNode, FlowTree, InferFailReason, LuaDeclId, LuaFunctionType,
    LuaInferCache, LuaSignature, LuaType, TypeOps, infer_expr, instantiate_func_generic,
    semantic::infer::{
        VarRefId,
        narrow::{get_single_antecedent, get_type_at_flow::get_type_at_flow, narrow_down_type},
    },
};

#[derive(Debug, Clone)]
pub(in crate::semantic) struct CorrelatedConditionNarrowing {
    search_root_correlated_types: Vec<SearchRootCorrelatedTypes>,
}

#[derive(Debug, Clone)]
struct SearchRootCorrelatedTypes {
    matching_target_types: Vec<LuaType>,
    uncorrelated_target_types: Vec<LuaType>,
    deferred_known_call_target_types: Option<Vec<LuaType>>,
}

#[derive(Debug)]
struct CollectedCorrelatedTypes {
    matching_target_types: Vec<LuaType>,
    correlated_candidate_types: Vec<LuaType>,
    unmatched_target_types: Vec<LuaType>,
    has_unmatched_discriminant_origin: bool,
    has_opaque_target_origin: bool,
}

impl CorrelatedConditionNarrowing {
    pub(in crate::semantic::infer::narrow) fn apply(
        &self,
        db: &DbIndex,
        antecedent_type: LuaType,
    ) -> LuaType {
        let mut root_target_types = Vec::new();
        let mut found_matching_root = false;
        for root_types in &self.search_root_correlated_types {
            let matching_target_types = &root_types.matching_target_types;
            let mut uncorrelated_target_types = root_types.uncorrelated_target_types.clone();
            let deferred_known_call_target_types =
                root_types.deferred_known_call_target_types.as_deref();

            let root_matching_target_type = if matching_target_types.is_empty() {
                None
            } else {
                let matching_target_type = LuaType::from_vec(matching_target_types.clone());
                let narrowed_correlated_type = narrow_matching_correlated_type(
                    db,
                    antecedent_type.clone(),
                    &matching_target_type,
                );
                if narrowed_correlated_type.is_never() {
                    None
                } else {
                    found_matching_root = true;
                    Some(narrowed_correlated_type)
                }
            };

            if let Some(known_call_target_types) = deferred_known_call_target_types {
                let remaining_root_type =
                    if known_call_target_types.is_empty() && uncorrelated_target_types.is_empty() {
                        Some(antecedent_type.clone())
                    } else {
                        subtract_correlated_candidate_types(
                            db,
                            antecedent_type.clone(),
                            &known_call_target_types,
                        )
                    };
                if let Some(remaining_root_type) = remaining_root_type {
                    uncorrelated_target_types.push(remaining_root_type);
                }
            }

            let root_uncorrelated_target_type = (!uncorrelated_target_types.is_empty())
                .then(|| LuaType::from_vec(uncorrelated_target_types));

            match (root_matching_target_type, root_uncorrelated_target_type) {
                (Some(root_matching_target_type), Some(root_uncorrelated_target_type)) => {
                    root_target_types.push(LuaType::from_vec(vec![
                        root_matching_target_type,
                        root_uncorrelated_target_type,
                    ]));
                }
                (Some(root_matching_target_type), None) => {
                    root_target_types.push(root_matching_target_type);
                }
                (None, Some(root_uncorrelated_target_type)) => {
                    root_target_types.push(root_uncorrelated_target_type);
                }
                (None, None) => {}
            }
        }

        if !found_matching_root {
            return antecedent_type;
        }

        if root_target_types.is_empty() {
            antecedent_type
        } else {
            LuaType::from_vec(root_target_types)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::semantic::infer::narrow) fn prepare_var_from_return_overload_condition(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    discriminant_decl_id: LuaDeclId,
    condition_position: rowan::TextSize,
    narrowed_discriminant_type: &LuaType,
) -> Result<Option<CorrelatedConditionNarrowing>, InferFailReason> {
    let Some(target_decl_id) = var_ref_id.get_decl_id_ref() else {
        return Ok(None);
    };
    if !tree.has_decl_multi_return_refs(&discriminant_decl_id)
        || !tree.has_decl_multi_return_refs(&target_decl_id)
    {
        return Ok(None);
    }

    let antecedent_flow_id = get_single_antecedent(flow_node)?;
    let search_root_flow_ids = tree.get_decl_multi_return_search_roots(
        &discriminant_decl_id,
        &target_decl_id,
        condition_position,
        antecedent_flow_id,
    );
    let root_correlated_types = search_root_flow_ids
        .iter()
        .copied()
        .map(|search_root_flow_id| {
            collect_correlated_types_from_search_root(
                db,
                tree,
                cache,
                root,
                var_ref_id,
                discriminant_decl_id,
                target_decl_id,
                condition_position,
                antecedent_flow_id,
                search_root_flow_id,
                narrowed_discriminant_type,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    if root_correlated_types
        .iter()
        .all(|root_types| root_types.matching_target_types.is_empty())
    {
        return Ok(None);
    }

    Ok(Some(CorrelatedConditionNarrowing {
        search_root_correlated_types: root_correlated_types,
    }))
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
    antecedent_flow_id: FlowId,
    search_root_flow_id: FlowId,
    narrowed_discriminant_type: &LuaType,
) -> Result<SearchRootCorrelatedTypes, InferFailReason> {
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
    let CollectedCorrelatedTypes {
        matching_target_types: root_matching_target_types,
        correlated_candidate_types: root_correlated_candidate_types,
        unmatched_target_types: root_unmatched_target_types,
        has_unmatched_discriminant_origin,
        has_opaque_target_origin,
    } = collect_matching_correlated_types(
        db,
        cache,
        root,
        &discriminant_refs,
        &target_refs,
        narrowed_discriminant_type,
    )?;

    let mut root_uncorrelated_target_types = root_unmatched_target_types;
    let has_uncorrelated_origin = discriminant_has_non_reference_origin
        || target_has_non_reference_origin
        || has_opaque_target_origin
        || has_unmatched_discriminant_origin;
    let correlated_candidate_types_is_empty = root_correlated_candidate_types.is_empty();
    let deferred_known_call_target_types =
        if has_uncorrelated_origin && search_root_flow_id == antecedent_flow_id {
            let mut known_call_target_types = root_correlated_candidate_types.clone();
            known_call_target_types.extend(root_uncorrelated_target_types.iter().cloned());
            Some(known_call_target_types)
        } else {
            None
        };
    if has_uncorrelated_origin && deferred_known_call_target_types.is_none() {
        if correlated_candidate_types_is_empty && root_uncorrelated_target_types.is_empty() {
            if let Ok(root_type) =
                get_type_at_flow(db, tree, cache, root, var_ref_id, search_root_flow_id)
            {
                root_uncorrelated_target_types.push(root_type);
            }
        } else {
            let mut known_call_target_types = root_correlated_candidate_types.clone();
            known_call_target_types.extend(root_uncorrelated_target_types.iter().cloned());
            if let Some(remaining_root_type) =
                get_type_at_flow(db, tree, cache, root, var_ref_id, search_root_flow_id)
                    .ok()
                    .and_then(|root_type| {
                        subtract_correlated_candidate_types(db, root_type, &known_call_target_types)
                    })
            {
                root_uncorrelated_target_types.push(remaining_root_type);
            }
        }
    }

    Ok(SearchRootCorrelatedTypes {
        matching_target_types: root_matching_target_types,
        uncorrelated_target_types: root_uncorrelated_target_types,
        deferred_known_call_target_types,
    })
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
                !correlated_candidate_types
                    .iter()
                    .any(|correlated_type| correlated_type_contains(db, correlated_type, member))
            })
            .collect::<Vec<_>>(),
        source_type => (!correlated_candidate_types
            .iter()
            .any(|correlated_type| correlated_type_contains(db, correlated_type, &source_type)))
        .then_some(source_type)
        .into_iter()
        .collect(),
    };

    (!remaining_types.is_empty()).then_some(LuaType::from_vec(remaining_types))
}

fn narrow_matching_correlated_type(
    db: &DbIndex,
    antecedent_type: LuaType,
    matching_target_type: &LuaType,
) -> LuaType {
    if let LuaType::Union(union) = matching_target_type {
        let narrowed_types = union
            .into_vec()
            .into_iter()
            .filter_map(|member| {
                let narrowed =
                    narrow_matching_correlated_type(db, antecedent_type.clone(), &member);
                (!narrowed.is_never()).then_some(narrowed)
            })
            .collect::<Vec<_>>();

        return if narrowed_types.is_empty() {
            LuaType::Never
        } else {
            LuaType::from_vec(narrowed_types)
        };
    }

    if matching_target_type.is_unknown()
        && let LuaType::Union(union) = &antecedent_type
    {
        let exact_unknown_types = union
            .into_vec()
            .into_iter()
            .filter(|member| member.is_unknown())
            .collect::<Vec<_>>();
        if !exact_unknown_types.is_empty() {
            return LuaType::from_vec(exact_unknown_types);
        }
    }

    if let Some(narrowed_type) = narrow_down_type(
        db,
        antecedent_type.clone(),
        matching_target_type.clone(),
        None,
    ) {
        return narrowed_type;
    }

    match antecedent_type {
        LuaType::Union(union) => {
            let narrowed_types = union
                .into_vec()
                .into_iter()
                .filter_map(|member| {
                    let narrowed =
                        narrow_matching_correlated_type(db, member, matching_target_type);
                    (!narrowed.is_never()).then_some(narrowed)
                })
                .collect::<Vec<_>>();

            if narrowed_types.is_empty() {
                LuaType::Never
            } else {
                LuaType::from_vec(narrowed_types)
            }
        }
        antecedent_type => TypeOps::Intersect.apply(db, &antecedent_type, matching_target_type),
    }
}

fn correlated_type_contains(db: &DbIndex, container: &LuaType, target: &LuaType) -> bool {
    if target.is_unknown() && !container.is_any() {
        return match container {
            LuaType::Unknown => true,
            LuaType::Union(union) => union
                .into_vec()
                .iter()
                .any(|member| correlated_type_contains(db, member, target)),
            _ => false,
        };
    }

    TypeOps::Union.apply(db, container, target) == *container
}

#[allow(clippy::too_many_arguments)]
fn collect_matching_correlated_types(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    discriminant_refs: &[crate::DeclMultiReturnRef],
    target_refs: &[crate::DeclMultiReturnRef],
    narrowed_discriminant_type: &LuaType,
) -> Result<CollectedCorrelatedTypes, InferFailReason> {
    let mut matching_target_types = Vec::new();
    let mut correlated_candidate_types = Vec::new();
    let mut unmatched_target_types = Vec::new();
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

        let overload_rows = instantiate_return_rows(db, cache, call_expr, signature);
        let discriminant_call_expr_id = discriminant_ref.call_expr.get_syntax_id();

        for target_ref in target_refs {
            if target_ref.call_expr.get_syntax_id() != discriminant_call_expr_id {
                continue;
            }
            correlated_discriminant_call_expr_ids.insert(discriminant_call_expr_id);
            correlated_target_call_expr_ids.insert(target_ref.call_expr.get_syntax_id());
            correlated_candidate_types.extend(overload_rows.iter().map(|overload| {
                LuaSignature::get_overload_row_slot(overload, target_ref.return_index)
            }));
            matching_target_types.extend(overload_rows.iter().filter_map(|overload| {
                let discriminant_type =
                    LuaSignature::get_overload_row_slot(overload, discriminant_ref.return_index);
                if !TypeOps::Intersect
                    .apply(db, &discriminant_type, narrowed_discriminant_type)
                    .is_never()
                {
                    return Some(LuaSignature::get_overload_row_slot(
                        overload,
                        target_ref.return_index,
                    ));
                }

                None
            }));
        }
    }

    let mut has_opaque_target_origin = false;
    for target_ref in target_refs {
        if correlated_target_call_expr_ids.contains(&target_ref.call_expr.get_syntax_id()) {
            continue;
        }

        let Some((call_expr, signature)) =
            infer_signature_for_call_ptr(db, cache, root, &target_ref.call_expr)?
        else {
            has_opaque_target_origin = true;
            continue;
        };
        let return_rows = instantiate_return_rows(db, cache, call_expr, signature);
        unmatched_target_types.extend(
            return_rows
                .iter()
                .map(|row| LuaSignature::get_overload_row_slot(row, target_ref.return_index)),
        );
    }

    let has_unmatched_discriminant_origin = discriminant_refs.iter().any(|discriminant_ref| {
        !correlated_discriminant_call_expr_ids.contains(&discriminant_ref.call_expr.get_syntax_id())
    });
    Ok(CollectedCorrelatedTypes {
        matching_target_types,
        correlated_candidate_types,
        unmatched_target_types,
        has_unmatched_discriminant_origin,
        has_opaque_target_origin,
    })
}

fn infer_signature_for_call_ptr<'a>(
    db: &'a DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    call_expr_ptr: &LuaAstPtr<LuaCallExpr>,
) -> Result<Option<(LuaCallExpr, &'a LuaSignature)>, InferFailReason> {
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

fn instantiate_return_rows(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
    signature: &LuaSignature,
) -> Vec<Vec<LuaType>> {
    if signature.return_overloads.is_empty() {
        let return_type = signature.get_return_type();
        let instantiated_return_type = if return_type.contain_tpl() {
            let func = LuaFunctionType::new(
                signature.async_state,
                signature.is_colon_define,
                signature.is_vararg,
                signature.get_type_params(),
                return_type.clone(),
            );
            match instantiate_func_generic(db, cache, &func, call_expr) {
                Ok(instantiated) => instantiated.get_ret().clone(),
                Err(_) => return_type,
            }
        } else {
            return_type
        };
        return vec![LuaSignature::return_type_to_row(instantiated_return_type)];
    }

    let mut rows = Vec::with_capacity(signature.return_overloads.len());
    for overload in &signature.return_overloads {
        let type_refs = &overload.type_refs;
        let overload_return_type = LuaSignature::row_to_return_type(type_refs.to_vec());
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

        rows.push(LuaSignature::return_type_to_row(instantiated_return_type));
    }

    rows
}
