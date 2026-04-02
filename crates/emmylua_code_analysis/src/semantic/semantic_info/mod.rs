mod infer_expr_semantic_decl;
mod resolve_global_decl;
mod semantic_decl_level;
mod semantic_guard;

use crate::{
    DbIndex, LuaDeclExtra, LuaDeclId, LuaInstanceType, LuaMemberId, LuaMemberOwner,
    LuaSemanticDeclId, LuaType, LuaTypeCache, TypeOps,
};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocNameType, LuaDocTag, LuaExpr, LuaLocalName, LuaParamName,
    LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken, LuaTableField,
};
pub use infer_expr_semantic_decl::infer_expr_semantic_decl;
pub use resolve_global_decl::resolve_global_decl_id;
pub use semantic_decl_level::SemanticDeclLevel;
pub use semantic_guard::SemanticDeclGuard;

use super::{LuaInferCache, infer_expr};

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticInfo {
    pub typ: LuaType,
    pub semantic_decl: Option<LuaSemanticDeclId>,
}

pub fn infer_token_semantic_info(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    token: LuaSyntaxToken,
) -> Option<SemanticInfo> {
    let parent = token.parent()?;
    match parent.kind().into() {
        LuaSyntaxKind::ForStat | LuaSyntaxKind::ForRangeStat | LuaSyntaxKind::LocalName => {
            let file_id = cache.get_file_id();
            let decl_id = LuaDeclId::new(file_id, token.text_range().start());
            let type_cache = db
                .get_type_index()
                .get_type_cache(&decl_id.into())
                .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown));
            let mut typ = type_cache.as_type().clone();

            // Only narrow LocalName declarations — ForStat/ForRangeStat cannot have
            // table literal initializers.
            if matches!(parent.kind().into(), LuaSyntaxKind::LocalName) {
                let root = parent.ancestors().last()?;
                if let Some(narrowed) =
                    try_narrow_local_to_instance(db, cache, &typ, &decl_id, &root)
                {
                    typ = narrowed;
                }
            }

            Some(SemanticInfo {
                typ,
                semantic_decl: Some(LuaSemanticDeclId::LuaDecl(decl_id)),
            })
        }
        LuaSyntaxKind::ParamName => {
            let file_id = cache.get_file_id();
            let decl_id = LuaDeclId::new(file_id, token.text_range().start());
            let decl = db.get_decl_index().get_decl(&decl_id)?;
            match &decl.extra {
                LuaDeclExtra::Param {
                    idx, signature_id, ..
                } => {
                    let signature = db.get_signature_index().get(signature_id)?;
                    let param_info = signature.get_param_info_by_id(*idx)?;
                    let mut typ = param_info.type_ref.clone();
                    if param_info.nullable && !typ.is_nullable() {
                        typ = TypeOps::Union.apply(db, &typ, &LuaType::Nil);
                    }

                    Some(SemanticInfo {
                        typ,
                        semantic_decl: Some(LuaSemanticDeclId::LuaDecl(decl_id)),
                    })
                }
                _ => None,
            }
        }
        _ => infer_node_semantic_info(db, cache, parent),
    }
}

pub fn infer_node_semantic_info(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    node: LuaSyntaxNode,
) -> Option<SemanticInfo> {
    match node {
        expr_node if LuaExpr::can_cast(expr_node.kind().into()) => {
            let expr = LuaExpr::cast(expr_node)?;
            let typ = infer_expr(db, cache, expr.clone()).unwrap_or(LuaType::Unknown);
            let property_owner = infer_expr_semantic_decl(
                db,
                cache,
                expr,
                SemanticDeclGuard::default(),
                SemanticDeclLevel::default(),
            );
            Some(SemanticInfo {
                typ,
                semantic_decl: property_owner,
            })
        }
        table_field_node if LuaTableField::can_cast(table_field_node.kind().into()) => {
            let table_field = LuaTableField::cast(table_field_node)?;
            let member_id = LuaMemberId::new(table_field.get_syntax_id(), cache.get_file_id());
            let type_cache = db
                .get_type_index()
                .get_type_cache(&member_id.into())
                .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown));
            Some(SemanticInfo {
                typ: type_cache.as_type().clone(),
                semantic_decl: Some(LuaSemanticDeclId::Member(member_id)),
            })
        }
        name_type if LuaDocNameType::can_cast(name_type.kind().into()) => {
            let name_type = LuaDocNameType::cast(name_type)?;
            let name = name_type.get_name_text()?;
            let type_decl = db
                .get_type_index()
                .find_type_decl(cache.get_file_id(), &name)?;
            Some(SemanticInfo {
                typ: LuaType::Ref(type_decl.get_id()),
                semantic_decl: LuaSemanticDeclId::TypeDecl(type_decl.get_id()).into(),
            })
        }
        tags if LuaDocTag::can_cast(tags.kind().into()) => {
            let tag = LuaDocTag::cast(tags)?;
            match tag {
                LuaDocTag::Alias(alias) => {
                    type_def_tag_info(alias.get_name_token()?.get_name_text(), db, cache)
                }
                LuaDocTag::Class(class) => {
                    type_def_tag_info(class.get_name_token()?.get_name_text(), db, cache)
                }
                LuaDocTag::Enum(enum_) => {
                    type_def_tag_info(enum_.get_name_token()?.get_name_text(), db, cache)
                }
                LuaDocTag::Field(field) => {
                    let member_id = LuaMemberId::new(field.get_syntax_id(), cache.get_file_id());
                    let type_cache = db
                        .get_type_index()
                        .get_type_cache(&member_id.into())
                        .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown));
                    Some(SemanticInfo {
                        typ: type_cache.as_type().clone(),
                        semantic_decl: Some(LuaSemanticDeclId::Member(member_id)),
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// If `typ` is a class type and `decl_id` has a non-empty `TableConst` initializer,
/// returns `Instance(typ, range)`. Otherwise returns `None`.
fn try_narrow_local_to_instance(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    typ: &LuaType,
    decl_id: &LuaDeclId,
    root: &LuaSyntaxNode,
) -> Option<LuaType> {
    if !typ.is_class_type(db) {
        return None;
    }
    let decl = db.get_decl_index().get_decl(decl_id)?;
    let value_syntax_id = decl.get_value_syntax_id()?;
    let node = value_syntax_id.to_node_from_root(root)?;
    let expr = LuaExpr::cast(node)?;
    let LuaType::TableConst(range) = infer_expr(db, cache, expr).ok()? else {
        return None;
    };
    let owner = LuaMemberOwner::Element(range.clone());
    if !db
        .get_member_index()
        .get_members(&owner)
        .is_some_and(|m| !m.is_empty())
    {
        return None;
    }
    Some(LuaType::Instance(
        LuaInstanceType::new(typ.clone(), range).into(),
    ))
}

fn type_def_tag_info(name: &str, db: &DbIndex, cache: &mut LuaInferCache) -> Option<SemanticInfo> {
    let type_decl = db
        .get_type_index()
        .find_type_decl(cache.get_file_id(), name)?;
    Some(SemanticInfo {
        typ: LuaType::Ref(type_decl.get_id()),
        semantic_decl: LuaSemanticDeclId::TypeDecl(type_decl.get_id()).into(),
    })
}

pub fn infer_token_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    token: LuaSyntaxToken,
    level: SemanticDeclLevel,
) -> Option<LuaSemanticDeclId> {
    let parent = token.parent()?;
    match parent.kind().into() {
        LuaSyntaxKind::ForStat
        | LuaSyntaxKind::ForRangeStat
        | LuaSyntaxKind::LocalName
        | LuaSyntaxKind::ParamName => {
            let file_id = cache.get_file_id();
            let decl_id = LuaDeclId::new(file_id, token.text_range().start());
            Some(LuaSemanticDeclId::LuaDecl(decl_id))
        }
        _ => infer_node_semantic_decl(db, cache, parent, level),
    }
}

pub fn infer_node_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    node: LuaSyntaxNode,
    level: SemanticDeclLevel,
) -> Option<LuaSemanticDeclId> {
    match node {
        expr_node if LuaExpr::can_cast(expr_node.kind().into()) => {
            let expr = LuaExpr::cast(expr_node)?;
            infer_expr_semantic_decl(db, cache, expr, SemanticDeclGuard::default(), level)
        }
        table_field_node if LuaTableField::can_cast(table_field_node.kind().into()) => {
            let table_field = LuaTableField::cast(table_field_node)?;
            let member_id = LuaMemberId::new(table_field.get_syntax_id(), cache.get_file_id());
            Some(LuaSemanticDeclId::Member(member_id))
        }
        name_type if LuaDocNameType::can_cast(name_type.kind().into()) => {
            let name_type = LuaDocNameType::cast(name_type)?;
            let name = name_type.get_name_text()?;
            let type_decl = db
                .get_type_index()
                .find_type_decl(cache.get_file_id(), &name)?;
            LuaSemanticDeclId::TypeDecl(type_decl.get_id()).into()
        }
        tags if LuaDocTag::can_cast(tags.kind().into()) => {
            let tag = LuaDocTag::cast(tags)?;
            match tag {
                LuaDocTag::Alias(alias) => {
                    type_def_tag_property_owner(alias.get_name_token()?.get_name_text(), db, cache)
                }
                LuaDocTag::Class(class) => {
                    type_def_tag_property_owner(class.get_name_token()?.get_name_text(), db, cache)
                }
                LuaDocTag::Enum(enum_) => {
                    type_def_tag_property_owner(enum_.get_name_token()?.get_name_text(), db, cache)
                }
                LuaDocTag::Field(field) => {
                    let member_id = LuaMemberId::new(field.get_syntax_id(), cache.get_file_id());
                    Some(LuaSemanticDeclId::Member(member_id))
                }
                _ => None,
            }
        }
        local_name if LuaLocalName::can_cast(local_name.kind().into()) => {
            let local_name = LuaLocalName::cast(local_name)?;
            let name_token = local_name.get_name_token()?;
            infer_token_semantic_decl(db, cache, name_token.syntax().clone(), level)
        }
        param_name if LuaParamName::can_cast(param_name.kind().into()) => {
            let param_name = LuaParamName::cast(param_name)?;
            let name_token = param_name.get_name_token()?;
            infer_token_semantic_decl(db, cache, name_token.syntax().clone(), level)
        }
        _ => None,
    }
}

fn type_def_tag_property_owner(
    name: &str,
    db: &DbIndex,
    cache: &mut LuaInferCache,
) -> Option<LuaSemanticDeclId> {
    let type_decl = db
        .get_type_index()
        .find_type_decl(cache.get_file_id(), name)?;
    LuaSemanticDeclId::TypeDecl(type_decl.get_id()).into()
}
