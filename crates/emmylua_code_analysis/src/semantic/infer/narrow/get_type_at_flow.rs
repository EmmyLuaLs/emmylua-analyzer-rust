use std::rc::Rc;

use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaChunk, LuaExpr, LuaVarExpr};

use crate::{
    CacheEntry, DbIndex, FlowId, FlowNode, FlowNodeKind, FlowTree, InferFailReason, LuaDeclId,
    LuaInferCache, LuaMemberId, LuaMemberOwner, LuaSignatureId, LuaType, TypeOps,
    check_type_compact, infer_expr,
    semantic::{
        infer::{
            InferResult, VarRefId, infer_expr_list_value_type_at,
            narrow::{
                ResultTypeOrContinue,
                condition_flow::{
                    ConditionFlowAction, InferConditionFlow, PendingConditionNarrow,
                    get_type_at_condition_flow,
                },
                get_multi_antecedents, get_single_antecedent,
                get_type_at_cast_flow::get_type_at_cast_flow,
                get_var_ref_type, literal_provides_optional_class_field, narrow_down_type,
                var_ref_id::get_var_expr_var_ref_id,
            },
        },
        member::find_members,
    },
};

pub fn get_type_at_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_id: FlowId,
) -> InferResult {
    get_type_at_flow_internal(db, tree, cache, root, var_ref_id, flow_id, true)
}

fn can_reuse_narrowed_assignment_source(
    db: &DbIndex,
    narrowed_source_type: &LuaType,
    expr_type: &LuaType,
) -> bool {
    if matches!(expr_type, LuaType::TableConst(_) | LuaType::Object(_)) {
        return is_partial_assignment_expr_compatible(db, narrowed_source_type, expr_type);
    }

    if !is_exact_assignment_expr_type(expr_type) {
        return false;
    }

    match narrow_down_type(db, narrowed_source_type.clone(), expr_type.clone(), None) {
        Some(narrowed_expr_type) => narrowed_expr_type == *expr_type,
        None => true,
    }
}

fn preserves_assignment_expr_type(typ: &LuaType) -> bool {
    matches!(typ, LuaType::TableConst(_) | LuaType::Object(_)) || is_exact_assignment_expr_type(typ)
}

fn is_partial_assignment_expr_compatible(
    db: &DbIndex,
    source_type: &LuaType,
    expr_type: &LuaType,
) -> bool {
    if check_type_compact(db, source_type, expr_type).is_ok() {
        return true;
    }

    // Only preserve branch narrowing for concrete partial table/object literals.
    // Broader RHS expressions can carry hidden state the current flow/type model cannot represent
    // without wider semantic changes.
    if !matches!(expr_type, LuaType::TableConst(_) | LuaType::Object(_)) {
        return false;
    }

    let expr_members = find_members(db, expr_type).unwrap_or_default();

    if expr_members.is_empty() {
        return true;
    }

    let Some(source_members) = find_members(db, source_type) else {
        return false;
    };

    expr_members.into_iter().all(|expr_member| {
        match source_members
            .iter()
            .find(|source_member| source_member.key == expr_member.key)
        {
            Some(source_member) => {
                is_partial_assignment_expr_compatible(db, &source_member.typ, &expr_member.typ)
            }
            None => true,
        }
    })
}

fn is_exact_assignment_expr_type(typ: &LuaType) -> bool {
    match typ {
        LuaType::Nil | LuaType::DocBooleanConst(_) => true,
        typ if typ.is_const() => !matches!(typ, LuaType::TableConst(_)),
        LuaType::Union(union) => union.into_vec().iter().all(is_exact_assignment_expr_type),
        LuaType::MultiLineUnion(multi_union) => {
            is_exact_assignment_expr_type(&multi_union.to_union())
        }
        LuaType::TypeGuard(inner) => is_exact_assignment_expr_type(inner),
        _ => false,
    }
}

fn get_type_at_flow_internal(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_id: FlowId,
    use_condition_narrowing: bool,
) -> InferResult {
    let key = (var_ref_id.clone(), flow_id, use_condition_narrowing);
    if let Some(cache_entry) = cache.flow_node_cache.get(&key) {
        return match cache_entry {
            CacheEntry::Cache(narrow_type) => Ok::<LuaType, InferFailReason>(narrow_type.clone()),
            CacheEntry::Ready => Err(InferFailReason::RecursiveInfer),
        };
    }

    cache.flow_node_cache.insert(key.clone(), CacheEntry::Ready);

    let result = (|| {
        let result_type;
        let mut antecedent_flow_id = flow_id;
        let mut pending_condition_narrows: Vec<Rc<PendingConditionNarrow>> = Vec::new();
        loop {
            let flow_node = tree
                .get_flow_node(antecedent_flow_id)
                .ok_or(InferFailReason::None)?;

            match &flow_node.kind {
                FlowNodeKind::Start | FlowNodeKind::Unreachable => {
                    result_type = get_var_ref_type(db, cache, var_ref_id)?;
                    break;
                }
                FlowNodeKind::LoopLabel | FlowNodeKind::Break | FlowNodeKind::Return => {
                    antecedent_flow_id = get_single_antecedent(flow_node)?;
                }
                FlowNodeKind::BranchLabel | FlowNodeKind::NamedLabel(_) => {
                    let multi_antecedents = get_multi_antecedents(tree, flow_node)?;

                    let mut branch_result_type = LuaType::Never;
                    for &flow_id in &multi_antecedents {
                        let branch_type = get_type_at_flow_internal(
                            db,
                            tree,
                            cache,
                            root,
                            var_ref_id,
                            flow_id,
                            use_condition_narrowing,
                        )?;
                        branch_result_type =
                            TypeOps::Union.apply(db, &branch_result_type, &branch_type);
                    }
                    result_type = branch_result_type;
                    break;
                }
                FlowNodeKind::DeclPosition(position) => {
                    if *position <= var_ref_id.get_position() {
                        match get_var_ref_type(db, cache, var_ref_id) {
                            Ok(var_type) => {
                                result_type = try_narrow_decl_to_instance(
                                    db, cache, root, var_ref_id, &var_type,
                                )
                                .unwrap_or(var_type);
                                break;
                            }
                            Err(err) => {
                                // 尝试推断声明位置的类型, 如果发生错误则返回初始错误, 否则返回当前推断错误
                                if let Some(init_type) =
                                    try_infer_decl_initializer_type(db, cache, root, var_ref_id)?
                                {
                                    result_type = init_type;
                                    break;
                                }

                                return Err(err);
                            }
                        }
                    } else {
                        antecedent_flow_id = get_single_antecedent(flow_node)?;
                    }
                }
                FlowNodeKind::Assignment(assign_ptr) => {
                    let assign_stat = assign_ptr.to_node(root).ok_or(InferFailReason::None)?;
                    let result_or_continue = get_type_at_assign_stat(
                        db,
                        tree,
                        cache,
                        root,
                        var_ref_id,
                        flow_node,
                        assign_stat,
                    )?;

                    if let ResultTypeOrContinue::Result(assign_type) = result_or_continue {
                        result_type = assign_type;
                        break;
                    } else {
                        antecedent_flow_id = get_single_antecedent(flow_node)?;
                    }
                }
                FlowNodeKind::ImplFunc(func_ptr) => {
                    let func_stat = func_ptr.to_node(root).ok_or(InferFailReason::None)?;
                    let Some(func_name) = func_stat.get_func_name() else {
                        antecedent_flow_id = get_single_antecedent(flow_node)?;
                        continue;
                    };

                    let Some(ref_id) = get_var_expr_var_ref_id(db, cache, func_name.to_expr())
                    else {
                        antecedent_flow_id = get_single_antecedent(flow_node)?;
                        continue;
                    };

                    if ref_id == *var_ref_id {
                        let Some(closure) = func_stat.get_closure() else {
                            return Err(InferFailReason::None);
                        };

                        result_type = LuaType::Signature(LuaSignatureId::from_closure(
                            cache.get_file_id(),
                            &closure,
                        ));
                        break;
                    } else {
                        antecedent_flow_id = get_single_antecedent(flow_node)?;
                    }
                }
                FlowNodeKind::TrueCondition(condition_ptr)
                | FlowNodeKind::FalseCondition(condition_ptr) => {
                    if !use_condition_narrowing {
                        antecedent_flow_id = get_single_antecedent(flow_node)?;
                        continue;
                    }

                    let condition_flow =
                        if matches!(&flow_node.kind, FlowNodeKind::TrueCondition(_)) {
                            InferConditionFlow::TrueCondition
                        } else {
                            InferConditionFlow::FalseCondition
                        };
                    let condition_key = (
                        var_ref_id.clone(),
                        antecedent_flow_id,
                        matches!(condition_flow, InferConditionFlow::TrueCondition),
                    );
                    let condition_action = {
                        if let Some(cache_entry) = cache.condition_flow_cache.get(&condition_key) {
                            match cache_entry {
                                CacheEntry::Cache(action) => {
                                    Ok::<ConditionFlowAction, InferFailReason>(action.clone())
                                }
                                CacheEntry::Ready => Err(InferFailReason::RecursiveInfer),
                            }
                        } else {
                            let condition =
                                condition_ptr.to_node(root).ok_or(InferFailReason::None)?;
                            cache
                                .condition_flow_cache
                                .insert(condition_key.clone(), CacheEntry::Ready);
                            let result = get_type_at_condition_flow(
                                db,
                                tree,
                                cache,
                                root,
                                var_ref_id,
                                flow_node,
                                condition,
                                condition_flow,
                            );
                            match &result {
                                Ok(action) => {
                                    cache
                                        .condition_flow_cache
                                        .insert(condition_key, CacheEntry::Cache(action.clone()));
                                }
                                Err(_) => {
                                    cache.condition_flow_cache.remove(&condition_key);
                                }
                            }
                            result
                        }
                    }?;

                    match condition_action {
                        ConditionFlowAction::Pending(pending_condition_narrow) => {
                            pending_condition_narrows.push(pending_condition_narrow);
                            antecedent_flow_id = get_single_antecedent(flow_node)?;
                        }
                        ConditionFlowAction::Result(condition_type) => {
                            result_type = condition_type;
                            break;
                        }
                        ConditionFlowAction::Continue => {
                            antecedent_flow_id = get_single_antecedent(flow_node)?;
                        }
                    }
                }
                FlowNodeKind::ForIStat(_) => {
                    // todo check for `for i = 1, 10 do end`
                    antecedent_flow_id = get_single_antecedent(flow_node)?;
                }
                FlowNodeKind::TagCast(cast_ast_ptr) => {
                    let tag_cast = cast_ast_ptr.to_node(root).ok_or(InferFailReason::None)?;
                    let cast_or_continue = get_type_at_cast_flow(
                        db, tree, cache, root, var_ref_id, flow_node, tag_cast,
                    )?;

                    if let ResultTypeOrContinue::Result(cast_type) = cast_or_continue {
                        result_type = cast_type;
                        break;
                    } else {
                        antecedent_flow_id = get_single_antecedent(flow_node)?;
                    }
                }
            }
        }

        let result_type = if use_condition_narrowing {
            pending_condition_narrows.into_iter().rev().fold(
                result_type,
                |result_type, pending_condition_narrow| {
                    pending_condition_narrow.apply(db, cache, result_type)
                },
            )
        } else {
            result_type
        };

        Ok(result_type)
    })();

    match &result {
        Ok(result_type) => {
            cache
                .flow_node_cache
                .insert(key, CacheEntry::Cache(result_type.clone()));
        }
        Err(_) => {
            cache.flow_node_cache.remove(&key);
        }
    }

    result
}

fn get_type_at_assign_stat(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    assign_stat: LuaAssignStat,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    for (i, var) in vars.iter().cloned().enumerate() {
        let Some(maybe_ref_id) = get_var_expr_var_ref_id(db, cache, var.to_expr()) else {
            continue;
        };

        if maybe_ref_id != *var_ref_id {
            // let typ = get_var_ref_type(db, cache, var_ref_id)?;
            continue;
        }

        // Check if there's an explicit type annotation (not just inferred type)
        let var_id = match var {
            LuaVarExpr::NameExpr(name_expr) => {
                Some(LuaDeclId::new(cache.get_file_id(), name_expr.get_position()).into())
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                Some(LuaMemberId::new(index_expr.get_syntax_id(), cache.get_file_id()).into())
            }
        };

        let explicit_var_type = var_id
            .and_then(|id| db.get_type_index().get_type_cache(&id))
            .filter(|tc| tc.is_doc())
            .map(|tc| tc.as_type().clone());

        let expr_type = infer_expr_list_value_type_at(db, cache, &exprs, i)?;
        let Some(expr_type) = expr_type else {
            return Ok(ResultTypeOrContinue::Continue);
        };

        let (source_type, reuse_source_narrowing) =
            if let Some(explicit) = explicit_var_type.clone() {
                (explicit, true)
            } else {
                let antecedent_flow_id = get_single_antecedent(flow_node)?;
                if !preserves_assignment_expr_type(&expr_type) {
                    (
                        get_type_at_flow_internal(
                            db,
                            tree,
                            cache,
                            root,
                            var_ref_id,
                            antecedent_flow_id,
                            false,
                        )?,
                        false,
                    )
                } else {
                    let narrowed_source_type =
                        get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
                    if can_reuse_narrowed_assignment_source(db, &narrowed_source_type, &expr_type) {
                        (narrowed_source_type, true)
                    } else {
                        (
                            get_type_at_flow_internal(
                                db,
                                tree,
                                cache,
                                root,
                                var_ref_id,
                                antecedent_flow_id,
                                false,
                            )?,
                            false,
                        )
                    }
                }
            };

        let narrowed = if source_type == LuaType::Nil {
            None
        } else {
            let declared =
                get_var_ref_type(db, cache, var_ref_id)
                    .ok()
                    .and_then(|decl| match decl {
                        LuaType::Def(_) | LuaType::Ref(_) => Some(decl),
                        _ => None,
                    });

            narrow_down_type(db, source_type.clone(), expr_type.clone(), declared)
        };

        let result_type = if reuse_source_narrowing || preserves_assignment_expr_type(&expr_type) {
            narrowed.unwrap_or_else(|| explicit_var_type.unwrap_or_else(|| expr_type.clone()))
        } else {
            expr_type
        };

        return Ok(ResultTypeOrContinue::Result(result_type));
    }

    Ok(ResultTypeOrContinue::Continue)
}

fn try_infer_decl_initializer_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
) -> Result<Option<LuaType>, InferFailReason> {
    let Some(decl_id) = var_ref_id.get_decl_id_ref() else {
        return Ok(None);
    };

    let decl = db
        .get_decl_index()
        .get_decl(&decl_id)
        .ok_or(InferFailReason::None)?;

    let Some(value_syntax_id) = decl.get_value_syntax_id() else {
        return Ok(None);
    };

    let Some(node) = value_syntax_id.to_node_from_root(root.syntax()) else {
        return Ok(None);
    };

    let Some(expr) = LuaExpr::cast(node) else {
        return Ok(None);
    };

    let expr_type = infer_expr(db, cache, expr.clone())?;
    let init_type = expr_type.get_result_slot_type(0);

    Ok(init_type)
}

/// If `var_type` is a class type, the declaration's initializer is a `TableConst`, and
/// at least one provided field is optional in the class, returns the Instance-narrowed
/// type. Otherwise returns `None`.
///
/// The optional-field guard prevents wrapping non-optional class declarations in
/// Instance (which would intersect field types with literal constants, narrowing
/// `integer` to `IntegerConst(1)` undesirably for initial declarations).
fn try_narrow_decl_to_instance(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    var_type: &LuaType,
) -> Option<LuaType> {
    if !var_type.is_class_type(db) {
        return None;
    }
    let init_type = try_infer_decl_initializer_type(db, cache, root, var_ref_id).ok()??;
    let LuaType::TableConst(ref range) = init_type else {
        return None;
    };
    let literal_owner = LuaMemberOwner::Element(range.clone());
    // Only create Instance when at least one provided literal field corresponds
    // to an optional class field — otherwise narrowing brings no benefit.
    if !literal_provides_optional_class_field(db, var_type, &literal_owner) {
        return None;
    }
    narrow_down_type(db, var_type.clone(), init_type, Some(var_type.clone()))
}
