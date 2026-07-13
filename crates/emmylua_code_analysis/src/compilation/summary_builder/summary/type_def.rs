//! Workspace type index — 跨文件类型定义聚合。

use smol_str::SmolStr;

use super::SalsaDocTypeDefKindSummary;
use crate::FileId;
use crate::compilation::SalsaDocVisibilityKindSummary;

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct TypeDefEntry {
    pub file_id: FileId,
    pub name: SmolStr,
    pub kind: SalsaDocTypeDefKindSummary,
    pub visibility: SalsaDocVisibilityKindSummary,
    pub syntax_offset: rowan::TextSize,
}

/// Workspace-aggregated type index.
///
/// | 条件 | 行为 |
/// |------|------|
/// | class + `(partial)` | 允许多文件定义 |
/// | class + meta 文件 | 同 workspace 内覆盖 |
/// | enum | 禁止多文件定义 |
/// | alias | 禁止多文件定义 |
///
/// 性能：重建 ∝ 总类型定义数（< 1000）。每个文件已 salsa memoized。
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct WorkspaceTypeIndex {
    pub by_name: Vec<(SmolStr, Vec<TypeDefEntry>)>,
    pub by_file: Vec<(FileId, Vec<SmolStr>)>,
}

impl WorkspaceTypeIndex {
    /// 根据类型名查找所有定义。
    pub fn find(&self, type_name: &str) -> Option<&[TypeDefEntry]> {
        self.by_name
            .iter()
            .find(|(n, _)| n.as_str() == type_name)
            .map(|(_, e)| e.as_slice())
    }

    pub fn count_files(&self, type_name: &str) -> usize {
        self.find(type_name).map(|e| e.len()).unwrap_or(0)
    }

    /// 获取某文件中定义的所有类型名。
    pub fn find_by_file(&self, file_id: FileId) -> &[SmolStr] {
        self.by_file
            .iter()
            .find(|(f, _)| *f == file_id)
            .map(|(_, names)| names.as_slice())
            .unwrap_or(&[])
    }
}
