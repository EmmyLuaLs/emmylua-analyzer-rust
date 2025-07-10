use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaChunk, LuaVarExpr};

use crate::{
    infer_expr,
    semantic::infer::{
        narrow::{
            condition_flow::{get_type_at_condition_flow, InferConditionFlow},
            get_multi_antecedents, get_single_antecedent,
            get_type_at_cast_flow::get_type_at_cast_flow,
            get_var_ref_type,
            narrow_type::narrow_down_type,
            var_ref_id::get_var_expr_var_ref_id,
            ResultTypeOrContinue,
        },
        InferResult, VarRefId,
    },
    CacheEntry, DbIndex, FlowId, FlowNode, FlowNodeKind, FlowTree, InferFailReason, LuaDeclId,
    LuaInferCache, LuaMemberId, LuaType, TypeOps,
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
    if let Some(cache_entry) = cache.flow_node_cache.get(&key) {
        if let CacheEntry::Cache(narrow_type) = cache_entry {
            return Ok(narrow_type.clone());
        }
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

                // 在分支前获取原始类型
                let original_type = if let Some(antecedent) = &flow_node.antecedent {
                    match antecedent {
                        crate::FlowAntecedent::Single(single_id) => {
                            get_type_at_flow(db, tree, cache, root, var_ref_id, *single_id)?
                        }
                        crate::FlowAntecedent::Multiple(_) => {
                            // 在 BranchLabel 中，多个 antecedent 需要获取共同的祖先
                            get_var_ref_type(db, cache, var_ref_id)?
                        }
                    }
                } else {
                    get_var_ref_type(db, cache, var_ref_id)?
                };

                let mut branch_types = Vec::new();

                for &flow_id in &multi_antecedents {
                    let branch_type = get_type_at_flow(db, tree, cache, root, var_ref_id, flow_id)?;
                    branch_types.push(branch_type);
                }

                // 分析类型覆盖
                let result_type_analysis = analyze_branch_coverage(
                    &original_type,
                    &branch_types,
                    db,
                    tree,
                    cache,
                    root,
                    var_ref_id,
                    &multi_antecedents,
                )?;

                result_type = result_type_analysis;
                break;
            }
            FlowNodeKind::DeclPosition(position) => {
                if *position <= var_ref_id.get_position() {
                    result_type = get_var_ref_type(db, cache, var_ref_id)?;
                    break;
                } else {
                    antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
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
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    assign_stat: LuaAssignStat,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    for i in 0..vars.len() {
        let var = vars[i].clone();
        let Some(maybe_ref_id) = get_var_expr_var_ref_id(db, cache, var.to_expr()) else {
            continue;
        };

        if maybe_ref_id != *var_ref_id {
            // let typ = get_var_ref_type(db, cache, var_ref_id)?;
            continue;
        }

        // maybe use type force type
        let var_type = match var {
            LuaVarExpr::NameExpr(name_expr) => {
                let decl_id = LuaDeclId::new(cache.get_file_id(), name_expr.get_position());
                let type_cache = db.get_type_index().get_type_cache(&decl_id.into());
                if let Some(typ_cache) = type_cache {
                    Some(typ_cache.as_type().clone())
                } else {
                    None
                }
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                let member_id = LuaMemberId::new(index_expr.get_syntax_id(), cache.get_file_id());
                let type_cache = db.get_type_index().get_type_cache(&member_id.into());
                if let Some(typ_cache) = type_cache {
                    Some(typ_cache.as_type().clone())
                } else {
                    None
                }
            }
        };

        if let Some(var_type) = var_type {
            return Ok(ResultTypeOrContinue::Result(var_type));
        }

        // infer from expr
        let expr_type = match exprs.get(i) {
            Some(expr) => {
                let expr_type = infer_expr(db, cache, expr.clone())?;
                match &expr_type {
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

        let antecedent_flow_id = get_single_antecedent(tree, flow_node)?;
        let antecedent_type =
            get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;

        return Ok(ResultTypeOrContinue::Result(
            narrow_down_type(db, antecedent_type, expr_type.clone()).unwrap_or(expr_type),
        ));
    }

    Ok(ResultTypeOrContinue::Continue)
}

// 分析分支覆盖率, 确定原始类型中哪些部分被赋值覆盖
fn analyze_branch_coverage(
    original_type: &LuaType,
    branch_types: &[LuaType],
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_ids: &[FlowId],
) -> Result<LuaType, InferFailReason> {
    if branch_types.is_empty() {
        return Ok(original_type.clone());
    }

    // 检查哪些分支实际上有赋值
    let mut assignment_branches = Vec::new();
    let mut non_assignment_branches = Vec::new();

    for (i, &flow_id) in flow_ids.iter().enumerate() {
        let branch_type = &branch_types[i];

        // 检查这个分支是否有赋值, 通过检查类型是否显著变化
        let has_assignment = !types_are_equivalent(branch_type, original_type)
            && has_assignment_in_branch(db, tree, cache, root, var_ref_id, flow_id)?;

        if has_assignment {
            assignment_branches.push(branch_type.clone());
        } else {
            non_assignment_branches.push(branch_type.clone());
        }
    }

    if !assignment_branches.is_empty() {
        // 检查所有赋值分支是否都是相同的类型
        let first_assignment_type = &assignment_branches[0];
        let all_assignments_same = assignment_branches
            .iter()
            .all(|t| types_are_equivalent(t, first_assignment_type));

        if all_assignments_same && assignment_branches.len() >= 2 {
            // 多个分支具有相同的赋值类型, 表明完全覆盖
            return Ok(first_assignment_type.clone());
        }
    }

    // 回退到原始行为: 合并所有分支类型
    let mut result_type = LuaType::Unknown;
    let mut has_any_type = false;

    for branch_type in branch_types {
        if !has_any_type {
            result_type = branch_type.clone();
            has_any_type = true;
        } else {
            result_type = TypeOps::Union.apply(db, &result_type, branch_type);
        }
    }

    Ok(result_type)
}

// 检查分支是否包含对目标变量的赋值
fn has_assignment_in_branch(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    start_flow_id: FlowId,
) -> Result<bool, InferFailReason> {
    let mut current_flow_id = start_flow_id;

    // 遍历流向后看是否在这个分支中有赋值
    loop {
        let flow_node = tree
            .get_flow_node(current_flow_id)
            .ok_or(InferFailReason::None)?;

        match &flow_node.kind {
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

                if let ResultTypeOrContinue::Result(_) = result_or_continue {
                    return Ok(true);
                }

                // 继续检查 antecedents
                current_flow_id = get_single_antecedent(tree, flow_node)?;
            }
            FlowNodeKind::TrueCondition(_) | FlowNodeKind::FalseCondition(_) => {
                // 继续通过条件节点
                current_flow_id = get_single_antecedent(tree, flow_node)?;
            }
            FlowNodeKind::BranchLabel | FlowNodeKind::NamedLabel(_) => {
                // 到达另一个分支点, 停止这里
                return Ok(false);
            }
            FlowNodeKind::Start | FlowNodeKind::Unreachable => {
                // 到达开始没有找到赋值
                return Ok(false);
            }
            FlowNodeKind::DeclPosition(_) => {
                // 到达声明, 停止这里
                return Ok(false);
            }
            _ => {
                // 继续检查 antecedents 对于其他 flow node 类型
                current_flow_id = get_single_antecedent(tree, flow_node)?;
            }
        }
    }
}

// 检查两个类型是否等价
fn types_are_equivalent(a: &LuaType, b: &LuaType) -> bool {
    match (a, b) {
        (LuaType::Union(a_union), LuaType::Union(b_union)) => {
            let a_types = a_union.into_vec();
            let b_types = b_union.into_vec();
            a_types.len() == b_types.len() && a_types.iter().all(|t| b_types.contains(t))
        }
        _ => a == b,
    }
}
