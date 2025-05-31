use emmylua_parser::LuaSyntaxNode;

use crate::{DbIndex, LuaMemberId, LuaSemanticDeclId};

use super::{
    member::find_member_origin_owner, semantic_info::infer_node_semantic_decl, LuaInferCache,
    SemanticDeclLevel,
};

pub fn is_reference_to(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    node: LuaSyntaxNode,
    semantic_decl: LuaSemanticDeclId,
    level: SemanticDeclLevel,
) -> Option<bool> {
    let node_semantic_decl_id = infer_node_semantic_decl(db, infer_config, node, level)?;
    if node_semantic_decl_id == semantic_decl {
        return Some(true);
    }

    match (node_semantic_decl_id, &semantic_decl) {
        (LuaSemanticDeclId::Member(node_member_id), LuaSemanticDeclId::Member(member_id)) => {
            if let Some(true) = is_member_reference_to(db, node_member_id, *member_id) {
                return Some(true);
            }

            is_member_origin_reference_to(db, infer_config, node_member_id, semantic_decl)
        }
        _ => Some(false),
    }
}

fn is_member_reference_to(
    db: &DbIndex,
    node_member_id: LuaMemberId,
    member_id: LuaMemberId,
) -> Option<bool> {
    let node_owner = db.get_member_index().get_current_owner(&node_member_id)?;
    let owner = db.get_member_index().get_current_owner(&member_id)?;

    Some(node_owner == owner)
}

fn is_member_origin_reference_to(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    node_member_id: LuaMemberId,
    semantic_decl: LuaSemanticDeclId,
) -> Option<bool> {
    let node_origin = find_member_origin_owner(db, infer_config, node_member_id)
        .unwrap_or(LuaSemanticDeclId::Member(node_member_id));

    match (node_origin, semantic_decl) {
        (LuaSemanticDeclId::Member(node_owner), LuaSemanticDeclId::Member(member_owner)) => {
            is_member_reference_to(db, node_owner, member_owner)
        }
        (node_origin, member_origin) => Some(node_origin == member_origin),
    }
}
