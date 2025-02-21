mod lua_member;

use std::{
    collections::{hash_map, HashMap},
    sync::Arc,
};

use crate::FileId;
pub use lua_member::{LuaMember, LuaMemberId, LuaMemberKey, LuaMemberOwner};

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct LuaMemberIndex {
    members: HashMap<LuaMemberId, LuaMember>,
    in_field_members: HashMap<FileId, Vec<LuaMemberId>>,
    owner_members: HashMap<LuaMemberOwner, HashMap<LuaMemberKey, OneOrMulti>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OneOrMulti {
    One(LuaMemberId),
    Multi(Arc<Vec<LuaMemberId>>),
}

impl OneOrMulti {
    pub fn get_member_id(&self) -> &LuaMemberId {
        match self {
            OneOrMulti::One(id) => id,
            OneOrMulti::Multi(ids) => ids.first().unwrap(),
        }
    }
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
        let key = member.get_key().clone();
        if !owner.is_none() {
            let member_map = self.owner_members.entry(owner).or_insert_with(HashMap::new);

            match member_map.entry(key) {
                hash_map::Entry::Occupied(mut entry) => match entry.get_mut() {
                    OneOrMulti::One(old_id) => {
                        if *old_id != id {
                            *entry.into_mut() =
                                OneOrMulti::Multi(Arc::new(vec![old_id.clone(), id]));
                        }
                    }
                    OneOrMulti::Multi(ids) => {
                        Arc::get_mut(ids).and_then(|ids| {
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                            Some(())
                        });
                    }
                },
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(OneOrMulti::One(id));
                }
            }
        }
        let file_id = member.get_file_id();
        self.in_field_members
            .entry(file_id)
            .or_insert_with(Vec::new)
            .push(id);
        self.members.insert(id, member);
        id
    }

    pub fn add_member_owner(&mut self, owner: LuaMemberOwner, id: LuaMemberId) -> Option<()> {
        let member = self.members.get_mut(&id)?;
        let key = member.get_key().clone();
        member.owner = owner.clone();

        let member_map = self.owner_members.entry(owner).or_insert_with(HashMap::new);

        match member_map.entry(key) {
            hash_map::Entry::Occupied(mut entry) => match entry.get_mut() {
                OneOrMulti::One(old_id) => {
                    if *old_id != id {
                        *entry.into_mut() =
                            OneOrMulti::Multi(Arc::new(vec![old_id.clone(), id]));
                    }
                }
                OneOrMulti::Multi(ids) => {
                    Arc::get_mut(ids).and_then(|ids| {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                        Some(())
                    });
                }
            },
            hash_map::Entry::Vacant(entry) => {
                entry.insert(OneOrMulti::One(id));
            }
        }

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
    ) -> Option<&HashMap<LuaMemberKey, OneOrMulti>> {
        self.owner_members.get(&owner)
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

    fn fill_snapshot_info(&self, info: &mut HashMap<String, String>) {
        info.insert("member.members".to_string(), self.members.len().to_string());
        info.insert(
            "member.in_field_members".to_string(),
            self.in_field_members.len().to_string(),
        );
        info.insert(
            "member.owner_members".to_string(),
            self.owner_members.len().to_string(),
        );
    }
}
