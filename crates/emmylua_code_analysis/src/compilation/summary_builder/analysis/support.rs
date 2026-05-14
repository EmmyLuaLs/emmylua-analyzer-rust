use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexExpr, LuaIndexKey, NumberResult};
use rowan::TextSize;
use smol_str::SmolStr;

use crate::{SalsaLocalAttributeSummary, SalsaScopeSummary};

use super::super::{
    SalsaDeclKindSummary, SalsaDeclSummary, SalsaDeclTreeSummary, SalsaGlobalRootSummary,
    SalsaMemberRootSummary, SalsaMemberTargetSummary,
};

pub fn find_visible_decl_before_offset<'a>(
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

pub fn extract_member_target_from_index_expr(
    decl_tree: &SalsaDeclTreeSummary,
    index_expr: &LuaIndexExpr,
) -> Option<SalsaMemberTargetSummary> {
    let (root, mut segments) =
        extract_member_root_and_segments(decl_tree, &index_expr.get_prefix_expr()?)?;
    segments.push(get_simple_member_name(index_expr)?);
    let member_name = segments.pop()?;
    Some(SalsaMemberTargetSummary {
        root,
        owner_segments: segments.into(),
        member_name,
    })
}

pub fn get_simple_member_name(index_expr: &LuaIndexExpr) -> Option<SmolStr> {
    match index_expr.get_index_key()? {
        LuaIndexKey::Name(name) => Some(name.get_name_text().into()),
        LuaIndexKey::String(string) => Some(string.get_value().into()),
        LuaIndexKey::Integer(number) => match number.get_number_value() {
            NumberResult::Int(value) => Some(SmolStr::new(value.to_string())),
            _ => None,
        },
        _ => None,
    }
}

fn extract_member_root_and_segments(
    decl_tree: &SalsaDeclTreeSummary,
    expr: &LuaExpr,
) -> Option<(SalsaMemberRootSummary, Vec<SmolStr>)> {
    match expr {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            if let Some(decl) =
                find_visible_decl_before_offset(decl_tree, &name, name_expr.get_position())
            {
                if !matches!(decl.kind, SalsaDeclKindSummary::Global) {
                    return Some((
                        SalsaMemberRootSummary::LocalDecl {
                            name: name.into(),
                            decl_id: decl.id,
                        },
                        Vec::new(),
                    ));
                }
            }

            let root = if name == "_G" || name == "_ENV" {
                SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Env)
            } else {
                SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Name(name.into()))
            };
            Some((root, Vec::new()))
        }
        LuaExpr::IndexExpr(index_expr) => {
            let (root, mut segments) =
                extract_member_root_and_segments(decl_tree, &index_expr.get_prefix_expr()?)?;
            segments.push(get_simple_member_name(index_expr)?);
            Some((root, segments))
        }
        LuaExpr::ParenExpr(paren_expr) => {
            extract_member_root_and_segments(decl_tree, &paren_expr.get_expr()?)
        }
        _ => None,
    }
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
            if matches!(attrib, Some(SalsaLocalAttributeSummary::IterConst)) {
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
