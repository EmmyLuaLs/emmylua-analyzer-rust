use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaChunk, LuaExpr, LuaVarExpr};

use crate::{
    CacheEntry, DbIndex, FlowId, FlowNodeKind, FlowTree, InferFailReason, LuaDeclId, LuaInferCache,
    LuaMemberId, LuaSignatureId, LuaType, LuaTypeOwner, TypeOps, infer_expr,
    semantic::infer::{
        InferResult, VarRefId,
        narrow::{
            ResultTypeOrContinue,
            condition_flow::{InferConditionFlow, get_type_at_condition_flow},
            get_multi_antecedents, get_single_antecedent,
            get_type_at_cast_flow::get_type_at_cast_flow,
            get_var_ref_type,
            var_ref_id::get_var_expr_var_ref_id,
        },
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
    let key = (var_ref_id.clone(), flow_id);
    if let Some(cache_entry) = cache.flow_node_cache.get(&key)
        && let CacheEntry::Cache(narrow_type) = cache_entry
    {
        return Ok(narrow_type.clone());
    }

    let result_type;
    let mut antecedent_flow_id = flow_id;
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
                antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
            }
            FlowNodeKind::BranchLabel | FlowNodeKind::NamedLabel(_) => {
                let multi_antecedents = get_multi_antecedents(tree, flow_node)?;

                let mut branch_result_type = LuaType::Unknown;
                for &flow_id in &multi_antecedents {
                    let branch_type = get_type_at_flow(db, tree, cache, root, var_ref_id, flow_id)?;
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
                            result_type = var_type;
                            break;
                        }
                        Err(err) => {
                            // 尝试推断声明位置的类型, 如果发生错误则返回初始错误, 否则返回当前推断错误
                            // Try to infer the type at the declaration site; if an error occurs,
                            // return the initial error, otherwise return the current inference
                            // error
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
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                }
            }
            FlowNodeKind::Assignment(assign_ptr) => {
                let assign_stat = assign_ptr.to_node(root).ok_or(InferFailReason::None)?;
                let result_or_continue =
                    get_type_at_assign_stat(db, cache, var_ref_id, assign_stat)?;

                if let ResultTypeOrContinue::Result(assign_type) = result_or_continue {
                    result_type = assign_type;
                    break;
                } else {
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                }
            }
            FlowNodeKind::ImplFunc(func_ptr) => {
                let func_stat = func_ptr.to_node(root).ok_or(InferFailReason::None)?;
                let Some(func_name) = func_stat.get_func_name() else {
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                    continue;
                };

                let Some(ref_id) = get_var_expr_var_ref_id(db, cache, func_name.to_expr()) else {
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
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
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                }
            }
            FlowNodeKind::TrueCondition(condition_ptr) => {
                let condition = condition_ptr.to_node(root).ok_or(InferFailReason::None)?;
                let result_or_continue = get_type_at_condition_flow(
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    flow_node,
                    condition,
                    InferConditionFlow::TrueCondition,
                )?;

                if let ResultTypeOrContinue::Result(condition_type) = result_or_continue {
                    result_type = condition_type;
                    break;
                } else {
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                }
            }
            FlowNodeKind::FalseCondition(condition_ptr) => {
                let condition = condition_ptr.to_node(root).ok_or(InferFailReason::None)?;
                let result_or_continue = get_type_at_condition_flow(
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    flow_node,
                    condition,
                    InferConditionFlow::FalseCondition,
                )?;

                if let ResultTypeOrContinue::Result(condition_type) = result_or_continue {
                    result_type = condition_type;
                    break;
                } else {
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                }
            }
            FlowNodeKind::ForIStat(_) => {
                // todo check for `for i = 1, 10 do end`
                antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
            }
            FlowNodeKind::TagCast(cast_ast_ptr) => {
                let tag_cast = cast_ast_ptr.to_node(root).ok_or(InferFailReason::None)?;
                let cast_or_continue =
                    get_type_at_cast_flow(db, tree, cache, root, var_ref_id, flow_node, tag_cast)?;

                if let ResultTypeOrContinue::Result(cast_type) = cast_or_continue {
                    result_type = cast_type;
                    break;
                } else {
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
                }
            }
        }
    }

    cache
        .flow_node_cache
        .insert(key, CacheEntry::Cache(result_type.clone()));
    Ok(result_type)
}

fn get_type_at_assign_stat(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
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

        // Check if there's an explicit type annotation (not just inferred type).
        let explicit_var_type = get_explicit_var_type(db, cache, var_ref_id, &var);

        // infer from expr
        let expr_type = match exprs.get(i) {
            Some(expr) => {
                let expr_type = infer_expr(db, cache, expr.clone())?;
                match &expr_type {
                    // If the expression type is variadic, get the first type
                    LuaType::Variadic(variadic) => match variadic.get_type(0) {
                        Some(typ) => typ.clone(),
                        None => return Ok(ResultTypeOrContinue::Continue),
                    },
                    _ => expr_type,
                }
            }
            None => {
                let expr_len = exprs.len();
                if expr_len == 0 {
                    return Ok(ResultTypeOrContinue::Continue);
                }

                let last_expr = exprs[expr_len - 1].clone();
                let last_expr_type = infer_expr(db, cache, last_expr)?;
                if let LuaType::Variadic(variadic) = last_expr_type {
                    let idx = i - expr_len + 1;
                    match variadic.get_type(idx) {
                        Some(typ) => typ.clone(),
                        None => return Ok(ResultTypeOrContinue::Continue),
                    }
                } else {
                    return Ok(ResultTypeOrContinue::Continue);
                }
            }
        };

        let result_type = if let Some(annotation) = explicit_var_type {
            if matches!(expr_type, LuaType::TableConst(_)) {
                annotation
            } else {
                expr_type
            }
        } else {
            expr_type
        };

        return Ok(ResultTypeOrContinue::Result(result_type));
    }

    Ok(ResultTypeOrContinue::Continue)
}

fn get_explicit_var_type(
    db: &DbIndex,
    cache: &LuaInferCache,
    var_ref_id: &VarRefId,
    var_expr: &LuaVarExpr,
) -> Option<LuaType> {
    var_ref_id
        .get_decl_id_ref()
        .map(LuaTypeOwner::Decl)
        .and_then(|type_owner| get_doc_type_from_owner(db, type_owner))
        .or_else(|| {
            match var_expr {
                LuaVarExpr::NameExpr(name_expr) => Some(LuaTypeOwner::Decl(LuaDeclId::new(
                    cache.get_file_id(),
                    name_expr.get_position(),
                ))),
                LuaVarExpr::IndexExpr(index_expr) => Some(LuaTypeOwner::Member(LuaMemberId::new(
                    index_expr.get_syntax_id(),
                    cache.get_file_id(),
                ))),
            }
            .and_then(|type_owner| get_doc_type_from_owner(db, type_owner))
        })
}

fn get_doc_type_from_owner(db: &DbIndex, type_owner: LuaTypeOwner) -> Option<LuaType> {
    db.get_type_index()
        .get_type_cache(&type_owner)
        .filter(|tc| tc.is_doc())
        .map(|tc| tc.as_type().clone())
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
    let init_type = match expr_type {
        LuaType::Variadic(variadic) => variadic.get_type(0).cloned(),
        ty => Some(ty),
    };

    Ok(init_type)
}
