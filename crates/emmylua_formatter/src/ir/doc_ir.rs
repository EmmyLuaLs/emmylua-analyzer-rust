use std::rc::Rc;

use emmylua_parser::{LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind};
use rowan::{SyntaxText, TextSize};
use smol_str::SmolStr;

/// Group identifier for querying break state across groups
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub(crate) u32);

/// Formatting intermediate representation
#[derive(Debug, Clone)]
pub enum DocIR {
    /// Raw text fragment
    Text(SmolStr),

    /// Raw source text emitted directly from an existing syntax node.
    SourceNode { node: LuaSyntaxNode, trim_end: bool },

    /// Raw source text emitted directly from an existing syntax token.
    SourceToken(LuaSyntaxToken),

    /// Stable syntax token emitted from LuaTokenKind
    SyntaxToken(LuaTokenKind),

    /// Hard line break — always emits a newline regardless of line width
    HardLine,

    /// Soft line break — becomes a newline when the Group is broken, otherwise a space
    SoftLine,

    /// Soft line break (no space) — becomes a newline when the Group is broken, otherwise nothing
    SoftLineOrEmpty,

    /// Fixed space
    Space,

    /// Indent wrapper — contents are indented one level
    Indent(Vec<DocIR>),

    /// Group — the Printer tries to fit all contents on one line;
    ///         if it exceeds line width, breaks and all SoftLines become newlines
    Group {
        contents: Vec<DocIR>,
        should_break: bool,
        id: Option<GroupId>,
    },

    /// List — directly concatenates multiple IRs
    List(Vec<DocIR>),

    /// Conditional branch — selects different output based on whether the Group is broken
    IfBreak {
        break_contents: Rc<DocIR>,
        flat_contents: Rc<DocIR>,
        group_id: Option<GroupId>,
    },

    /// Fill — greedy fill: places as many elements on one line as the line width allows
    Fill { parts: Vec<DocIR> },

    /// Line suffix — output at the end of the current line (for trailing comments)
    LineSuffix(Vec<DocIR>),

    /// Alignment group — consecutive entries whose alignment points are padded to the same column.
    /// The Printer pads each entry's `before` to the max width so `after` parts line up.
    AlignGroup(Rc<AlignGroupData>),
}

/// Data for an alignment group (behind Rc to keep DocIR enum small)
#[derive(Debug, Clone)]
pub struct AlignGroupData {
    pub entries: Vec<AlignEntry>,
}

/// Type alias for an eq-split pair: (before_docs, after_docs)
pub type EqSplit = (Vec<DocIR>, Vec<DocIR>);

/// A single entry in an alignment group
#[derive(Debug, Clone)]
pub enum AlignEntry {
    /// A line split at the alignment point.
    /// `before` is padded to the max width across the group, then `after` is appended.
    /// `trailing` (if present) is a trailing comment aligned to a common column.
    Aligned {
        before: Vec<DocIR>,
        after: Vec<DocIR>,
        trailing: Option<Vec<DocIR>>,
    },
    /// A non-aligned line (e.g., standalone comment or non-= statement with trailing comment)
    Line {
        content: Vec<DocIR>,
        trailing: Option<Vec<DocIR>>,
    },
}

/// Compute the flat (single-line) width of an IR slice.
///
/// This follows the same rules the printer uses in flat mode so alignment logic
/// can estimate columns even when content contains nested groups or indents.
pub fn ir_flat_width(docs: &[DocIR]) -> usize {
    docs.iter()
        .map(|d| match d {
            DocIR::Text(s) => s.len(),
            DocIR::SourceNode { node, trim_end } => {
                let text = node.text();
                syntax_text_len(&text, *trim_end)
            }
            DocIR::SourceToken(token) => token.text().len(),
            DocIR::SyntaxToken(kind) => kind.syntax_text().map(str::len).unwrap_or(0),
            DocIR::HardLine => 0,
            DocIR::SoftLine => 1,
            DocIR::SoftLineOrEmpty => 0,
            DocIR::Space => 1,
            DocIR::Indent(items) => ir_flat_width(items),
            DocIR::Group { contents, .. } => ir_flat_width(contents),
            DocIR::List(items) => ir_flat_width(items),
            DocIR::IfBreak { flat_contents, .. } => {
                ir_flat_width(std::slice::from_ref(flat_contents.as_ref()))
            }
            DocIR::Fill { parts } => ir_flat_width(parts),
            DocIR::LineSuffix(_) => 0,
            DocIR::AlignGroup(group) => group
                .entries
                .iter()
                .map(|entry| match entry {
                    AlignEntry::Aligned {
                        before,
                        after,
                        trailing,
                    } => {
                        let mut width = ir_flat_width(before) + ir_flat_width(after);
                        if let Some(trail) = trailing {
                            width += 1 + ir_flat_width(trail);
                        }
                        width
                    }
                    AlignEntry::Line { content, trailing } => {
                        let mut width = ir_flat_width(content);
                        if let Some(trail) = trailing {
                            width += 1 + ir_flat_width(trail);
                        }
                        width
                    }
                })
                .max()
                .unwrap_or(0),
        })
        .sum()
}

pub fn syntax_text_len(text: &SyntaxText, trim_end: bool) -> usize {
    let len = text.len();
    let end = if trim_end {
        syntax_text_trimmed_end(text)
    } else {
        len
    };

    let width: u32 = end.into();
    width as usize
}

pub fn syntax_text_trimmed_end(text: &SyntaxText) -> TextSize {
    let mut trailing_len = 0usize;

    text.for_each_chunk(|chunk| {
        let trimmed_len = chunk.trim_end_matches(['\r', '\n', ' ', '\t']).len();
        if trimmed_len == chunk.len() {
            trailing_len = 0;
        } else if trimmed_len == 0 {
            trailing_len += chunk.len();
        } else {
            trailing_len = chunk.len() - trimmed_len;
        }
    });

    let trailing_size = TextSize::from(trailing_len as u32);
    text.len() - trailing_size
}
