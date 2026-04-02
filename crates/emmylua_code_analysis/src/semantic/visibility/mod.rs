#[cfg(test)]
mod test;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaFuncStat, LuaGeneralToken, LuaIndexExpr,
    LuaSyntaxToken, LuaVarExpr, VisibilityKind,
};

use crate::{
    DbIndex, Emmyrc, FileId, LuaCommonProperty, LuaMemberOwner, LuaSemanticDeclId, LuaType,
    LuaTypeDeclId, LuaTypeIdentifier, ModuleInfo, SemanticModel, db_index::WorkspaceId,
    try_extract_signature_id_from_field,
};

use super::{LuaInferCache, infer_expr, type_check::is_sub_type_of};

pub fn is_type_decl_visible(
    db: &DbIndex,
    file_id: FileId,
    decl_id: &LuaTypeDeclId,
) -> Option<bool> {
    if let Some(local_file_id) = match decl_id.get_id() {
        LuaTypeIdentifier::Global(_) => None,
        LuaTypeIdentifier::Local(file_id, _) => Some(*file_id),
    } {
        return Some(local_file_id == file_id);
    }

    let type_decl = db.get_type_index().get_type_decl(decl_id)?;
    let current_workspace_id = db
        .get_module_index()
        .get_workspace_id(file_id)
        .or_else(|| {
            if db.get_vfs().is_remote_file(&file_id) {
                Some(WorkspaceId::REMOTE)
            } else {
                None
            }
        })?;
    Some(type_decl.is_visible_from(current_workspace_id))
}

pub fn check_visibility(
    db: &DbIndex,
    file_id: FileId,
    emmyrc: &Emmyrc,
    infer_config: &mut LuaInferCache,
    token: LuaSyntaxToken,
    property_owner: LuaSemanticDeclId,
) -> Option<bool> {
    let property = match get_property(db, &property_owner) {
        Some(property) => property,
        None => {
            return check_member_name(db, file_id, emmyrc, infer_config, token, property_owner);
        }
    };

    if let Some(version_conds) = property.version_conds() {
        let version_number = emmyrc.runtime.version.to_lua_version_number();
        let visible = version_conds.iter().any(|cond| cond.check(&version_number));
        if !visible {
            return Some(false);
        }
    }

    match property.visibility {
        VisibilityKind::Public => return Some(true),
        VisibilityKind::Protected | VisibilityKind::Private => {
            return Some(
                check_visibility_by_visibility(
                    db,
                    infer_config,
                    file_id,
                    property_owner.clone(),
                    token,
                    property.visibility,
                )
                .unwrap_or(false),
            );
        }
        VisibilityKind::Package => {
            return Some(file_id == property_owner.get_file_id()?);
        }
        VisibilityKind::Internal => {
            let property_file_id = property_owner.get_file_id()?;
            let property_workspace_id = db.get_module_index().get_workspace_id(property_file_id)?;
            let current_workspace_id = db.get_module_index().get_workspace_id(file_id)?;
            if current_workspace_id != property_workspace_id {
                return Some(false);
            }
        }
    }

    check_member_name(db, file_id, emmyrc, infer_config, token, property_owner)
}

fn check_visibility_by_visibility(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    file_id: FileId,
    property_owner: LuaSemanticDeclId,
    token: LuaSyntaxToken,
    visibility: VisibilityKind,
) -> Option<bool> {
    let member_owner = match property_owner {
        LuaSemanticDeclId::Member(member_id) => {
            db.get_member_index().get_current_owner(&member_id)?
        }
        _ => return Some(true),
    };

    let token = LuaGeneralToken::cast(token)?;
    if check_def_visibility(
        db,
        infer_config,
        file_id,
        member_owner,
        token.clone(),
        visibility,
    )
    .unwrap_or(false)
    {
        return Some(true);
    }

    let blocks = token.ancestors::<LuaBlock>();
    for block in blocks {
        if check_block_visibility(db, infer_config, member_owner, block, visibility)
            .unwrap_or(false)
        {
            return Some(true);
        }
    }

    Some(false)
}

fn check_block_visibility(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    member_owner: &LuaMemberOwner,
    block: LuaBlock,
    visibility: VisibilityKind,
) -> Option<bool> {
    let func_stat = block
        .get_parent::<LuaClosureExpr>()?
        .get_parent::<LuaFuncStat>()?;

    let func_name = func_stat.get_func_name()?;
    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
        let prefix_expr = index_expr.get_prefix_expr()?;
        let typ = infer_expr(db, infer_config, prefix_expr).ok()?;
        if visibility == VisibilityKind::Protected {
            if let (LuaType::Def(left), LuaMemberOwner::Type(right)) = (typ, member_owner) {
                if left == *right {
                    return Some(true);
                }

                if is_sub_type_of(db, &left, right) {
                    return Some(true);
                }
            }
        } else if visibility == VisibilityKind::Private {
            match (typ, member_owner) {
                (LuaType::Def(left), LuaMemberOwner::Type(right)) => {
                    return Some(left == *right);
                }
                (LuaType::TableConst(left), LuaMemberOwner::Element(right)) => {
                    return Some(left == *right);
                }
                _ => {}
            }
        }
    }

    Some(false)
}

fn check_def_visibility(
    db: &DbIndex,
    infer_config: &mut LuaInferCache,
    _file_id: FileId,
    member_owner: &LuaMemberOwner,
    token: LuaGeneralToken,
    visibility: VisibilityKind,
) -> Option<bool> {
    let index_expr = token.get_parent::<LuaIndexExpr>()?;
    let prefix_expr = index_expr.get_prefix_expr()?;
    let typ = infer_expr(db, infer_config, prefix_expr).ok()?;

    // 这是为解决 require 后仍然是`Def`类型的问题, 但现在不需要了, 不过还是留着以防万一
    // if !in_def_file(db, &typ, file_id) {
    //     return Some(false);
    // }

    match visibility {
        VisibilityKind::Protected => match (typ, member_owner) {
            (LuaType::Def(left), LuaMemberOwner::Type(right)) => {
                Some(left == *right || is_sub_type_of(db, &left, right))
            }
            _ => Some(false),
        },
        VisibilityKind::Private => match (typ, member_owner) {
            (LuaType::Def(left), LuaMemberOwner::Type(right)) => Some(left == *right),
            (LuaType::TableConst(left), LuaMemberOwner::Element(right)) => Some(left == *right),
            _ => Some(false),
        },
        _ => None,
    }
}

// fn in_def_file(db: &DbIndex, typ: &LuaType, file_id: FileId) -> bool {
//     match typ {
//         LuaType::Def(id) => {
//             let decl = db.get_type_index().get_type_decl(id);
//             if let Some(decl) = decl {
//                 decl.get_locations()
//                     .iter()
//                     .any(|location| location.file_id == file_id)
//             } else {
//                 false
//             }
//         }
//         LuaType::TableConst(in_file) => in_file.file_id == file_id,
//         _ => false,
//     }
// }

fn get_property<'a>(
    db: &'a DbIndex,
    property_owner: &'a LuaSemanticDeclId,
) -> Option<&'a LuaCommonProperty> {
    match db.get_property_index().get_property(property_owner) {
        Some(common_property) => Some(common_property),
        None => {
            let LuaSemanticDeclId::Member(member_id) = property_owner else {
                return None;
            };
            let member = db.get_member_index().get_member(member_id)?;
            let signature_id = try_extract_signature_id_from_field(db, member)?;
            db.get_property_index()
                .get_property(&LuaSemanticDeclId::Signature(signature_id))
        }
    }
}

fn check_member_name(
    db: &DbIndex,
    file_id: FileId,
    emmyrc: &Emmyrc,
    infer_config: &mut LuaInferCache,
    token: LuaSyntaxToken,
    property_owner: LuaSemanticDeclId,
) -> Option<bool> {
    if let LuaSemanticDeclId::Member(member_id) = property_owner
        && let Some(member) = db.get_member_index().get_member(&member_id)
        && let Some(name) = member.get_key().get_name()
    {
        let config = emmyrc;
        for pattern in &config.doc.private_name {
            let is_match = if let Some(prefix) = pattern.strip_suffix('*') {
                name.starts_with(prefix)
            } else if let Some(suffix) = pattern.strip_prefix('*') {
                name.ends_with(suffix)
            } else {
                name == pattern
            };
            if is_match {
                return Some(
                    check_visibility_by_visibility(
                        db,
                        infer_config,
                        file_id,
                        property_owner,
                        token,
                        VisibilityKind::Private,
                    )
                    .unwrap_or(false),
                );
            }
        }
    };
    Some(true)
}

pub fn check_module_visibility(
    semantic_model: &SemanticModel,
    module_info: &ModuleInfo,
) -> Option<bool> {
    let current_workspace_id = semantic_model
        .get_db()
        .get_module_index()
        .get_workspace_id(semantic_model.get_file_id())?;
    Some(module_info.is_requireable_from(current_workspace_id))
}
