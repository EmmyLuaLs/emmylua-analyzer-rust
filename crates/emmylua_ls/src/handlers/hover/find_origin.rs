use std::collections::HashSet;

use emmylua_code_analysis::{
    LuaDeclId, LuaMemberId, LuaSemanticDeclId, LuaType, SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaSyntaxKind, LuaTableExpr, LuaTableField};

#[derive(Debug, Clone)]
pub enum DeclOriginResult {
    Single(LuaSemanticDeclId),
    Multiple(Vec<LuaSemanticDeclId>),
}

impl DeclOriginResult {
    pub fn get_first(&self) -> Option<LuaSemanticDeclId> {
        match self {
            DeclOriginResult::Single(decl) => Some(decl.clone()),
            DeclOriginResult::Multiple(decls) => decls.first().cloned(),
        }
    }

    pub fn get_types(&self, semantic_model: &SemanticModel) -> Vec<(LuaSemanticDeclId, LuaType)> {
        let get_type = |decl: &LuaSemanticDeclId| -> Option<(LuaSemanticDeclId, LuaType)> {
            match decl {
                LuaSemanticDeclId::Member(member_id) => {
                    let typ = semantic_model.get_type(member_id.clone().into());
                    Some((decl.clone(), typ))
                }
                LuaSemanticDeclId::LuaDecl(decl_id) => {
                    let typ = semantic_model.get_type(decl_id.clone().into());
                    Some((decl.clone(), typ))
                }
                _ => None,
            }
        };

        match self {
            DeclOriginResult::Single(decl) => get_type(decl).into_iter().collect(),
            DeclOriginResult::Multiple(decls) => decls.iter().filter_map(get_type).collect(),
        }
    }
}

pub fn find_decl_origin_owners(
    semantic_model: &SemanticModel,
    decl_id: LuaDeclId,
) -> DeclOriginResult {
    let node = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&decl_id.file_id)
        .and_then(|tree| {
            let root = tree.get_red_root();
            semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)
                .and_then(|decl| decl.get_value_syntax_id())
                .and_then(|syntax_id| syntax_id.to_node_from_root(&root))
        });

    if let Some(node) = node {
        let semantic_decl = semantic_model.find_decl(node.into(), SemanticDeclLevel::default());
        match semantic_decl {
            Some(LuaSemanticDeclId::Member(member_id)) => {
                find_member_origin_owners(semantic_model, member_id)
            }
            Some(LuaSemanticDeclId::LuaDecl(decl_id)) => {
                DeclOriginResult::Single(LuaSemanticDeclId::LuaDecl(decl_id))
            }
            _ => DeclOriginResult::Single(LuaSemanticDeclId::LuaDecl(decl_id)),
        }
    } else {
        DeclOriginResult::Single(LuaSemanticDeclId::LuaDecl(decl_id))
    }
}

pub fn find_member_origin_owners(
    semantic_model: &SemanticModel,
    member_id: LuaMemberId,
) -> DeclOriginResult {
    const MAX_ITERATIONS: usize = 50;
    let mut visited_members = HashSet::new();

    let mut current_owner = resolve_member_owner(semantic_model, &member_id);
    let mut final_owner = current_owner.clone();
    let mut iteration_count = 0;

    while let Some(LuaSemanticDeclId::Member(current_member_id)) = &current_owner {
        if visited_members.contains(current_member_id) || iteration_count >= MAX_ITERATIONS {
            break;
        }

        visited_members.insert(current_member_id.clone());
        iteration_count += 1;

        match resolve_member_owner(semantic_model, current_member_id) {
            Some(next_owner) => {
                final_owner = Some(next_owner.clone());
                current_owner = Some(next_owner);
            }
            None => break,
        }
    }

    if final_owner.is_none() {
        final_owner = Some(LuaSemanticDeclId::Member(member_id));
    }

    // 如果存在多个同名成员, 则返回多个成员
    if let Some(same_named_members) = find_all_same_named_members(semantic_model, &final_owner) {
        if same_named_members.len() > 1 {
            return DeclOriginResult::Multiple(same_named_members);
        }
    }
    // 否则返回单个成员
    DeclOriginResult::Single(final_owner.unwrap_or_else(|| LuaSemanticDeclId::Member(member_id)))
}

pub fn find_member_origin_owner(
    semantic_model: &SemanticModel,
    member_id: LuaMemberId,
) -> Option<LuaSemanticDeclId> {
    find_member_origin_owners(semantic_model, member_id).get_first()
}

pub fn find_all_same_named_members(
    semantic_model: &SemanticModel,
    final_owner: &Option<LuaSemanticDeclId>,
) -> Option<Vec<LuaSemanticDeclId>> {
    let final_owner = final_owner.as_ref()?;
    let member_id = match final_owner {
        LuaSemanticDeclId::Member(id) => id,
        _ => return None,
    };

    let original_member = semantic_model
        .get_db()
        .get_member_index()
        .get_member(&member_id)?;

    let target_key = original_member.get_key();
    let current_owner = semantic_model
        .get_db()
        .get_member_index()
        .get_current_owner(member_id)?;

    let all_members = semantic_model
        .get_db()
        .get_member_index()
        .get_members(current_owner)?;
    let same_named: Vec<LuaSemanticDeclId> = all_members
        .iter()
        .filter(|member| member.get_key() == target_key)
        .map(|member| LuaSemanticDeclId::Member(member.get_id()))
        .collect();

    if same_named.is_empty() {
        None
    } else {
        Some(same_named)
    }
}

fn resolve_member_owner(
    semantic_model: &SemanticModel,
    member_id: &LuaMemberId,
) -> Option<LuaSemanticDeclId> {
    let root = semantic_model
        .get_db()
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
                    resolve_table_field_through_type_inference(semantic_model, &table_field)
                {
                    return Some(owner_id);
                }
                // 非类, 那么通过右值推断
                let value_expr = table_field.get_value_expr()?;
                let value_node = value_expr.get_syntax_id().to_node_from_root(&root)?;
                semantic_model.find_decl(
                    value_node.into(),
                    emmylua_code_analysis::SemanticDeclLevel::default(),
                )
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
                    return semantic_model.find_decl(
                        expr_node.into(),
                        emmylua_code_analysis::SemanticDeclLevel::default(),
                    );
                }
            }
            None
        }
        _ => None,
    }
}

fn resolve_table_field_through_type_inference(
    semantic_model: &SemanticModel,
    table_field: &LuaTableField,
) -> Option<LuaSemanticDeclId> {
    let parent = table_field.syntax().parent()?;
    let table_expr = LuaTableExpr::cast(parent)?;
    let table_type = semantic_model.infer_table_should_be(table_expr)?;

    if !matches!(
        table_type,
        emmylua_code_analysis::LuaType::Ref(_) | emmylua_code_analysis::LuaType::Def(_)
    ) {
        return None;
    }

    let field_key = table_field.get_field_key()?;
    let key = semantic_model.get_member_key(&field_key)?;
    let member_infos = semantic_model.get_member_infos(&table_type)?;

    member_infos
        .iter()
        .find(|m| m.key == key)?
        .property_owner_id
        .clone()
}

pub fn replace_semantic_type(
    semantic_decls: &mut [(LuaSemanticDeclId, LuaType)],
    origin_type: &LuaType,
) {
    // `origin_type`不一定包含所有`semantic_decls`中的类型, 实际的推断可能非常复杂, 这里仅是临时方案.

    // 解开`origin_type`
    let mut type_vec = Vec::new();
    match origin_type {
        LuaType::Union(union) => {
            for typ in union.get_types() {
                type_vec.push(typ);
            }
        }
        _ => {
            type_vec.push(origin_type);
        }
    }
    if type_vec.len() != semantic_decls.len() {
        return;
    }

    // 判断是否存在泛型, 如果有任意类型不匹配我们就认为存在泛型
    let mut has_generic = false;
    let type_set: HashSet<_> = type_vec.iter().collect();
    for (_, typ) in semantic_decls.iter() {
        if !type_set.contains(&typ) {
            has_generic = true;
            break;
        }
    }
    if !has_generic {
        return;
    }

    // 替换`semantic_decls`中的类型
    for (i, (_, typ)) in semantic_decls.iter_mut().enumerate() {
        if i < type_vec.len() {
            *typ = type_vec[i].clone();
        }
    }
}
