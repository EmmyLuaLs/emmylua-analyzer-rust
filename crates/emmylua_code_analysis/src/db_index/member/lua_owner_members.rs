use hashbrown::HashMap;

use crate::{LuaMemberIndexItem, LuaMemberKey};

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct LuaOwnerMembers {
    members: HashMap<LuaMemberKey, LuaMemberIndexItem>,
    resolve_state: OwnerMemberStatus,
}

#[allow(unused)]
impl LuaOwnerMembers {
    pub fn new() -> Self {
        Self {
            members: HashMap::new(),
            resolve_state: OwnerMemberStatus::UnResolved,
        }
    }

    pub fn add_member(&mut self, key: LuaMemberKey, item: LuaMemberIndexItem) {
        self.members.insert(key, item);
    }

    pub fn get_member(&self, key: &LuaMemberKey) -> Option<&LuaMemberIndexItem> {
        self.members.get(key)
    }

    pub fn contains_member(&self, key: &LuaMemberKey) -> bool {
        self.members.contains_key(key)
    }

    pub fn get_member_len(&self) -> usize {
        self.members.len()
    }

    pub fn get_member_mut(&mut self, key: &LuaMemberKey) -> Option<&mut LuaMemberIndexItem> {
        self.members.get_mut(key)
    }

    pub fn get_member_items(&self) -> impl Iterator<Item = &LuaMemberIndexItem> {
        self.members.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&LuaMemberKey, &mut LuaMemberIndexItem)> {
        self.members.iter_mut()
    }

    pub fn remove_member(&mut self, key: &LuaMemberKey) -> Option<LuaMemberIndexItem> {
        self.members.remove(key)
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn set_resolved(&mut self) {
        self.resolve_state = OwnerMemberStatus::Resolved;
    }

    pub fn set_unresolved(&mut self) {
        self.resolve_state = OwnerMemberStatus::UnResolved;
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self.resolve_state, OwnerMemberStatus::Resolved)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerMemberStatus {
    UnResolved,
    Resolved,
}
