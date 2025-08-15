mod lua_member;
mod lua_member_feature;
mod lua_member_item;
mod lua_member_owner;
mod lua_owner_members;

use std::collections::{HashMap, HashSet};

use super::traits::LuaIndex;
use crate::{FileId, GlobalId, db_index::member::lua_owner_members::LuaOwnerMembers};
pub use lua_member::{LuaMember, LuaMemberId, LuaMemberKey};
pub use lua_member_feature::LuaMemberFeature;
pub use lua_member_item::LuaMemberIndexItem;
pub use lua_member_owner::LuaMemberOwner;

#[derive(Debug)]
pub struct LuaMemberIndex {
    members: HashMap<LuaMemberId, LuaMember>,
    in_filed: HashMap<FileId, HashSet<MemberOrOwner>>,
    owner_members: HashMap<LuaMemberOwner, LuaOwnerMembers>,
    member_current_owner: HashMap<LuaMemberId, LuaMemberOwner>,
    member_global_id: HashMap<LuaMemberId, GlobalId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MemberOrOwner {
    Member(LuaMemberId),
    Owner(LuaMemberOwner),
}

impl LuaMemberIndex {
    pub fn new() -> Self {
        Self {
            members: HashMap::new(),
            in_filed: HashMap::new(),
            owner_members: HashMap::new(),
            member_current_owner: HashMap::new(),
            member_global_id: HashMap::new(),
        }
    }

    pub fn add_member(&mut self, owner: LuaMemberOwner, member: LuaMember) -> LuaMemberId {
        let id = member.get_id();
        let file_id = member.get_file_id();
        self.members.insert(id, member);
        self.add_in_file_object(file_id, MemberOrOwner::Member(id));

        self.member_current_owner.insert(id, owner.clone());
        self.add_in_file_object(file_id, MemberOrOwner::Owner(owner.clone()));
        self.add_member_to_owner(owner.clone(), id);

        id
    }

    fn add_in_file_object(&mut self, file_id: FileId, member_or_owner: MemberOrOwner) {
        self.in_filed
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(member_or_owner);
    }

    pub fn add_member_to_owner(&mut self, owner: LuaMemberOwner, id: LuaMemberId) -> Option<()> {
        let member = self.get_member(&id)?;
        let key = member.get_key().clone();
        let feature = member.get_feature();
        let member_map = self
            .owner_members
            .entry(owner.clone())
            .or_insert_with(LuaOwnerMembers::new);
        if feature.is_decl() {
            if let Some(item) = member_map.get_member_mut(&key) {
                match item {
                    LuaMemberIndexItem::One(old_id) => {
                        if old_id != &id {
                            let ids = vec![old_id.clone(), id];
                            *item = LuaMemberIndexItem::Many(ids);
                        }
                    }
                    LuaMemberIndexItem::Many(ids) => {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            } else {
                member_map.add_member(key.clone(), LuaMemberIndexItem::One(id));
            }
        } else {
            if !member_map.contains_member(&key) {
                member_map.add_member(key, LuaMemberIndexItem::One(id));
                return Some(());
            }

            let item = member_map.get_member(&key)?.clone();
            let new_items = if self.is_item_only_meta(&item) {
                match item {
                    LuaMemberIndexItem::One(old_id) => {
                        if old_id == id {
                            return Some(());
                        }
                        LuaMemberIndexItem::Many(vec![id, old_id])
                    }
                    LuaMemberIndexItem::Many(mut ids) => {
                        if ids.contains(&id) {
                            return Some(());
                        }

                        ids.push(id);
                        LuaMemberIndexItem::Many(ids)
                    }
                }
            } else {
                return Some(());
            };

            self.owner_members
                .entry(owner.clone())
                .or_insert_with(LuaOwnerMembers::new)
                .add_member(key.clone(), new_items);
        }

        Some(())
    }

    pub fn add_member_global_id(
        &mut self,
        member_id: LuaMemberId,
        global_id: GlobalId,
    ) -> Option<()> {
        self.member_global_id.insert(member_id, global_id);
        Some(())
    }

    pub fn get_member_global_id(&self, member_id: &LuaMemberId) -> Option<&GlobalId> {
        self.member_global_id.get(member_id)
    }

    fn is_item_only_meta(&self, item: &LuaMemberIndexItem) -> bool {
        match item {
            LuaMemberIndexItem::One(id) => {
                if let Some(member) = self.get_member(id) {
                    return member.get_feature().is_meta_decl();
                }
            }
            LuaMemberIndexItem::Many(ids) => {
                for id in ids {
                    if let Some(member) = self.get_member(id) {
                        if !member.get_feature().is_meta_decl() {
                            return false;
                        }
                    }
                }
                return true;
            }
        }

        false
    }

    pub fn set_member_owner(
        &mut self,
        owner: LuaMemberOwner,
        file_id: FileId,
        id: LuaMemberId,
    ) -> Option<()> {
        self.member_current_owner.insert(id, owner.clone());
        self.add_in_file_object(file_id, MemberOrOwner::Owner(owner));

        Some(())
    }

    pub fn get_member(&self, id: &LuaMemberId) -> Option<&LuaMember> {
        self.members.get(id)
    }

    pub fn get_member_mut(&mut self, id: &LuaMemberId) -> Option<&mut LuaMember> {
        self.members.get_mut(id)
    }

    pub fn get_members(&self, owner: &LuaMemberOwner) -> Option<Vec<&LuaMember>> {
        let member_items = self.owner_members.get(owner)?;
        let mut members = Vec::new();
        for item in member_items.get_member_items() {
            match item {
                LuaMemberIndexItem::One(id) => {
                    if let Some(member) = self.get_member(id) {
                        members.push(member);
                    }
                }
                LuaMemberIndexItem::Many(ids) => {
                    for id in ids {
                        if let Some(member) = self.get_member(id) {
                            members.push(member);
                        }
                    }
                }
            }
        }

        Some(members)
    }

    #[allow(unused)]
    pub fn get_member_item_by_member_id(
        &self,
        member_id: LuaMemberId,
    ) -> Option<&LuaMemberIndexItem> {
        let owner = self.member_current_owner.get(&member_id)?;
        let member_key = self.members.get(&member_id)?.get_key();
        let member_items = self.owner_members.get(owner)?;
        let item = member_items.get_member(member_key)?;
        Some(item)
    }

    pub fn get_sorted_members(&self, owner: &LuaMemberOwner) -> Option<Vec<&LuaMember>> {
        let mut members = self.get_members(owner)?;
        members.sort_by_key(|member| member.get_sort_key());
        Some(members)
    }

    pub fn get_member_item(
        &self,
        owner: &LuaMemberOwner,
        key: &LuaMemberKey,
    ) -> Option<&LuaMemberIndexItem> {
        self.owner_members
            .get(owner)
            .and_then(|map| map.get_member(key))
    }

    pub fn get_member_len(&self, owner: &LuaMemberOwner) -> usize {
        self.owner_members
            .get(owner)
            .map_or(0, |map| map.get_member_len())
    }

    pub fn get_current_owner(&self, id: &LuaMemberId) -> Option<&LuaMemberOwner> {
        self.member_current_owner.get(id)
    }
}

impl LuaIndex for LuaMemberIndex {
    fn remove(&mut self, file_id: FileId) {
        if let Some(member_ids) = self.in_filed.remove(&file_id) {
            let mut owners = HashSet::new();
            for member_id_or_owner in member_ids {
                match member_id_or_owner {
                    MemberOrOwner::Member(member_id) => {
                        self.members.remove(&member_id);
                        self.member_current_owner.remove(&member_id);
                        self.member_global_id.remove(&member_id);
                    }
                    MemberOrOwner::Owner(owner) => {
                        owners.insert(owner);
                    }
                }
            }


            let mut need_removed_owner = Vec::new();
            for owner in owners {
                if let Some(member_items) = self.owner_members.get_mut(&owner) {
                    let mut need_removed_key = Vec::new();
                    for (key, item) in member_items.iter_mut() {
                        match item {
                            LuaMemberIndexItem::One(id) => {
                                if id.file_id == file_id {
                                    need_removed_key.push(key.clone());
                                }
                            }
                            LuaMemberIndexItem::Many(ids) => {
                                ids.retain(|id| id.file_id != file_id);
                                if ids.is_empty() {
                                    need_removed_key.push(key.clone());
                                }
                            }
                        }
                    }

                    for key in need_removed_key {
                        member_items.remove_member(&key);
                    }

                    if member_items.is_empty() {
                        need_removed_owner.push(owner);
                    }
                }
            }

            for owner in need_removed_owner {
                self.owner_members.remove(&owner);
            }
        }
    }

    fn clear(&mut self) {
        self.members.clear();
        self.in_filed.clear();
        self.owner_members.clear();
        self.member_current_owner.clear();
        self.member_global_id.clear();
    }
}
