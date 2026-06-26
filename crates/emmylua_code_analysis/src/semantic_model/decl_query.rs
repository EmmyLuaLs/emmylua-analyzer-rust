//! DeclQuery — 声明查询
//!
//! 提供声明相关的集中查询接口：声明树、声明引用、声明类型、声明范围、
//! 声明属性、成员 key 转换、可见性检查等。

use std::sync::Arc;

use emmylua_parser::{LuaIndexKey, LuaSyntaxNode, LuaSyntaxToken};
use rowan::TextSize;
use smol_str::SmolStr;

use crate::compilation::{
    SalsaDeclId, SalsaDeclTreeSummary, SalsaDocTypeDefKindSummary, SalsaDocVisibilityKindSummary,
    SalsaNameUseSummary, SalsaPropertySummary, SalsaSummaryDatabase,
};
use crate::semantic_model::InferCache;
use crate::semantic_model::infer::InferQuery;
use crate::semantic_model::offset_types::DeclPosition;
use crate::{
    Emmyrc, FileId, LuaMemberKey, LuaSemanticDeclId, LuaType, LuaTypeDeclId, SemanticDeclLevel,
};

use super::{reference, visibility};

/// 声明查询器。
pub struct DeclQuery<'db> {
    db: &'db SalsaSummaryDatabase,
    file_id: FileId,
    emmyrc: Arc<Emmyrc>,
    root: emmylua_parser::LuaChunk,
    infer_cache: std::cell::RefCell<InferCache>,
}

impl<'db> DeclQuery<'db> {
    pub(crate) fn new(
        db: &'db SalsaSummaryDatabase,
        file_id: FileId,
        emmyrc: Arc<Emmyrc>,
        root: emmylua_parser::LuaChunk,
        infer_cache: std::cell::RefCell<InferCache>,
    ) -> Self {
        Self {
            db,
            file_id,
            emmyrc,
            root,
            infer_cache,
        }
    }

    fn db(&self) -> &SalsaSummaryDatabase {
        &self.db
    }

    fn infer(&self) -> InferQuery<'_> {
        InferQuery::with_cache(
            self.db,
            self.file_id,
            self.emmyrc.clone(),
            &self.infer_cache,
            self.root.clone(),
        )
    }

    /// 获取当前文件的声明树。
    pub fn tree(&self) -> Option<Arc<SalsaDeclTreeSummary>> {
        let db = self.db();
        db.file().decl_tree(self.file_id)
    }

    /// 查询某个声明的所有 name 引用。
    pub fn references(&self, decl_id: SalsaDeclId) -> Option<Vec<SalsaNameUseSummary>> {
        let db = self.db();
        db.lexical().decl_references(self.file_id, decl_id)
    }

    /// 通过声明位置查找声明的 range。
    pub fn range(&self, position: DeclPosition) -> Option<rowan::TextRange> {
        let db = self.db();
        let pos: TextSize = position.into();
        let tree = db.file().decl_tree(self.file_id)?;
        tree.decls
            .iter()
            .find(|d| d.id.as_text_size() == pos)
            .map(|decl| rowan::TextRange::new(decl.start_offset, decl.end_offset))
    }

    /// 通过声明位置获取声明的类型信息。
    pub fn type_at(&self, position: DeclPosition) -> Option<LuaType> {
        let db = self.db();
        let pos: TextSize = position.into();
        // Path 1: use site resolution (works for references to named types)
        if let Some(name_info) = db.types().name(self.file_id, pos) {
            if let Some(dt) = name_info.decl_type {
                if let Some(ty) = self.infer().resolve_decl_type(&db, dt) {
                    return Some(ty);
                }
            }
        }
        // Path 2: look up decl by position in decl tree, then get its type
        if let Some(tree) = db.file().decl_tree(self.file_id) {
            if let Some(decl) = tree.decls.iter().find(|d| d.id.as_text_size() == pos) {
                if let Some(dt) = db.types().decl(self.file_id, decl.id) {
                    if let Some(ty) = self.infer().resolve_decl_type(&db, dt) {
                        return Some(ty);
                    }
                }
            }
        }
        // Path 3: fallback — check type_defs for @class/@enum near this position
        self.resolve_nearby_class(db, pos)
    }

    /// Fallback: when the owner-binding chain doesn't link @class to the decl,
    /// look for type_defs whose owner is near this position.
    fn resolve_nearby_class(&self, db: &SalsaSummaryDatabase, position: TextSize) -> Option<LuaType> {
        let doc = db.doc().summary(self.file_id)?;
        for type_def in &doc.type_defs {
            if let Some(owner_offset) = type_def.owner.syntax_offset {
                let dist = if owner_offset > position {
                    u32::from(owner_offset) - u32::from(position)
                } else {
                    u32::from(position) - u32::from(owner_offset)
                };
                if dist < 100
                    && matches!(
                        type_def.kind,
                        SalsaDocTypeDefKindSummary::Class | SalsaDocTypeDefKindSummary::Enum
                    )
                {
                    let type_id = LuaTypeDeclId::global(type_def.name.as_str());
                    return Some(LuaType::Ref(type_id));
                }
            }
        }
        None
    }

    /// 查找节点引用的声明，同时返回声明是否覆盖整个节点。
    pub fn find_covers_node(
        &self,
        node: LuaSyntaxNode,
        level: SemanticDeclLevel,
    ) -> Option<(LuaSemanticDeclId, bool)> {
        let db = self.db();
        reference::find_decl_covers_node(&db, self.file_id, &node, level)
    }

    /// 查找 AST 节点引用的声明。
    pub fn find_by_node(
        &self,
        node: LuaSyntaxNode,
        level: SemanticDeclLevel,
    ) -> Option<LuaSemanticDeclId> {
        let db = self.db();
        reference::find_decl(&db, self.file_id, &node, level)
    }

    /// 检查 AST 节点是否是对目标声明的引用。
    pub fn is_reference_to(
        &self,
        node: LuaSyntaxNode,
        decl_id: &LuaSemanticDeclId,
        level: SemanticDeclLevel,
    ) -> Option<bool> {
        let db = self.db();
        reference::is_reference_to(&db, self.file_id, &node, decl_id, level)
    }

    /// 判断声明在给定 token 位置是否可见。
    pub fn is_visible(
        &self,
        token: LuaSyntaxToken,
        decl_id: &LuaSemanticDeclId,
        visibility: Option<&SalsaDocVisibilityKindSummary>,
    ) -> Option<bool> {
        let db = self.db();
        let infer = self.infer();
        visibility::check_visibility(
            &db,
            &infer,
            self.file_id,
            &self.emmyrc,
            token,
            decl_id,
            visibility,
        )
    }

    /// 查询某个语义声明的属性（salsa-native 替代 get_property_index）。
    pub fn property(&self, decl_id: &LuaSemanticDeclId) -> Option<Vec<SalsaPropertySummary>> {
        let db = self.db();
        match decl_id {
            LuaSemanticDeclId::LuaDecl(ld) => {
                let sid = SalsaDeclId(DeclPosition(ld.position));
                db.file().properties_for_decl(self.file_id, sid)
            }
            LuaSemanticDeclId::Member(_) => None,
            _ => None,
        }
    }

    pub fn member_key(&self, field_key: LuaIndexKey) -> Option<LuaMemberKey> {
        match field_key {
            LuaIndexKey::Name(token) => {
                Some(LuaMemberKey::Name(SmolStr::new(token.get_name_text())))
            }
            LuaIndexKey::String(token) => Some(LuaMemberKey::Name(SmolStr::new(token.get_value()))),
            LuaIndexKey::Integer(token) => {
                let val = match token.get_number_value() {
                    emmylua_parser::NumberResult::Int(i) => i,
                    emmylua_parser::NumberResult::Uint(u) => u as i64,
                    emmylua_parser::NumberResult::Float(f) => f as i64,
                    emmylua_parser::NumberResult::Number => 0,
                };
                Some(LuaMemberKey::Integer(val))
            }
            _ => None,
        }
    }
}
