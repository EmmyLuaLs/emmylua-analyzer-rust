mod condition_flow;
mod get_type_at_cast_flow;
mod get_type_at_flow;
mod narrow_type;
mod var_ref_id;

use crate::{
    CacheEntry, DbIndex, FlowAntecedent, FlowId, FlowNode, FlowTree, InferFailReason, InferGuard,
    LuaInferCache, infer_param,
    semantic::infer::{
        InferResult,
        infer_index::infer_member_by_member_key,
        infer_name::{find_decl_member_type, infer_global_type},
        try_infer_expr_no_flow,
    },
};
pub(in crate::semantic) use condition_flow::{ConditionFlowAction, InferConditionFlow};
use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr, LuaIndexMemberExpr};
pub use get_type_at_cast_flow::get_type_at_call_expr_inline_cast;
pub use narrow_type::{narrow_down_type, narrow_false_or_nil, remove_false_or_nil};
pub use var_ref_id::{VarRefId, get_var_expr_var_ref_id};

pub fn infer_expr_narrow_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
    var_ref_id: VarRefId,
) -> InferResult {
    let file_id = cache.get_file_id();
    let Some(flow_tree) = db.get_flow_index().get_flow_tree(&file_id) else {
        return get_var_ref_type(db, cache, &var_ref_id);
    };

    let Some(flow_id) = flow_tree.get_flow_id(expr.get_syntax_id()) else {
        return get_var_ref_type(db, cache, &var_ref_id);
    };

    let root = LuaChunk::cast(expr.get_root()).ok_or(InferFailReason::None)?;
    get_type_at_flow::get_type_at_flow(db, flow_tree, cache, &root, &var_ref_id, flow_id)
}

pub(in crate::semantic) fn infer_expr_at_antecedent_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    flow_node: &FlowNode,
    expr: LuaExpr,
) -> Result<Option<crate::LuaType>, InferFailReason> {
    let antecedent_flow_id = get_single_antecedent(flow_node)?;
    infer_expr_at_flow(db, tree, cache, root, antecedent_flow_id, expr)
}

pub(in crate::semantic) fn infer_expr_at_expr_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    expr: LuaExpr,
) -> Result<Option<crate::LuaType>, InferFailReason> {
    let Some(flow_id) = tree.get_flow_id(expr.get_syntax_id()) else {
        return try_infer_expr_no_flow(db, cache, expr);
    };
    infer_expr_at_flow(db, tree, cache, root, flow_id, expr)
}

fn infer_expr_at_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    flow_id: FlowId,
    mut expr: LuaExpr,
) -> Result<Option<crate::LuaType>, InferFailReason> {
    loop {
        match &expr {
            LuaExpr::ParenExpr(paren_expr) => {
                expr = paren_expr.get_expr().ok_or(InferFailReason::None)?;
            }
            LuaExpr::NameExpr(_) => {
                let Some(var_ref_id) = get_var_expr_var_ref_id(db, cache, expr.clone()) else {
                    return try_infer_expr_no_flow(db, cache, expr);
                };

                // Reuse the existing flow engine for simple ref leaves. If that query
                // cannot answer, fall back to the current no-flow behavior instead of
                // copying name/index-specific logic into each condition site.
                return match get_type_at_flow::get_type_at_flow(
                    db,
                    tree,
                    cache,
                    root,
                    &var_ref_id,
                    flow_id,
                ) {
                    Ok(ty) => Ok(Some(ty)),
                    Err(
                        InferFailReason::None
                        | InferFailReason::RecursiveInfer
                        | InferFailReason::FieldNotFound,
                    ) => try_infer_expr_no_flow(db, cache, expr),
                    Err(err) => Err(err),
                };
            }
            LuaExpr::IndexExpr(index_expr) => {
                let Some(prefix_expr) = index_expr.get_prefix_expr() else {
                    return try_infer_expr_no_flow(db, cache, expr);
                };
                let Some(prefix_type) =
                    infer_expr_at_flow(db, tree, cache, root, flow_id, prefix_expr)?
                else {
                    return try_infer_expr_no_flow(db, cache, expr);
                };

                return match infer_member_by_member_key(
                    db,
                    cache,
                    &prefix_type,
                    LuaIndexMemberExpr::IndexExpr(index_expr.clone()),
                    &InferGuard::new(),
                ) {
                    Ok(ty) => Ok(Some(ty)),
                    Err(
                        InferFailReason::None
                        | InferFailReason::RecursiveInfer
                        | InferFailReason::FieldNotFound,
                    ) => try_infer_expr_no_flow(db, cache, expr),
                    Err(err) => Err(err),
                };
            }
            _ => return try_infer_expr_no_flow(db, cache, expr),
        }
    }
}

pub(in crate::semantic) fn get_var_ref_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
) -> InferResult {
    if let Some(decl_id) = var_ref_id.get_decl_id_ref() {
        let decl = db
            .get_decl_index()
            .get_decl(&decl_id)
            .ok_or(InferFailReason::None)?;

        if decl.is_global() {
            let name = decl.get_name();
            return infer_global_type(db, name);
        }

        if let Some(type_cache) = db.get_type_index().get_type_cache(&decl.get_id().into()) {
            // 不要在此阶段展开泛型别名, 必须让后续的泛型匹配阶段基于声明形态完成推断
            return Ok(type_cache.as_type().clone());
        }

        if decl.is_param() {
            return infer_param(db, decl);
        }

        Err(InferFailReason::UnResolveDeclType(decl.get_id()))
    } else if let Some(member_id) = var_ref_id.get_member_id_ref() {
        find_decl_member_type(db, member_id)
    } else {
        if let Some(type_cache) = cache.index_ref_origin_type_cache.get(var_ref_id)
            && let CacheEntry::Cache(ty) = type_cache
        {
            return Ok(ty.clone());
        }

        Err(InferFailReason::None)
    }
}

fn get_single_antecedent(flow: &FlowNode) -> Result<FlowId, InferFailReason> {
    match &flow.antecedent {
        Some(antecedent) => match antecedent {
            FlowAntecedent::Single(id) => Ok(*id),
            FlowAntecedent::Multiple(_) => Err(InferFailReason::None),
        },
        None => Err(InferFailReason::None),
    }
}

fn get_multi_antecedents(tree: &FlowTree, flow: &FlowNode) -> Result<Vec<FlowId>, InferFailReason> {
    match &flow.antecedent {
        Some(antecedent) => match antecedent {
            FlowAntecedent::Single(id) => Ok(vec![*id]),
            FlowAntecedent::Multiple(multi_id) => {
                let multi_flow = tree
                    .get_multi_antecedents(*multi_id)
                    .ok_or(InferFailReason::None)?;
                Ok(multi_flow.to_vec())
            }
        },
        None => Err(InferFailReason::None),
    }
}
