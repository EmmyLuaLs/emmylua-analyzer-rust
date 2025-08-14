mod property;

use std::collections::{HashMap, HashSet};

use emmylua_parser::{LuaAstNode, LuaDocTagField, LuaDocType, LuaVersionCondition, VisibilityKind};
pub use property::LuaCommonProperty;
pub use property::{LuaDeprecated, LuaExport, LuaExportScope, LuaPropertyId};

use crate::{
    DbIndex, FileId, LuaMember, LuaSignatureId, db_index::property::property::LuaTagContent,
};

use super::{LuaSemanticDeclId, traits::LuaIndex};

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
        mut see_content: String,
        see_description: Option<String>,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        let tag_content = property
            .tag_content
            .get_or_insert_with(|| Box::new(LuaTagContent::new()));

        if let Some(see_description) = see_description {
            see_content += " ";
            see_content += &see_description;
        }

        tag_content.add_tag("see".into(), see_content);

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
        tag_name: String,
        other_content: String,
    ) -> Option<()> {
        let property = self.get_or_create_property(owner_id.clone())?;
        let tag_content = property
            .tag_content
            .get_or_insert_with(|| Box::new(LuaTagContent::new()));
        tag_content.add_tag(tag_name, other_content);

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

/// 尝试从 @field 定义中提取函数类型的位置信息
pub fn try_extract_signature_id_from_field(
    db: &DbIndex,
    member: &LuaMember,
) -> Option<LuaSignatureId> {
    // 检查是否是 field 定义
    if !member.is_field() {
        return None;
    }

    let root = db
        .get_vfs()
        .get_syntax_tree(&member.get_file_id())?
        .get_red_root();
    let field_node = member.get_syntax_id().to_node_from_root(&root)?;

    // 尝试转换为 LuaDocTagField
    let field_tag = LuaDocTagField::cast(field_node)?;

    // 获取类型定义
    let type_node = field_tag.get_type()?;

    match &type_node {
        LuaDocType::Func(doc_func) => Some(LuaSignatureId::from_doc_func(
            member.get_file_id(),
            &doc_func,
        )),
        _ => None,
    }
}
