use crate::{DbIndex, InferFailReason, LuaSemanticDeclId, LuaType, TypeOps};

use super::LuaMemberId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaMemberIndexItem {
    One(LuaMemberId),
    Many(Vec<LuaMemberId>),
}

impl LuaMemberIndexItem {
    pub fn resolve_type(&self, db: &DbIndex) -> Result<LuaType, InferFailReason> {
        resolve_member_type(db, &self)
    }

    pub fn resolve_semantic_decl(&self, db: &DbIndex) -> Option<LuaSemanticDeclId> {
        resolve_member_semantic_id(db, &self)
    }

    #[allow(unused)]
    pub fn resolve_type_owner_member_id(&self, db: &DbIndex) -> Option<LuaMemberId> {
        resolve_type_owner_member_id(db, &self)
    }

    pub fn is_one(&self) -> bool {
        matches!(self, LuaMemberIndexItem::One(_))
    }

    pub fn get_member_ids(&self) -> Vec<LuaMemberId> {
        match self {
            LuaMemberIndexItem::One(member_id) => vec![*member_id],
            LuaMemberIndexItem::Many(member_ids) => member_ids.clone(),
        }
    }
}

fn resolve_member_type(
    db: &DbIndex,
    member_item: &LuaMemberIndexItem,
) -> Result<LuaType, InferFailReason> {
    match member_item {
        LuaMemberIndexItem::One(member_id) => {
            let member_type_cache = db
                .get_type_index()
                .get_type_cache(&member_id.clone().into());
            return match member_type_cache {
                Some(cache) => Ok(cache.as_type().clone()),
                None => Err(InferFailReason::UnResolveMemberType(*member_id)),
            };
        }
        LuaMemberIndexItem::Many(member_ids) => {
            let mut resolve_state = MemberTypeResolveState::All;
            let mut members = vec![];
            for member_id in member_ids {
                if let Some(member) = db.get_member_index().get_member(member_id) {
                    members.push(member);
                } else {
                    return Err(InferFailReason::None);
                }
            }
            if db.get_emmyrc().strict.meta_override_file_define {
                for member in &members {
                    let feature = member.get_feature();
                    if feature.is_meta_decl() {
                        resolve_state = MemberTypeResolveState::Meta;
                        break;
                    } else if feature.is_file_decl() {
                        resolve_state = MemberTypeResolveState::FileDecl;
                    }
                }
            }

            match resolve_state {
                MemberTypeResolveState::All => {
                    let mut typ = LuaType::Unknown;
                    for member in members {
                        typ = TypeOps::Union.apply(
                            db,
                            &typ,
                            &db.get_type_index()
                                .get_type_cache(&member.get_id().into())
                                .ok_or(InferFailReason::UnResolveMemberType(member.get_id()))?
                                .as_type(),
                        );
                    }
                    Ok(typ)
                }
                MemberTypeResolveState::Meta => {
                    let mut typ = LuaType::Unknown;
                    for member in &members {
                        let feature = member.get_feature();
                        if feature.is_meta_decl() {
                            typ = TypeOps::Union.apply(
                                db,
                                &typ,
                                &db.get_type_index()
                                    .get_type_cache(&member.get_id().into())
                                    .ok_or(InferFailReason::UnResolveMemberType(member.get_id()))?
                                    .as_type(),
                            );
                        }
                    }
                    Ok(typ)
                }
                MemberTypeResolveState::FileDecl => {
                    let mut typ = LuaType::Unknown;
                    for member in &members {
                        let feature = member.get_feature();
                        if feature.is_file_decl() {
                            typ = TypeOps::Union.apply(
                                db,
                                &typ,
                                &db.get_type_index()
                                    .get_type_cache(&member.get_id().into())
                                    .ok_or(InferFailReason::UnResolveMemberType(member.get_id()))?
                                    .as_type(),
                            );
                        }
                    }
                    Ok(typ)
                }
            }
        }
    }
}

fn resolve_type_owner_member_id(
    db: &DbIndex,
    member_item: &LuaMemberIndexItem,
) -> Option<LuaMemberId> {
    match member_item {
        LuaMemberIndexItem::One(member_id) => Some(*member_id),
        LuaMemberIndexItem::Many(member_ids) => {
            let member_index = db.get_member_index();
            let mut resolve_state = MemberTypeResolveState::All;
            let members = member_ids
                .iter()
                .map(|id| member_index.get_member(id))
                .collect::<Option<Vec<_>>>()?;
            for member in &members {
                let feature = member.get_feature();
                if feature.is_meta_decl() {
                    resolve_state = MemberTypeResolveState::Meta;
                    break;
                } else if feature.is_file_decl() {
                    resolve_state = MemberTypeResolveState::FileDecl;
                }
            }

            match resolve_state {
                MemberTypeResolveState::All => {
                    for member in members {
                        let member_type_cache = db
                            .get_type_index()
                            .get_type_cache(&member.get_id().into())?;
                        if member_type_cache.as_type().is_member_owner() {
                            return Some(member.get_id());
                        }
                    }

                    None
                }
                MemberTypeResolveState::Meta => {
                    for member in &members {
                        let feature = member.get_feature();
                        if feature.is_meta_decl() {
                            return Some(member.get_id());
                        }
                    }

                    None
                }
                MemberTypeResolveState::FileDecl => {
                    for member in &members {
                        let feature = member.get_feature();
                        if feature.is_file_decl() {
                            return Some(member.get_id());
                        }
                    }

                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberTypeResolveState {
    All,
    Meta,
    FileDecl,
}

fn resolve_member_semantic_id(
    db: &DbIndex,
    member_item: &LuaMemberIndexItem,
) -> Option<LuaSemanticDeclId> {
    match member_item {
        LuaMemberIndexItem::One(member_id) => Some(LuaSemanticDeclId::Member(*member_id)),
        LuaMemberIndexItem::Many(member_ids) => {
            let mut resolve_state = MemberSemanticDeclResolveState::MetaOrNone;
            let members = member_ids
                .iter()
                .map(|id| db.get_member_index().get_member(id))
                .collect::<Option<Vec<_>>>()?;
            for member in &members {
                let feature = member.get_feature();
                if feature.is_file_define() {
                    resolve_state = MemberSemanticDeclResolveState::FirstDefine;
                } else if feature.is_file_decl() {
                    resolve_state = MemberSemanticDeclResolveState::FileDecl;
                    break;
                }
            }

            match resolve_state {
                MemberSemanticDeclResolveState::MetaOrNone => {
                    let mut last_valid_member =
                        LuaSemanticDeclId::Member(members.first()?.get_id());
                    for member in &members {
                        let feature = member.get_feature();
                        if feature.is_meta_decl() {
                            let semantic_id = LuaSemanticDeclId::Member(member.get_id());
                            last_valid_member = semantic_id.clone();
                            if check_member_version(db, semantic_id.clone()) {
                                return Some(semantic_id);
                            }
                        }
                    }

                    Some(last_valid_member)
                }
                MemberSemanticDeclResolveState::FirstDefine => {
                    for member in &members {
                        let feature = member.get_feature();
                        if feature.is_file_define() {
                            return Some(LuaSemanticDeclId::Member(member.get_id()));
                        }
                    }

                    None
                }
                MemberSemanticDeclResolveState::FileDecl => {
                    for member in &members {
                        let feature = member.get_feature();
                        if feature.is_file_decl() {
                            return Some(LuaSemanticDeclId::Member(member.get_id()));
                        }
                    }

                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberSemanticDeclResolveState {
    MetaOrNone,
    FirstDefine,
    FileDecl,
}

fn check_member_version(db: &DbIndex, semantic_id: LuaSemanticDeclId) -> bool {
    let Some(property) = db.get_property_index().get_property(&semantic_id) else {
        return true;
    };

    if let Some(version) = &property.version_conds {
        let version_number = db.get_emmyrc().runtime.version.to_lua_version_number();
        return version.iter().any(|cond| cond.check(&version_number));
    }

    true
}
