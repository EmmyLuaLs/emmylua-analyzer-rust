mod property;
use std::collections::{HashMap, HashSet};

use emmylua_parser::{LuaVersionCondition, VisibilityKind};
use property::LuaProperty;
pub use property::{LuaPropertyId, LuaPropertyOwnerId};

use crate::FileId;

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct LuaPropertyIndex {
    properties: HashMap<LuaPropertyId, LuaProperty>,
    property_owners_map: HashMap<LuaPropertyOwnerId, LuaPropertyId>,

    id_count: u32,
    in_filed_owner: HashMap<FileId, HashSet<LuaPropertyOwnerId>>,
}

impl LuaPropertyIndex {
    pub fn new() -> Self {
        Self {
            id_count: 0,
            in_filed_owner: HashMap::new(),
            properties: HashMap::new(),
            property_owners_map: HashMap::new(),
        }
    }

    fn get_or_create_property(&mut self, owner_id: LuaPropertyOwnerId) -> Option<&mut LuaProperty> {
        if let Some(property_id) = self.property_owners_map.get(&owner_id) {
            self.properties.get_mut(property_id)
        } else {
            let id = LuaPropertyId::new(self.id_count);
            self.id_count += 1;
            self.property_owners_map.insert(owner_id.clone(), id);
            self.properties.insert(id, LuaProperty::new(id.clone()));
            self.properties.get_mut(&id)
        }
    }

    pub fn add_owner_map(
        &mut self,
        source_owner_id: LuaPropertyOwnerId,
        same_property_owner_id: LuaPropertyOwnerId,
        file_id: FileId,
    ) -> Option<()> {
        let property_id = self
            .get_or_create_property(source_owner_id.clone())?
            .id
            .clone();
        self.property_owners_map
            .insert(same_property_owner_id, property_id);

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(source_owner_id);

        Some(())
    }

    pub fn add_description(
        &mut self,
        file_id: FileId,
        owner_id: LuaPropertyOwnerId,
        description: String,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.description = Some(Box::new(description));

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_visibility(
        &mut self,
        file_id: FileId,
        owner_id: LuaPropertyOwnerId,
        visibility: VisibilityKind,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.visibility = Some(visibility);

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_source(
        &mut self,
        file_id: FileId,
        owner_id: LuaPropertyOwnerId,
        source: String,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.source = Some(Box::new(source));

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_nodiscard(&mut self, file_id: FileId, owner_id: LuaPropertyOwnerId) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.is_nodiscard = true;

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_deprecated(
        &mut self,
        file_id: FileId,
        owner_id: LuaPropertyOwnerId,
        message: Option<String>,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.is_deprecated = true;
        property.deprecated_message = message.map(Box::new);

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_version(
        &mut self,
        file_id: FileId,
        owner_id: LuaPropertyOwnerId,
        version_conds: Vec<LuaVersionCondition>,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.version_conds = Some(Box::new(version_conds));

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_async(&mut self, file_id: FileId, owner_id: LuaPropertyOwnerId) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.is_async = true;

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn get_property(&self, owner_id: LuaPropertyOwnerId) -> Option<&LuaProperty> {
        self.property_owners_map
            .get(&owner_id)
            .and_then(|id| self.properties.get(id))
    }
}

impl LuaIndex for LuaPropertyIndex {
    fn remove(&mut self, file_id: FileId) {
        if let Some(property_owner_ids) = self.in_filed_owner.remove(&file_id) {
            for property_owner_id in property_owner_ids {
                if let Some(property_id) = self.property_owners_map.remove(&property_owner_id) {
                    self.properties.remove(&property_id);
                }
            }
        }
    }

    fn clear(&mut self) {
        self.properties.clear();
        self.property_owners_map.clear();
        self.in_filed_owner.clear();
        self.id_count = 0;
    }
}
