use emmylua_parser::LuaTokenKind;

use crate::ir::{self, DocIR};

#[derive(Clone)]
pub enum SequenceEntry {
    Item(Vec<DocIR>),
    Comment(Vec<DocIR>),
    Separator { docs: Vec<DocIR>, space_after: bool },
}

pub fn comma_entry() -> SequenceEntry {
    SequenceEntry::Separator {
        docs: vec![ir::syntax_token(LuaTokenKind::TkComma)],
        space_after: true,
    }
}

pub fn render_sequence(docs: &mut Vec<DocIR>, entries: &[SequenceEntry], mut line_start: bool) {
    let mut needs_space_before_item = false;

    for entry in entries {
        match entry {
            SequenceEntry::Item(item_docs) => {
                if !line_start && needs_space_before_item {
                    docs.push(ir::space());
                }
                docs.extend(item_docs.clone());
                line_start = false;
                needs_space_before_item = false;
            }
            SequenceEntry::Comment(comment_docs) => {
                if !line_start {
                    docs.push(ir::hard_line());
                }
                docs.extend(comment_docs.clone());
                docs.push(ir::hard_line());
                line_start = true;
                needs_space_before_item = false;
            }
            SequenceEntry::Separator {
                docs: separator_docs,
                space_after,
            } => {
                docs.extend(separator_docs.clone());
                line_start = false;
                needs_space_before_item = *space_after;
            }
        }
    }
}

pub fn sequence_has_comment(entries: &[SequenceEntry]) -> bool {
    entries
        .iter()
        .any(|entry| matches!(entry, SequenceEntry::Comment(_)))
}

pub fn sequence_ends_with_comment(entries: &[SequenceEntry]) -> bool {
    matches!(entries.last(), Some(SequenceEntry::Comment(_)))
}

pub fn sequence_starts_with_comment(entries: &[SequenceEntry]) -> bool {
    matches!(entries.first(), Some(SequenceEntry::Comment(_)))
}
