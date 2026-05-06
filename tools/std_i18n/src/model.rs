use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntrySelector {
    Desc,
    Param { name: String },
    Return { index: usize },
    ReturnItem { index: usize, value: String },
    Field { name: String },
    Item { value: String },
}

impl EntrySelector {
    pub fn encode(&self) -> String {
        match self {
            Self::Desc => "d".to_string(),
            Self::Param { name } => format!("p:{name}"),
            Self::Return { index } => format!("r:{index}"),
            Self::ReturnItem { index, value } => format!("ri:{index}:{value}"),
            Self::Field { name } => format!("f:{name}"),
            Self::Item { value } => format!("i:{value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedEntry {
    pub locale_key: String,
    pub selector: EntrySelector,
    /// 该条目所属注释块在文件中的范围。
    pub comment_span: SourceSpan,
    /// 源码中的原始描述文本（未做 preprocess），用于定位行内替换目标。
    pub raw: String,
    /// 预处理后的描述文本（用于输出 YAML 的英文原文对照）。
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedComment {
    pub span: SourceSpan,
    pub raw: String,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedDocFile {
    pub comments: Vec<ExtractedComment>,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedStdFile {
    pub path: PathBuf,
    pub content: String,
    pub line_starts: Vec<usize>,
    pub entries: Vec<ExtractedEntry>,
    pub targets: Vec<ReplaceTarget>,
}

#[derive(Debug, Clone)]
pub struct ReplaceTarget {
    pub locale_key: String,
    pub comment_span: SourceSpan,
    pub selector: EntrySelector,
    pub start: usize,
    pub end: usize,
    pub strategy: ReplaceStrategy,
}

#[derive(Debug, Clone)]
pub enum ReplaceStrategy {
    DocBlock { indent: String },
    LineCommentTail { prefix: String },
}
