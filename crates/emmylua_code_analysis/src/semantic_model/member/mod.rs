//! 成员查询模块
//!
//! 封装 `SalsaSummaryDatabase` 的成员相关查询，提供类型安全的接口。

use std::sync::Arc;

use crate::FileId;
use crate::compilation::SalsaSummaryDatabase;
use crate::compilation::{
    SalsaMemberIndexSummary, SalsaMemberSummary, SalsaMemberTargetId, SalsaMemberTypeInfoSummary,
    SalsaMemberUseSummary, SalsaSyntaxIdSummary,
};

/// 成员查询器。通过 `SemanticModel::members()` 获取。
///
/// 所有查询都经过 salsa，自动享受增量计算和缓存。
pub struct MemberQuery<'db> {
    db: &'db SalsaSummaryDatabase,
    file_id: FileId,
}

impl<'db> MemberQuery<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase, file_id: FileId) -> Self {
        Self { db, file_id }
    }

    fn db(&self) -> &SalsaSummaryDatabase {
        &self.db
    }

    /// 获取当前文件的所有成员索引。
    pub fn all(&self) -> Option<Arc<SalsaMemberIndexSummary>> {
        self.db().file().members(self.file_id)
    }

    /// 获取当前文件的所有成员列表（克隆）。
    pub fn list(&self) -> Option<Vec<SalsaMemberSummary>> {
        self.all().map(|index| index.members.clone())
    }

    /// 根据语法 ID 查找成员。
    pub fn by_syntax_id(&self, syntax_id: SalsaSyntaxIdSummary) -> Option<SalsaMemberSummary> {
        self.db()
            .file()
            .member_by_syntax_id(self.file_id, syntax_id)
    }

    /// 获取某个成员的类型信息。
    pub fn type_of(
        &self,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<SalsaMemberTypeInfoSummary> {
        self.db().types().member(self.file_id, member_target)
    }

    /// 查找成员的所有引用。
    pub fn references_of(
        &self,
        member_target: impl Into<SalsaMemberTargetId>,
    ) -> Option<Vec<SalsaMemberUseSummary>> {
        self.db()
            .lexical()
            .member_references(self.file_id, member_target)
    }
}
