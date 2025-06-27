use emmylua_parser::{LuaAstNode, LuaExpr, LuaNameExpr};

use crate::{
    semantic::infer::InferResult, CacheEntry, CacheKey, DbIndex, LuaDeclId, LuaInferCache, LuaType,
};

pub fn infer_name_expr_narrow_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name_expr: LuaNameExpr,
    decl_id: LuaDeclId,
    decl_type: LuaType,
) -> InferResult {
    let file_id = cache.get_file_id();
    let Some(flow_tree) = db.get_flow_index().get_flow_tree(&file_id) else {
        return Ok(decl_type);
    };

    let Some(flow_id) = flow_tree.get_flow_id(name_expr.get_syntax_id()) else {
        return Ok(decl_type);
    };

    let key = CacheKey::FlowNode(decl_id, flow_id);
    if let Some(cache_entry) = cache.get(&key) {
        if let CacheEntry::ExprCache(narrow_type) = cache_entry {
            return Ok(narrow_type.clone());
        }
    }

    let mut narrow_tyoe = decl_type.clone();

    // loop {
    //     if let Some(flow_node) = flow_tree.get_flow_node(flow_id) {

    //     } else {
    //         break; // No more antecedents
    //     }
    // }

    let value = CacheEntry::ExprCache(narrow_tyoe.clone());
    cache.add_cache(&key, value);

    Ok(narrow_tyoe)
}
