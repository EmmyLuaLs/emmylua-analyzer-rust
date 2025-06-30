mod export;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaFuncStat, LuaGeneralToken, LuaIndexExpr,
    LuaSyntaxToken, LuaVarExpr, VisibilityKind,
};

use crate::{DbIndex, Emmyrc, FileId, LuaMemberOwner, LuaSemanticDeclId, LuaType};

use super::{infer_expr, type_check::is_sub_type_of, LuaInferCache};

pub use export::check_export_visibility;

pub fn check_visibility(
    db: &DbIndex,
    file_id: FileId,
    emmyrc: &Emmyrc,
    infer_config: &mut LuaInferCache,
    token: LuaSyntaxToken,
    property_owner: LuaSemanticDeclId,
) -> Option<bool> {
    let property = db.get_property_index().get_property(&property_owner)?;
    if let Some(version_conds) = &property.version_conds {
        let version_number = emmyrc.runtime.version.to_lua_version_number();
        let visible = version_conds.iter().any(|cond| cond.check(&version_number));
        if !visible {
            return Some(false);
        }
    }

    if let Some(visibility) = property.visibility {
        match visibility {
            VisibilityKind::None | VisibilityKind::Public => return Some(true),
            VisibilityKind::Protected | VisibilityKind::Private => {
                return Some(
                    check_visibility_by_visibility(
                        db,
                        infer_config,
                        file_id,
                        property_owner,
                        token,
                        visibility,
                    )
                    .unwrap_or(false),
                );
            }
            VisibilityKind::Package => {
                return Some(file_id == property_owner.get_file_id()?);
            }
            VisibilityKind::Internal => {
                let property_file_id = property_owner.get_file_id()?;
                let property_workspace_id =
                    db.get_module_index().get_workspace_id(property_file_id)?;
                let current_workspace_id = db.get_module_index().get_workspace_id(file_id)?;
                if current_workspace_id != property_workspace_id {
                    return Some(false);
                }
            }
        }
    }

    Some(true)
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
        &member_owner,
        token.clone(),
        visibility,
    )
    .unwrap_or(false)
    {
        return Some(true);
    }

    let blocks = token.ancestors::<LuaBlock>();
    for block in blocks {
        if check_block_visibility(db, infer_config, &member_owner, block, visibility)
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
        let typ = infer_expr(db, infer_config, prefix_expr.into()).ok()?;
        if visibility == VisibilityKind::Protected {
            match (typ, member_owner) {
                (LuaType::Def(left), LuaMemberOwner::Type(right)) => {
                    if left == *right {
                        return Some(true);
                    }

                    if is_sub_type_of(db, &left, &right) {
                        return Some(true);
                    }
                }
                _ => {}
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
    file_id: FileId,
    member_owner: &LuaMemberOwner,
    token: LuaGeneralToken,
    visibility: VisibilityKind,
) -> Option<bool> {
    let index_expr = token.get_parent::<LuaIndexExpr>()?;
    let prefix_expr = index_expr.get_prefix_expr()?;
    let typ = infer_expr(db, infer_config, prefix_expr.into()).ok()?;

    if !in_def_file(db, &typ, file_id) {
        return Some(false);
    }

    match visibility {
        VisibilityKind::Protected => match (typ, member_owner) {
            (LuaType::Def(left), LuaMemberOwner::Type(right)) => {
                Some(left == *right || is_sub_type_of(db, &left, &right))
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

fn in_def_file(db: &DbIndex, typ: &LuaType, file_id: FileId) -> bool {
    match typ {
        LuaType::Def(id) => {
            let decl = db.get_type_index().get_type_decl(id);
            if let Some(decl) = decl {
                decl.get_locations()
                    .iter()
                    .any(|location| location.file_id == file_id)
            } else {
                false
            }
        }
        LuaType::TableConst(in_file) => in_file.file_id == file_id,
        _ => false,
    }
}
