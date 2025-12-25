use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractedKind {
    Desc,
    Param { name: String },
    Return { index: usize },
    ReturnItem { index: usize, value: String },
    Field { name: String },
    Item { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedEntry {
    pub key: String,
    pub locale_key: String,
    pub kind: ExtractedKind,
    /// 原始符号名（owner/class/alias），不做 module 映射。
    pub symbol: String,
    /// module 映射后的 base（用于生成 locale key）。
    pub base: String,
    /// 来自 `@version` 的后缀（含 `@`，例如 `@>5.2`）。
    pub version_suffix: Option<String>,
    /// 该条目所属注释块在文件中的范围。
    pub comment_span: SourceSpan,
    /// 源码中的原始描述文本（未做 preprocess），用于 translator 做行内替换定位。
    pub raw: String,
    /// 预处理后的描述文本（用于输出 YAML 的英文原文对照）。
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFile {
    pub path: PathBuf,
    /// 按源码顺序的注释块（每个注释块内含若干条目）。
    pub comments: Vec<ExtractedComment>,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedComment {
    pub span: SourceSpan,
    pub raw: String,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedLuaDocFile {
    pub module_map: HashMap<String, String>,
    pub comments: Vec<ExtractedComment>,
    pub entries: Vec<ExtractedEntry>,
}
