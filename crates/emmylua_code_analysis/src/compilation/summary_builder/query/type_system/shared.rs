use crate::{
    SalsaDeclId, SalsaDeclKindSummary, SalsaDeclSummary, SalsaDeclTreeSummary, SalsaScopeSummary,
};
use emmylua_parser::{LuaAstNode, LuaBlock, LuaChunk, LuaNameExpr};
use rowan::TextSize;

pub(crate) fn name_expr_targets_decl(
    decl_tree: &SalsaDeclTreeSummary,
    name_expr: &LuaNameExpr,
    decl_id: SalsaDeclId,
) -> bool {
    name_expr_decl_id(decl_tree, name_expr)
        .is_some_and(|resolved_decl_id| resolved_decl_id == decl_id)
}

pub(crate) fn name_expr_decl_id(
    decl_tree: &SalsaDeclTreeSummary,
    name_expr: &LuaNameExpr,
) -> Option<SalsaDeclId> {
    let name = name_expr.get_name_text()?;
    find_visible_decl_before_offset(decl_tree, &name, name_expr.get_position()).map(|decl| decl.id)
}

pub(crate) fn innermost_block_at_offset(chunk: &LuaChunk, offset: TextSize) -> Option<LuaBlock> {
    chunk
        .descendants::<LuaBlock>()
        .filter(|block| block.get_range().contains(offset))
        .min_by_key(|block| block.get_range().len())
}

pub(crate) fn nearest_ancestor_block<T: LuaAstNode>(node: &T) -> Option<LuaBlock> {
    node.syntax().ancestors().skip(1).find_map(LuaBlock::cast)
}

pub(crate) fn find_visible_decl_before_offset<'a>(
    decl_tree: &'a SalsaDeclTreeSummary,
    name: &str,
    offset: TextSize,
) -> Option<&'a SalsaDeclSummary> {
    let scope_id = innermost_scope_id_at_offset(decl_tree, offset)?;
    decl_tree.decls.iter().rev().find(|decl| {
        decl.name == name
            && decl.start_offset <= offset
            && is_scope_in_ancestor_chain(decl_tree, visibility_scope_id(decl_tree, decl), scope_id)
    })
}

fn innermost_scope_id_at_offset(decl_tree: &SalsaDeclTreeSummary, offset: TextSize) -> Option<u32> {
    decl_tree
        .scopes
        .iter()
        .filter(|scope| scope.start_offset <= offset && offset <= scope.end_offset)
        .max_by_key(|scope| scope.start_offset)
        .map(|scope| scope.id)
}

fn visibility_scope_id(decl_tree: &SalsaDeclTreeSummary, decl: &SalsaDeclSummary) -> u32 {
    match &decl.kind {
        SalsaDeclKindSummary::Local { attrib } => {
            if matches!(attrib, Some(crate::SalsaLocalAttributeSummary::IterConst)) {
                return decl.scope_id;
            }

            get_scope_by_id(decl_tree, decl.scope_id)
                .and_then(|scope| scope.parent)
                .unwrap_or(decl.scope_id)
        }
        _ => decl.scope_id,
    }
}

fn is_scope_in_ancestor_chain(
    decl_tree: &SalsaDeclTreeSummary,
    candidate_scope_id: u32,
    current_scope_id: u32,
) -> bool {
    let mut scope_id = Some(current_scope_id);
    while let Some(id) = scope_id {
        if id == candidate_scope_id {
            return true;
        }
        scope_id = get_scope_by_id(decl_tree, id).and_then(|scope| scope.parent);
    }
    false
}

fn get_scope_by_id(decl_tree: &SalsaDeclTreeSummary, scope_id: u32) -> Option<&SalsaScopeSummary> {
    decl_tree.scopes.iter().find(|scope| scope.id == scope_id)
}
