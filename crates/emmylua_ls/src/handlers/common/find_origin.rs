use emmylua_code_analysis::{
    LuaDeclExtra, LuaDeclId, LuaMemberId, LuaSemanticDeclId, LuaType, SemanticDeclLevel,
    SemanticModel,
};

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
                    let typ = semantic_model.get_type((*member_id).into());
                    Some((decl.clone(), typ))
                }
                LuaSemanticDeclId::LuaDecl(decl_id) => {
                    let db = semantic_model.get_db();
                    let decl_info = db.get_decl_index().get_decl(decl_id)?;
                    let typ = if let LuaDeclExtra::Param {
                        idx, signature_id, ..
                    } = &decl_info.extra
                    {
                        db.get_signature_index()
                            .get(signature_id)?
                            .get_param_info_by_id(*idx)?
                            .type_ref
                            .clone()
                    } else {
                        semantic_model.get_type((*decl_id).into())
                    };
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
                find_member_origin_owners(semantic_model, member_id, true)
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
    find_all: bool,
) -> DeclOriginResult {
    let final_owner = semantic_model
        .get_member_origin_owner(member_id)
        .and_then(|origin| reject_param_origin(semantic_model, origin))
        .unwrap_or_else(|| LuaSemanticDeclId::Member(member_id));

    if !find_all {
        return DeclOriginResult::Single(final_owner);
    }

    // 如果存在多个同名成员, 则返回多个成员
    let final_owner_result = Some(final_owner.clone());
    if let Some(same_named_members) =
        find_all_same_named_members(semantic_model, &final_owner_result)
        && same_named_members.len() > 1
    {
        return DeclOriginResult::Multiple(same_named_members);
    }
    // 否则返回单个成员
    DeclOriginResult::Single(final_owner)
}

pub fn find_member_origin_owner(
    semantic_model: &SemanticModel,
    member_id: LuaMemberId,
) -> Option<LuaSemanticDeclId> {
    find_member_origin_owners(semantic_model, member_id, false).get_first()
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
        .get_member(member_id)?;

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

fn reject_param_origin(
    semantic_model: &SemanticModel,
    result: LuaSemanticDeclId,
) -> Option<LuaSemanticDeclId> {
    match &result {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let decl = semantic_model.get_db().get_decl_index().get_decl(decl_id)?;
            if decl.is_param() {
                return None;
            }
            Some(result)
        }
        _ => Some(result),
    }
}
