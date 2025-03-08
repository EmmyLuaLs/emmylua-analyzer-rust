mod lua_member;

use std::collections::HashMap;

use crate::FileId;
pub use lua_member::{LuaMember, LuaMemberId, LuaMemberKey, LuaMemberOwner};

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct LuaMemberIndex {
    members: HashMap<LuaMemberId, LuaMember>,
    in_field_members: HashMap<FileId, Vec<LuaMemberId>>,
    owner_members: HashMap<LuaMemberOwner, HashMap<LuaMemberKey, LuaMemberId>>,
}

impl LuaMemberIndex {
    pub fn new() -> Self {
        Self {
            members: HashMap::new(),
            in_field_members: HashMap::new(),
            owner_members: HashMap::new(),
        }
    }

    pub fn add_member(&mut self, member: LuaMember) -> LuaMemberId {
        let id = member.get_id();
        let owner = member.get_owner();
        let file_id = member.get_file_id();
        self.in_field_members
            .entry(file_id)
            .or_insert_with(Vec::new)
            .push(id);
        self.members.insert(id, member);

        if !owner.is_none() {
            self.add_member_owner(owner.clone(), id);
            self.add_member_to_owner(owner, id);
        }
        id
    }

    pub fn add_member_to_owner(&mut self, owner: LuaMemberOwner, id: LuaMemberId) -> Option<()> {
        let member = self.members.get(&id)?;
        let key = member.get_key().clone();
        let member_map = self.owner_members.entry(owner).or_insert_with(HashMap::new);
        if !member_map.contains_key(&key) {
            member_map.insert(key, id);
        }

        Some(())
    }

    pub fn add_member_owner(&mut self, owner: LuaMemberOwner, id: LuaMemberId) -> Option<()> {
        let member = self.members.get_mut(&id)?;
        member.owner = owner.clone();

        Some(())
    }

    pub fn get_member(&self, id: &LuaMemberId) -> Option<&LuaMember> {
        self.members.get(id)
    }

    pub fn get_member_mut(&mut self, id: &LuaMemberId) -> Option<&mut LuaMember> {
        self.members.get_mut(id)
    }

    pub fn get_member_map(
        &self,
        owner: LuaMemberOwner,
    ) -> Option<&HashMap<LuaMemberKey, LuaMemberId>> {
        self.owner_members.get(&owner)
    }

    pub fn get_member_from_owner(
        &self,
        owner: &LuaMemberOwner,
        key: &LuaMemberKey,
    ) -> Option<&LuaMember> {
        if let Some(member_id) = self.owner_members.get(owner)?.get(key) {
            self.members.get(member_id)
        } else {
            None
        }
    }
}

impl LuaIndex for LuaMemberIndex {
    fn remove(&mut self, file_id: FileId) {
        if let Some(member_ids) = self.in_field_members.remove(&file_id) {
            for member_id in member_ids {
                if let Some(member) = self.members.remove(&member_id) {
                    let owner = member.get_owner();
                    let key = member.get_key();
                    if let Some(owner_members) = self.owner_members.get_mut(&owner) {
                        owner_members.remove(&key);
                        if owner_members.is_empty() {
                            self.owner_members.remove(&owner);
                        }
                    }
                }
            }
        }
    }

    fn clear(&mut self) {
        self.members.clear();
        self.in_field_members.clear();
        self.owner_members.clear();
    }
}
