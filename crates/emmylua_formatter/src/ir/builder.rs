use smol_str::SmolStr;
use std::rc::Rc;

use emmylua_parser::{LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind};

use super::{AlignEntry, AlignGroupData, DocIR, GroupId};

pub fn text(s: impl Into<SmolStr>) -> DocIR {
    DocIR::Text(s.into())
}

pub fn source_node(node: LuaSyntaxNode) -> DocIR {
    DocIR::SourceNode {
        node,
        trim_end: false,
    }
}

pub fn source_node_trimmed(node: LuaSyntaxNode) -> DocIR {
    DocIR::SourceNode {
        node,
        trim_end: true,
    }
}

pub fn source_token(token: LuaSyntaxToken) -> DocIR {
    DocIR::SourceToken(token)
}

pub fn syntax_token(kind: LuaTokenKind) -> DocIR {
    DocIR::SyntaxToken(kind)
}

pub fn space() -> DocIR {
    DocIR::Space
}

pub fn hard_line() -> DocIR {
    DocIR::HardLine
}

pub fn soft_line() -> DocIR {
    DocIR::SoftLine
}

pub fn soft_line_or_empty() -> DocIR {
    DocIR::SoftLineOrEmpty
}

pub fn group(docs: Vec<DocIR>) -> DocIR {
    DocIR::Group {
        contents: docs,
        should_break: false,
        id: None,
    }
}

pub fn group_break(docs: Vec<DocIR>) -> DocIR {
    DocIR::Group {
        contents: docs,
        should_break: true,
        id: None,
    }
}

pub fn group_with_id(docs: Vec<DocIR>, id: GroupId) -> DocIR {
    DocIR::Group {
        contents: docs,
        should_break: false,
        id: Some(id),
    }
}

pub fn indent(docs: Vec<DocIR>) -> DocIR {
    DocIR::Indent(docs)
}

pub fn list(docs: Vec<DocIR>) -> DocIR {
    DocIR::List(docs)
}

pub fn if_break(break_doc: DocIR, flat_doc: DocIR) -> DocIR {
    DocIR::IfBreak {
        break_contents: Rc::new(break_doc),
        flat_contents: Rc::new(flat_doc),
        group_id: None,
    }
}

pub fn if_break_with_group(break_doc: DocIR, flat_doc: DocIR, group_id: GroupId) -> DocIR {
    DocIR::IfBreak {
        break_contents: Rc::new(break_doc),
        flat_contents: Rc::new(flat_doc),
        group_id: Some(group_id),
    }
}

pub fn fill(parts: Vec<DocIR>) -> DocIR {
    DocIR::Fill { parts }
}

pub fn line_suffix(docs: Vec<DocIR>) -> DocIR {
    DocIR::LineSuffix(docs)
}

/// Insert separators between elements
pub fn intersperse(docs: Vec<Vec<DocIR>>, separator: Vec<DocIR>) -> Vec<DocIR> {
    let mut result = Vec::with_capacity(docs.len() * 2);
    for (i, doc) in docs.into_iter().enumerate() {
        if i > 0 {
            result.extend(separator.clone());
        }
        result.extend(doc);
    }
    result
}

/// Flatten multiple DocIR fragments into a single Vec
pub fn concat(items: impl IntoIterator<Item = DocIR>) -> Vec<DocIR> {
    items.into_iter().collect()
}

/// Build an alignment group from a list of entries
pub fn align_group(entries: Vec<AlignEntry>) -> DocIR {
    DocIR::AlignGroup(Rc::new(AlignGroupData { entries }))
}
