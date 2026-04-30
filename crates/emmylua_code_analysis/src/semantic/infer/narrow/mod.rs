mod condition_flow;
mod get_type_at_cast_flow;
mod get_type_at_flow;
mod narrow_type;
mod var_ref_id;

use crate::{
    CacheEntry, DbIndex, FlowAntecedent, FlowId, FlowNode, FlowTree, InferFailReason,
    LuaInferCache, infer_param,
    semantic::infer::{
        InferResult,
        infer_name::{find_decl_member_type, infer_global_type},
    },
};
pub(in crate::semantic) use condition_flow::{ConditionFlowAction, InferConditionFlow};
use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr};
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

#[cfg(test)]
mod tests {
    use crate::{CacheEntry, LuaType, VirtualWorkspace};
    use emmylua_parser::{LuaAstNode, LuaTableExpr};

    use super::*;

    #[test]
    fn test_replay_overlay_is_scoped_without_cache_seed() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def("local value = {}");
        let syntax_id = ws.get_node::<LuaTableExpr>(file_id).get_syntax_id();
        let mut cache = LuaInferCache::new(file_id, Default::default());

        cache.with_replay_overlay(&[(syntax_id, LuaType::Table)], &[syntax_id], |cache| {
            assert_eq!(cache.replay_expr_type(syntax_id), Some(&LuaType::Table));
            assert!(cache.no_flow_table_exprs.contains(&syntax_id));
            assert!(!cache.expr_no_flow_cache.contains_key(&syntax_id));
            cache
                .expr_no_flow_cache
                .insert(syntax_id, CacheEntry::Cache(Some(LuaType::Table)));
        });

        assert!(cache.replay_expr_type(syntax_id).is_none());
        assert!(!cache.no_flow_table_exprs.contains(&syntax_id));
        assert!(!cache.expr_no_flow_cache.contains_key(&syntax_id));
    }
}
