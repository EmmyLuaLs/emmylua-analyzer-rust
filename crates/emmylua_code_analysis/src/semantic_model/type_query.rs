//! TypeQuery — 类型定义查询
//!
//! 提供类型相关的集中查询接口：类型定义查找、属性查询、属性参数解析、
//! 类型成员索引、doc 类型名解析等。

use smol_str::SmolStr;

use crate::compilation::{
    SalsaDocOwnerKindSummary, SalsaDocOwnerSummary, SalsaDocTagPropertyEntrySummary,
    SalsaDocTagPropertySummary, SalsaDocTypeDefKindSummary, SalsaDocTypeDefSummary,
    SalsaDocTypeLoweredKind, SalsaDocVisibilityKindSummary, SalsaPropertyKeySummary,
    SalsaSummaryDatabase, TypeDefEntry, WorkspacePropertyEntry,
};
use crate::semantic_model::offset_types::OwnerPosition;
use crate::{FileId, LuaMemberKey, LuaType, LuaTypeDeclId};

/// 类型定义查询器。
pub struct TypeQuery<'db> {
    db: &'db SalsaSummaryDatabase,
    file_id: FileId,
}

impl<'db> TypeQuery<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase, file_id: FileId) -> Self {
        Self { db, file_id }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 类型定义查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取类型定义（class/enum/alias/attribute），跨文件查找。
    pub fn get_def(&self, name: &str) -> Option<SalsaDocTypeDefSummary> {
        self.get_def_with_file(name).map(|(def, _)| def)
    }

    /// 获取类型定义及所在文件 ID。
    pub fn get_def_with_file(&self, name: &str) -> Option<(SalsaDocTypeDefSummary, FileId)> {
        let db = self.db;
        if let Some(def) = db.doc().type_def_by_name(self.file_id, name) {
            return Some((def, self.file_id));
        }
        for fid in db.file_ids() {
            if let Some(def) = db.doc().type_def_by_name(fid, name) {
                return Some((def, fid));
            }
        }
        None
    }

    /// 判断类型 ID 是否指向 enum / class / alias / attribute。
    /// 跨文件查找：先在当前文件查，再遍历其他文件。
    pub fn get_kind(&self, type_id: &LuaTypeDeclId) -> Option<SalsaDocTypeDefKindSummary> {
        self.get_def_with_file(type_id.get_name())
            .map(|(def, _)| def.kind)
    }

    pub fn is_enum(&self, type_id: &LuaTypeDeclId) -> bool {
        matches!(
            self.get_kind(type_id),
            Some(SalsaDocTypeDefKindSummary::Enum)
        )
    }

    pub fn is_class(&self, type_id: &LuaTypeDeclId) -> bool {
        matches!(
            self.get_kind(type_id),
            Some(SalsaDocTypeDefKindSummary::Class)
        )
    }

    pub fn is_alias(&self, type_id: &LuaTypeDeclId) -> bool {
        matches!(
            self.get_kind(type_id),
            Some(SalsaDocTypeDefKindSummary::Alias)
        )
    }

    /// 统计使用某类型名的文件数（用于 duplicate type 检测）。
    pub fn count_files(&self, name: &str) -> usize {
        let db = self.db;
        db.semantic()
            .type_index()
            .map(|idx| idx.count_files(name))
            .unwrap_or(0)
    }

    /// 获取类型的跨文件定义条目列表。
    pub fn entries(&self, name: &str) -> Option<Vec<TypeDefEntry>> {
        let db = self.db;
        db.semantic()
            .type_index()
            .and_then(|idx| idx.find(name).map(|e| e.to_vec()))
    }

    /// 获取当前文件中定义的所有类型名。
    pub fn file_names(&self) -> Vec<String> {
        let db = self.db;
        db.semantic()
            .type_index()
            .map(|idx| {
                idx.find_by_file(self.file_id)
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 通过 doc type 名解析为 LuaType（salsa-native 替代 infer_doc_type）。
    pub fn resolve_name(&self, name: &str) -> Option<LuaType> {
        let db = self.db;
        let type_def = db.doc().type_def_by_name(self.file_id, name)?;
        let type_id = if matches!(type_def.visibility, SalsaDocVisibilityKindSummary::Private) {
            LuaTypeDeclId::file(self.file_id, name)
        } else {
            LuaTypeDeclId::global(name)
        };
        match type_def.kind {
            SalsaDocTypeDefKindSummary::Alias => Some(LuaType::Def(type_id)),
            _ => Some(LuaType::Ref(type_id)),
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 属性查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取成员属性条目（包含类型信息，跨文件）。
    pub fn property_entries(&self, type_name: &str) -> Option<Vec<WorkspacePropertyEntry>> {
        let db = self.db;
        db.semantic()
            .member_index()
            .and_then(|idx| idx.find(type_name).map(|e| e.to_vec()))
    }

    /// 获取类型的成员 key 列表（跨文件合并）。
    pub fn member_keys(&self, prefix_type: &LuaType) -> Option<Vec<LuaMemberKey>> {
        let db = self.db;
        get_member_keys_workspace(&db, prefix_type)
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // Doc 属性查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取声明的所有 doc tag 属性。
    pub fn doc_properties(
        &self,
        file_id: FileId,
        offset: OwnerPosition,
    ) -> Option<SalsaDocTagPropertySummary> {
        let db = self.db;
        let owner = SalsaDocOwnerSummary {
            kind: SalsaDocOwnerKindSummary::None,
            syntax_offset: Some(offset.into()),
        };
        db.doc().tag_property(file_id, owner)
    }

    /// 检查声明是否有指定的 doc tag 属性。
    pub fn has_doc_property(
        &self,
        file_id: FileId,
        offset: OwnerPosition,
        entry: SalsaDocTagPropertyEntrySummary,
    ) -> bool {
        let db = self.db;
        let owner = SalsaDocOwnerSummary {
            kind: SalsaDocOwnerKindSummary::None,
            syntax_offset: Some(offset.into()),
        };
        db.doc()
            .tag_property(file_id, owner)
            .is_some_and(|p| p.entries.contains(&entry))
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // Attribute 参数查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 获取 attribute 类型的参数定义。
    /// 返回 `Vec<(param_name, Option<param_type>)>`。
    pub fn attribute_params(
        &self,
        type_def: &SalsaDocTypeDefSummary,
    ) -> Option<Vec<(String, Option<LuaType>)>> {
        let db = self.db;
        let offset = type_def.value_type_offset?;
        let resolved = db.doc().resolved_type_by_key(self.file_id, offset)?;
        if let SalsaDocTypeLoweredKind::Function { params, .. } = &resolved.lowered.kind {
            let def_params: Vec<(String, Option<LuaType>)> = params
                .iter()
                .map(|p| {
                    (
                        p.name.clone().map(|n| n.to_string()).unwrap_or_default(),
                        None,
                    )
                })
                .collect();
            return Some(def_params);
        }
        None
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 内部工具
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn get_member_keys_workspace(
    db: &SalsaSummaryDatabase,
    prefix_type: &LuaType,
) -> Option<Vec<LuaMemberKey>> {
    match prefix_type {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            let entries = db.semantic().properties_of_type(type_id.get_name())?;
            let keys: Vec<LuaMemberKey> = entries
                .iter()
                .map(|e| match &e.key {
                    SalsaPropertyKeySummary::Name(n) => {
                        LuaMemberKey::Name(SmolStr::new(n.as_str()))
                    }
                    SalsaPropertyKeySummary::Integer(i) => LuaMemberKey::Integer(*i),
                    _ => LuaMemberKey::None,
                })
                .filter(|k| !matches!(k, LuaMemberKey::None))
                .collect();
            if keys.is_empty() { None } else { Some(keys) }
        }
        LuaType::Union(u) => {
            let mut all = Vec::new();
            for m in u.into_vec() {
                if let Some(keys) = get_member_keys_workspace(db, &m) {
                    all.extend(keys);
                }
            }
            if all.is_empty() { None } else { Some(all) }
        }
        _ => None,
    }
}
