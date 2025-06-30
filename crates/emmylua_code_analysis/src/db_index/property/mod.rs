mod property;

use std::collections::{HashMap, HashSet};

use emmylua_parser::{LuaVersionCondition, VisibilityKind};
use property::LuaCommonProperty;
pub use property::{LuaDeprecated, LuaExport, LuaExportScope, LuaPropertyId};

use crate::FileId;

use super::{traits::LuaIndex, LuaSemanticDeclId};

#[derive(Debug)]
pub struct LuaPropertyIndex {
    properties: HashMap<LuaPropertyId, LuaCommonProperty>,
    property_owners_map: HashMap<LuaSemanticDeclId, LuaPropertyId>,

    id_count: u32,
    in_filed_owner: HashMap<FileId, HashSet<LuaSemanticDeclId>>,
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

    fn get_or_create_property(
        &mut self,
        owner_id: LuaSemanticDeclId,
    ) -> Option<&mut LuaCommonProperty> {
        if let Some(property_id) = self.property_owners_map.get(&owner_id) {
            self.properties.get_mut(property_id)
        } else {
            let id = LuaPropertyId::new(self.id_count);
            self.id_count += 1;
            self.property_owners_map.insert(owner_id.clone(), id);
            self.properties
                .insert(id, LuaCommonProperty::new(id.clone()));
            self.properties.get_mut(&id)
        }
    }

    pub fn add_owner_map(
        &mut self,
        source_owner_id: LuaSemanticDeclId,
        same_property_owner_id: LuaSemanticDeclId,
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
        owner_id: LuaSemanticDeclId,
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
        owner_id: LuaSemanticDeclId,
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
        owner_id: LuaSemanticDeclId,
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

    pub fn add_deprecated(
        &mut self,
        file_id: FileId,
        owner_id: LuaSemanticDeclId,
        message: Option<String>,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.deprecated = match message {
            Some(msg) => Some(LuaDeprecated::DeprecatedWithMessage(Box::new(msg))),
            None => Some(LuaDeprecated::Deprecated),
        };

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_version(
        &mut self,
        file_id: FileId,
        owner_id: LuaSemanticDeclId,
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

    pub fn add_see(
        &mut self,
        file_id: FileId,
        owner_id: LuaSemanticDeclId,
        see_content: String,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.see_content = Some(Box::new(see_content));

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_other(
        &mut self,
        file_id: FileId,
        owner_id: LuaSemanticDeclId,
        other_content: String,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        if let Some(content) = &property.other_content {
            let mut content = content.clone();
            content.push_str("\n\n");
            content.push_str(&other_content);
            property.other_content = Some(content);
        } else {
            property.other_content = Some(other_content.into());
        }

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn add_export(
        &mut self,
        file_id: FileId,
        owner_id: LuaSemanticDeclId,
        export: property::LuaExport,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        property.export = Some(LuaExport {
            scope: export.scope,
        });

        self.in_filed_owner
            .entry(file_id)
            .or_insert_with(HashSet::new)
            .insert(owner_id);

        Some(())
    }

    pub fn get_property(&self, owner_id: &LuaSemanticDeclId) -> Option<&LuaCommonProperty> {
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
