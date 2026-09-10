use std::{borrow::Cow, rc::Rc};

use crate::{
    DbIndex, LuaIntersectionType, LuaMemberIndexItem, LuaMemberKey, LuaMemberOwner,
    LuaOwnerMembers, LuaType,
    semantic::{
        cache::TypeCacheEntry,
        member::MemberSymbol,
        type_check::{
            error_chain::{missing_members_message, property_message},
            is_optional,
            relation::{IntersectionState, Relater, RelationFailure, RelationResult},
            structured::index::relate_index_member,
        },
    },
};

pub(in crate::semantic::type_check) struct MemberView<'typ, 'db> {
    typ: &'typ LuaType,
    owner: Option<&'db LuaOwnerMembers>,
    entry: Option<Rc<TypeCacheEntry>>,
}

pub(super) enum MemberValue<'a> {
    Type(&'a LuaType),
    Indexed(&'a LuaMemberIndexItem),
    Cached(&'a MemberSymbol),
}

impl<'a> MemberValue<'a> {
    #[inline(always)]
    pub(super) fn typ(self, db: &'a DbIndex) -> Cow<'a, LuaType> {
        match self {
            Self::Type(typ) => Cow::Borrowed(typ),
            Self::Cached(member) => Cow::Borrowed(member.typ(db)),
            Self::Indexed(LuaMemberIndexItem::One(id)) => db
                .get_type_index()
                .get_type_cache(&(*id).into())
                .map(|cache| Cow::Borrowed(cache.as_type()))
                .unwrap_or(Cow::Owned(LuaType::Any)),
            Self::Indexed(item) => Cow::Owned(item.resolve_type(db).unwrap_or(LuaType::Any)),
        }
    }
}

impl<'typ, 'db> MemberView<'typ, 'db> {
    #[inline]
    pub(in crate::semantic::type_check) fn new(
        relater: &mut Relater<'db>,
        typ: &'typ LuaType,
    ) -> Self {
        let owner = match typ {
            LuaType::TableConst(range) => relater
                .db()
                .get_member_index()
                .get_owner_members(&LuaMemberOwner::Element(range.clone())),
            _ => None,
        };
        let entry = match typ {
            LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Generic(_)
            | LuaType::Union(_)
            | LuaType::MultiLineUnion(_)
            | LuaType::Intersection(_) => Some(relater.type_entry(typ)),
            _ => None,
        };
        Self { typ, owner, entry }
    }

    pub(super) fn is_empty(&self, db: &DbIndex) -> bool {
        match self.typ {
            LuaType::Object(object) => {
                object.get_fields().is_empty() && object.get_index_access().is_empty()
            }
            LuaType::TableConst(_) => self.owner.is_none_or(LuaOwnerMembers::is_empty),
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => self
                .entry
                .as_ref()
                .is_none_or(|entry| entry.members(db, self.typ).is_empty()),
            _ => true,
        }
    }

    #[inline(always)]
    pub(in crate::semantic::type_check) fn member_type(
        &self,
        relater: &mut Relater<'db>,
        key: &LuaMemberKey,
        intersection_state: IntersectionState,
    ) -> Result<Option<Cow<'_, LuaType>>, RelationFailure> {
        let db = relater.db();
        let member_type = match self.typ {
            LuaType::Object(object) => object
                .get_field(key)
                .or_else(|| {
                    let LuaMemberKey::TypeKey(key_type) = key else {
                        return None;
                    };
                    object
                        .get_index_access()
                        .iter()
                        .find_map(|(index_type, value_type)| {
                            (index_type == key_type).then_some(value_type)
                        })
                })
                .map(Cow::Borrowed),
            LuaType::TableConst(_) => self
                .owner
                .and_then(|owner| owner.get_member(key))
                .map(|item| MemberValue::Indexed(item).typ(db)),
            LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Generic(_)
            | LuaType::Union(_)
            | LuaType::MultiLineUnion(_)
            | LuaType::Intersection(_) => self
                .entry
                .as_ref()
                .and_then(|entry| entry.member_type(db, self.typ, key))
                .map(Cow::Borrowed),
            LuaType::Tuple(tuple) => match key {
                LuaMemberKey::Integer(index) if *index > 0 => {
                    tuple.get_type(*index as usize - 1).map(Cow::Borrowed)
                }
                _ => None,
            },
            LuaType::Array(array) => match key {
                LuaMemberKey::Integer(index) if *index > 0 => Some(Cow::Borrowed(array.get_base())),
                LuaMemberKey::TypeKey(key_type) if key_type.is_integer() => {
                    Some(Cow::Borrowed(array.get_base()))
                }
                _ => None,
            },
            LuaType::TableGeneric(params) if params.len() == 2 => {
                let Some(key_type) = key.to_index_type() else {
                    return Ok(None);
                };
                match relater.probe_relation(&key_type, &params[0], intersection_state) {
                    Ok(()) => Some(Cow::Borrowed(&params[1])),
                    Err(RelationFailure::Unrelated) => None,
                    Err(RelationFailure::Indeterminate(kind)) => {
                        return Err(RelationFailure::Indeterminate(kind));
                    }
                }
            }
            _ => None,
        };
        Ok(member_type)
    }

    pub(super) fn contains_key(
        &self,
        relater: &mut Relater<'db>,
        key: &LuaMemberKey,
        intersection_state: IntersectionState,
    ) -> Result<bool, RelationFailure> {
        if let Some(entry) = &self.entry {
            return Ok(entry.members(relater.db(), self.typ).contains_key(key));
        }
        if matches!(self.typ, LuaType::TableConst(_)) {
            return Ok(self.owner.is_some_and(|owner| owner.contains_member(key)));
        }
        Ok(self
            .member_type(relater, key, intersection_state)?
            .is_some())
    }

    pub(super) fn visit(
        &self,
        db: &DbIndex,
        mut visitor: impl FnMut(&LuaMemberKey, MemberValue<'_>) -> RelationResult,
    ) -> RelationResult {
        match self.typ {
            LuaType::Object(object) => {
                for (key, typ) in object.get_fields() {
                    visitor(key, MemberValue::Type(typ))?;
                }
                for (key, typ) in object.get_index_access() {
                    visitor(&LuaMemberKey::TypeKey(key.clone()), MemberValue::Type(typ))?;
                }
            }
            LuaType::TableConst(_) => {
                if let Some(owner) = self.owner {
                    for (key, item) in owner.iter() {
                        visitor(key, MemberValue::Indexed(item))?;
                    }
                }
            }
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
                if let Some(entry) = &self.entry {
                    for (key, member) in entry.members(db, self.typ) {
                        visitor(key, MemberValue::Cached(member))?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn visit_types(
        &self,
        relater: &mut Relater<'db>,
        mut visitor: impl FnMut(&mut Relater<'db>, &LuaMemberKey, &LuaType) -> RelationResult,
    ) -> RelationResult {
        let db = relater.db();
        self.visit(db, |key, value| visitor(relater, key, &value.typ(db)))
    }
}

#[inline]
pub(in crate::semantic::type_check) fn relate_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> RelationResult {
    let target_members = MemberView::new(relater, target);
    if target_members.is_empty(relater.db()) {
        return Ok(());
    }
    let source_members = MemberView::new(relater, source);
    let (missing_keys, _) = collect_missing_from_views(
        relater,
        &source_members,
        &target_members,
        intersection_state,
    )?;
    if !missing_keys.is_empty() {
        return unrelated_missing_members(relater, source, target, missing_keys);
    }
    target_members.visit_types(relater, |relater, key, member_type| {
        if let LuaMemberKey::TypeKey(key_type) = key {
            if intersection_state.contains(IntersectionState::TARGET) {
                return Ok(());
            }
            return relate_index_member(
                relater,
                source,
                target,
                key_type,
                member_type,
                intersection_state,
            );
        }
        relater.consume_relation_budget()?;
        let Some(source_type) = source_members.member_type(relater, key, intersection_state)?
        else {
            // 诊断模式已完整检查缺失字段, 此处只剩可选成员.
            if relater.is_explain() || is_optional(relater.db(), member_type) {
                return Ok(());
            }
            return relater.fail(|db| {
                missing_members_message(db, source, member_type, std::slice::from_ref(key))
            });
        };
        let result = relater.relate(&source_type, member_type, intersection_state);
        relater.on_unrelated(result, |_| property_message(key))
    })
}

pub(in crate::semantic::type_check) fn relate_target_intersection_index_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection: &LuaIntersectionType,
) -> RelationResult {
    for member in intersection.get_types() {
        if let LuaType::Intersection(nested) = member {
            relate_target_intersection_index_members(relater, source, target, nested)?;
            continue;
        }
        MemberView::new(relater, member).visit_types(relater, |relater, key, value_type| {
            let LuaMemberKey::TypeKey(key_type) = key else {
                return Ok(());
            };
            relate_index_member(
                relater,
                source,
                target,
                key_type,
                value_type,
                IntersectionState::NONE,
            )
        })?;
    }
    Ok(())
}

/// 只在缺少字段时读取目标类型, 避免缺失检查提前实例化已有字段.
fn collect_missing_from_views<'db>(
    relater: &mut Relater<'db>,
    source: &MemberView<'_, 'db>,
    target: &MemberView<'_, 'db>,
    intersection_state: IntersectionState,
) -> Result<(Vec<LuaMemberKey>, bool), RelationFailure> {
    let db = relater.db();
    let mut missing_keys = Vec::new();
    let mut has_shared_key = false;
    target.visit(db, |key, value| {
        if matches!(key, LuaMemberKey::TypeKey(_)) {
            return Ok(());
        }
        if source.contains_key(relater, key, intersection_state)? {
            has_shared_key = true;
        } else if !is_optional(db, &value.typ(db)) {
            missing_keys.push(key.clone());
        }
        Ok(())
    })?;
    Ok((missing_keys, has_shared_key))
}

pub(in crate::semantic::type_check) fn collect_missing_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Result<(Vec<LuaMemberKey>, bool), RelationFailure> {
    let target_members = MemberView::new(relater, target);
    if target_members.is_empty(relater.db()) {
        return Ok((Vec::new(), false));
    }
    let source_members = MemberView::new(relater, source);
    collect_missing_from_views(
        relater,
        &source_members,
        &target_members,
        intersection_state,
    )
}

pub(in crate::semantic::type_check) fn unrelated_missing_members(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    keys: Vec<LuaMemberKey>,
) -> RelationResult {
    relater.fail(|db| missing_members_message(db, source, target, &keys))
}

#[cfg(test)]
mod tests {
    use crate::{
        VirtualWorkspace,
        semantic::{
            cache::SemanticLocalCache,
            type_check::{RelationFailure, probe_assignable},
        },
    };

    #[test]
    fn declared_sequences_use_all_duplicate_index_declarations() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CacheArrayValues<T>
            ---@field [integer] T
            ---@field [integer] number
            ---@class CacheTupleValues<T>
            ---@field [1] T
            ---@field [1] number
            ---@class CacheInheritedArray: CacheArrayValues<string>
            ---@class CacheInheritedTuple: CacheTupleValues<string>
        "#,
        );
        for (source, narrow, wide) in [
            (
                "CacheArrayValues<string>",
                "string[]",
                "(string | number)[]",
            ),
            ("CacheInheritedArray", "string[]", "(string | number)[]"),
            ("CacheTupleValues<string>", "[string]", "[string | number]"),
            ("CacheInheritedTuple", "[string]", "[string | number]"),
        ] {
            let source = ws.ty(source);
            let narrow = ws.ty(narrow);
            let wide = ws.ty(wide);
            let db = ws.analysis.compilation.get_db();
            let mut cache = SemanticLocalCache::default();
            assert_eq!(
                probe_assignable(db, &source, &narrow, Some(&mut cache)),
                Err(RelationFailure::Unrelated)
            );
            assert_eq!(
                probe_assignable(db, &source, &wide, Some(&mut cache)),
                Ok(())
            );
        }
    }
}
