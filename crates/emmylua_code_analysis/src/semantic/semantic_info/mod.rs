mod infer_expr_property_owner;
mod owner_guard;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocNameType, LuaDocTag, LuaExpr, LuaLocalName, LuaSyntaxKind,
    LuaSyntaxNode, LuaSyntaxToken, LuaTableField,
};
use infer_expr_property_owner::infer_expr_property_owner;
use owner_guard::OwnerGuard;

use crate::{DbIndex, LuaDeclExtra, LuaDeclId, LuaMemberId, LuaPropertyOwnerId, LuaType};

use super::{infer_expr, LuaInferConfig};

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticInfo {
    pub typ: LuaType,
    pub property_owner: Option<LuaPropertyOwnerId>,
}

pub fn infer_token_semantic_info(
    db: &DbIndex,
    infer_config: &mut LuaInferConfig,
    token: LuaSyntaxToken,
) -> Option<SemanticInfo> {
    let parent = token.parent()?;
    match parent.kind().into() {
        LuaSyntaxKind::ForStat | LuaSyntaxKind::ForRangeStat | LuaSyntaxKind::LocalName => {
            let file_id = infer_config.get_file_id();
            let decl_id = LuaDeclId::new(file_id, token.text_range().start());
            let decl = db.get_decl_index().get_decl(&decl_id)?;
            let typ = decl.get_type().cloned().unwrap_or(LuaType::Unknown);
            Some(SemanticInfo {
                typ,
                property_owner: Some(LuaPropertyOwnerId::LuaDecl(decl_id)),
            })
        }
        LuaSyntaxKind::ParamName => {
            let file_id = infer_config.get_file_id();
            let decl_id = LuaDeclId::new(file_id, token.text_range().start());
            let decl = db.get_decl_index().get_decl(&decl_id)?;
            match &decl.extra {
                LuaDeclExtra::Param {
                    idx, signature_id, ..
                } => {
                    let signature = db.get_signature_index().get(&signature_id)?;
                    let param_info = signature.get_param_info_by_id(*idx)?;
                    let mut typ = param_info.type_ref.clone();
                    if param_info.nullable && !typ.is_nullable() {
                        typ = LuaType::Nullable(typ.into());
                    }

                    Some(SemanticInfo {
                        typ,
                        property_owner: Some(LuaPropertyOwnerId::LuaDecl(decl_id)),
                    })
                }
                _ => None,
            }
        }
        _ => infer_node_semantic_info(db, infer_config, parent),
    }
}

pub fn infer_node_semantic_info(
    db: &DbIndex,
    infer_config: &mut LuaInferConfig,
    node: LuaSyntaxNode,
) -> Option<SemanticInfo> {
    match node {
        expr_node if LuaExpr::can_cast(expr_node.kind().into()) => {
            let expr = LuaExpr::cast(expr_node)?;
            let typ = infer_expr(db, infer_config, expr.clone()).unwrap_or(LuaType::Unknown);
            let property_owner =
                infer_expr_property_owner(db, infer_config, expr, OwnerGuard::default());
            Some(SemanticInfo {
                typ,
                property_owner,
            })
        }
        table_field_node if LuaTableField::can_cast(table_field_node.kind().into()) => {
            let table_field = LuaTableField::cast(table_field_node)?;
            let member_id =
                LuaMemberId::new(table_field.get_syntax_id(), infer_config.get_file_id());
            let member = db.get_member_index().get_member(&member_id)?;
            let typ = member.get_decl_type().clone();
            Some(SemanticInfo {
                typ,
                property_owner: Some(LuaPropertyOwnerId::Member(member_id)),
            })
        }
        name_type if LuaDocNameType::can_cast(name_type.kind().into()) => {
            let name_type = LuaDocNameType::cast(name_type)?;
            let name = name_type.get_name_text()?;
            let type_decl = db
                .get_type_index()
                .find_type_decl(infer_config.get_file_id(), &name)?;
            Some(SemanticInfo {
                typ: LuaType::Ref(type_decl.get_id()),
                property_owner: LuaPropertyOwnerId::TypeDecl(type_decl.get_id()).into(),
            })
        }
        tags if LuaDocTag::can_cast(tags.kind().into()) => {
            let tag = LuaDocTag::cast(tags)?;
            match tag {
                LuaDocTag::Alias(alias) => {
                    type_def_tag_info(alias.get_name_token()?.get_name_text(), db, infer_config)
                }
                LuaDocTag::Class(class) => {
                    type_def_tag_info(class.get_name_token()?.get_name_text(), db, infer_config)
                }
                LuaDocTag::Enum(enum_) => {
                    type_def_tag_info(enum_.get_name_token()?.get_name_text(), db, infer_config)
                }
                LuaDocTag::Field(field) => {
                    let member_id =
                        LuaMemberId::new(field.get_syntax_id(), infer_config.get_file_id());
                    let member = db.get_member_index().get_member(&member_id)?;
                    let typ = member.get_decl_type();
                    Some(SemanticInfo {
                        typ: typ.clone(),
                        property_owner: Some(LuaPropertyOwnerId::Member(member_id)),
                    })
                }
                _ => return None,
            }
        }
        _ => None,
    }
}

fn type_def_tag_info(
    name: &str,
    db: &DbIndex,
    infer_config: &mut LuaInferConfig,
) -> Option<SemanticInfo> {
    let type_decl = db
        .get_type_index()
        .find_type_decl(infer_config.get_file_id(), name)?;
    Some(SemanticInfo {
        typ: LuaType::Ref(type_decl.get_id()),
        property_owner: LuaPropertyOwnerId::TypeDecl(type_decl.get_id()).into(),
    })
}

pub fn infer_token_property_owner(
    db: &DbIndex,
    infer_config: &mut LuaInferConfig,
    token: LuaSyntaxToken,
) -> Option<LuaPropertyOwnerId> {
    let parent = token.parent()?;
    match parent.kind().into() {
        LuaSyntaxKind::ForStat
        | LuaSyntaxKind::ForRangeStat
        | LuaSyntaxKind::LocalName
        | LuaSyntaxKind::ParamName => {
            let file_id = infer_config.get_file_id();
            let decl_id = LuaDeclId::new(file_id, token.text_range().start());
            Some(LuaPropertyOwnerId::LuaDecl(decl_id))
        }
        _ => infer_node_property_owner(db, infer_config, parent),
    }
}

pub fn infer_node_property_owner(
    db: &DbIndex,
    infer_config: &mut LuaInferConfig,
    node: LuaSyntaxNode,
) -> Option<LuaPropertyOwnerId> {
    match node {
        expr_node if LuaExpr::can_cast(expr_node.kind().into()) => {
            let expr = LuaExpr::cast(expr_node)?;
            infer_expr_property_owner(db, infer_config, expr, OwnerGuard::default())
        }
        table_field_node if LuaTableField::can_cast(table_field_node.kind().into()) => {
            let table_field = LuaTableField::cast(table_field_node)?;
            let member_id =
                LuaMemberId::new(table_field.get_syntax_id(), infer_config.get_file_id());
            Some(LuaPropertyOwnerId::Member(member_id))
        }
        name_type if LuaDocNameType::can_cast(name_type.kind().into()) => {
            let name_type = LuaDocNameType::cast(name_type)?;
            let name = name_type.get_name_text()?;
            let type_decl = db
                .get_type_index()
                .find_type_decl(infer_config.get_file_id(), &name)?;
            LuaPropertyOwnerId::TypeDecl(type_decl.get_id()).into()
        }
        tags if LuaDocTag::can_cast(tags.kind().into()) => {
            let tag = LuaDocTag::cast(tags)?;
            match tag {
                LuaDocTag::Alias(alias) => type_def_tag_property_owner(
                    alias.get_name_token()?.get_name_text(),
                    db,
                    infer_config,
                ),
                LuaDocTag::Class(class) => type_def_tag_property_owner(
                    class.get_name_token()?.get_name_text(),
                    db,
                    infer_config,
                ),
                LuaDocTag::Enum(enum_) => type_def_tag_property_owner(
                    enum_.get_name_token()?.get_name_text(),
                    db,
                    infer_config,
                ),
                LuaDocTag::Field(field) => {
                    let member_id =
                        LuaMemberId::new(field.get_syntax_id(), infer_config.get_file_id());
                    Some(LuaPropertyOwnerId::Member(member_id))
                }
                _ => return None,
            }
        }
        local_name if LuaLocalName::can_cast(local_name.kind().into()) => {
            let local_name = LuaLocalName::cast(local_name)?;
            let name_token = local_name.get_name_token()?;
            infer_token_property_owner(db, infer_config, name_token.syntax().clone())
        }
        _ => None,
    }
}

fn type_def_tag_property_owner(
    name: &str,
    db: &DbIndex,
    infer_config: &mut LuaInferConfig,
) -> Option<LuaPropertyOwnerId> {
    let type_decl = db
        .get_type_index()
        .find_type_decl(infer_config.get_file_id(), name)?;
    LuaPropertyOwnerId::TypeDecl(type_decl.get_id()).into()
}
