//! 强类型 offset 定义
//!
//! 问题：`TextSize` 被用于多种不同的位置语义，导致无法在编译期区分：
//!   - 声明名 token 位置 (SalsaDeclId)
//!   - 文档所有者 AST 节点位置 (SalsaDocOwnerSummary.syntax_offset)
//!   - 源码语法节点位置 (LuaAstNode::get_position)
//!
//! 方案：用 newtype 包装 `TextSize`，每种语义一个类型，编译器强制区分。

use rowan::TextSize;

/// 声明名 token 的位置（用于 SalsaDeclId）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeclPosition(pub TextSize);

/// 文档所有者 AST 节点的位置（用于 SalsaDocOwnerSummary.syntax_offset）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OwnerPosition(pub TextSize);

/// 源码语法节点的位置（用于 SalsaSignatureSummary 等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SyntaxPosition(pub TextSize);

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 便捷方法
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

impl DeclPosition {
    pub fn as_u32(self) -> u32 {
        self.0.into()
    }
    pub fn to_text_size(self) -> TextSize {
        self.0
    }
}

impl OwnerPosition {
    pub fn as_u32(self) -> u32 {
        self.0.into()
    }
    pub fn to_text_size(self) -> TextSize {
        self.0
    }
}

impl SyntaxPosition {
    pub fn as_u32(self) -> u32 {
        self.0.into()
    }
    pub fn to_text_size(self) -> TextSize {
        self.0
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// From<TextSize> — 构造便捷
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

impl From<TextSize> for DeclPosition {
    fn from(t: TextSize) -> Self {
        DeclPosition(t)
    }
}

impl From<u32> for DeclPosition {
    fn from(v: u32) -> Self {
        DeclPosition(TextSize::from(v))
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// From<TextSize> for SyntaxPosition — 向后兼容
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

impl From<TextSize> for SyntaxPosition {
    fn from(t: TextSize) -> Self {
        SyntaxPosition(t)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Into<TextSize> — 显式转换
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

impl From<DeclPosition> for TextSize {
    fn from(p: DeclPosition) -> TextSize {
        p.0
    }
}
impl From<OwnerPosition> for TextSize {
    fn from(p: OwnerPosition) -> TextSize {
        p.0
    }
}
impl From<SyntaxPosition> for TextSize {
    fn from(p: SyntaxPosition) -> TextSize {
        p.0
    }
}
