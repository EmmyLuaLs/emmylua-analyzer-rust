use std::rc::Rc;

use smol_str::SmolStr;

/// Group identifier for querying break state across groups
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub(crate) u32);

/// Formatting intermediate representation
#[derive(Debug, Clone)]
pub enum DocIR {
    /// Raw text fragment
    Text(SmolStr),

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
    Aligned {
        before: Vec<DocIR>,
        after: Vec<DocIR>,
    },
    /// A non-aligned line (e.g., standalone comment) kept in sequence
    Line(Vec<DocIR>),
}

/// Compute the flat (single-line) width of an IR slice.
/// Only handles simple nodes (Text, Space, List); other nodes contribute 0.
/// This is safe for alignment `before` parts which are always flat.
pub fn ir_flat_width(docs: &[DocIR]) -> usize {
    docs.iter()
        .map(|d| match d {
            DocIR::Text(s) => s.len(),
            DocIR::Space => 1,
            DocIR::List(items) => ir_flat_width(items),
            _ => 0,
        })
        .sum()
}
