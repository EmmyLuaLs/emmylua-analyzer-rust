mod find_index;
mod find_members;
mod get_member_map;
mod infer_raw_member;

use std::collections::HashSet;

use crate::{
    db_index::{LuaType, LuaTypeDeclId},
    DbIndex, LuaMemberFeature, LuaMemberId, LuaMemberKey, LuaSemanticDeclId,
};
use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaSyntaxKind, LuaTableExpr, LuaTableField};
pub use find_index::find_index_operations;
pub use find_members::find_members;
pub use get_member_map::get_member_map;
pub use infer_raw_member::infer_raw_member_type;

use super::{
    infer_node_semantic_decl, infer_table_should_be, InferFailReason, LuaInferCache,
    SemanticDeclLevel,
};

pub fn get_buildin_type_map_type_id(type_: &LuaType) -> Option<LuaTypeDeclId> {
    match type_ {
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            Some(LuaTypeDeclId::new("string"))
        }
        LuaType::Io => Some(LuaTypeDeclId::new("io")),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LuaMemberInfo {
    pub property_owner_id: Option<LuaSemanticDeclId>,
    pub key: LuaMemberKey,
    pub typ: LuaType,
    pub feature: Option<LuaMemberFeature>,
    pub overload_index: Option<usize>,
}

type FindMembersResult = Option<Vec<LuaMemberInfo>>;
type RawGetMemberTypeResult = Result<LuaType, InferFailReason>;

pub fn find_member_origin_owner(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    member_id: LuaMemberId,
) -> Option<LuaSemanticDeclId> {
    const MAX_ITERATIONS: usize = 50;
    let mut visited_members = HashSet::new();

    let mut current_owner = resolve_member_owner(db, infer_config, &member_id);
    let mut final_owner = current_owner.clone();
    let mut iteration_count = 0;

    while let Some(LuaSemanticDeclId::Member(current_member_id)) = &current_owner {
        if visited_members.contains(current_member_id) || iteration_count >= MAX_ITERATIONS {
            break;
        }

        visited_members.insert(current_member_id.clone());
        iteration_count += 1;

        match resolve_member_owner(db, infer_config, current_member_id) {
            Some(next_owner) => {
                final_owner = Some(next_owner.clone());
                current_owner = Some(next_owner);
            }
            None => break,
        }
    }

    final_owner
}

fn resolve_member_owner(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    member_id: &LuaMemberId,
) -> Option<LuaSemanticDeclId> {
    let root = db
        .get_vfs()
        .get_syntax_tree(&member_id.file_id)?
        .get_red_root();
    let current_node = member_id.get_syntax_id().to_node_from_root(&root)?;
    match member_id.get_syntax_id().get_kind() {
        LuaSyntaxKind::TableFieldAssign => {
            if LuaTableField::can_cast(current_node.kind().into()) {
                let table_field = LuaTableField::cast(current_node.clone())?;
                // 如果表是类, 那么通过类型推断获取 owner
                if let Some(owner_id) =
                    resolve_table_field_through_type_inference(db, infer_config, &table_field)
                {
                    return Some(owner_id);
                }
                // 非类, 那么通过右值推断
                let value_expr = table_field.get_value_expr()?;
                let value_node = value_expr.get_syntax_id().to_node_from_root(&root)?;
                infer_node_semantic_decl(db, infer_config, value_node, SemanticDeclLevel::default())
            } else {
                None
            }
        }
        LuaSyntaxKind::IndexExpr => {
            let assign_node = current_node.parent()?;
            let assign_stat = LuaAssignStat::cast(assign_node)?;
            let (vars, exprs) = assign_stat.get_var_and_expr_list();

            for (var, expr) in vars.iter().zip(exprs.iter()) {
                if var.syntax().text_range() == current_node.text_range() {
                    let expr_node = expr.get_syntax_id().to_node_from_root(&root)?;
                    return infer_node_semantic_decl(
                        db,
                        infer_config,
                        expr_node,
                        SemanticDeclLevel::default(),
                    );
                }
            }
            None
        }
        _ => None,
    }
}

fn resolve_table_field_through_type_inference(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    table_field: &LuaTableField,
) -> Option<LuaSemanticDeclId> {
    let parent = table_field.syntax().parent()?;
    let table_expr = LuaTableExpr::cast(parent)?;
    let table_type = infer_table_should_be(db, infer_config, table_expr).ok()?;

    if !matches!(table_type, LuaType::Ref(_) | LuaType::Def(_)) {
        return None;
    }

    let field_key = table_field.get_field_key()?;
    let key = LuaMemberKey::from_index_key(db, infer_config, &field_key).ok()?;
    let member_infos = find_members(db, &table_type)?;

    member_infos
        .iter()
        .find(|m| m.key == key)?
        .property_owner_id
        .clone()
}
