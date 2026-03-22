use emmylua_parser::LuaTokenKind;

use crate::config::ExpandStrategy;
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

#[derive(Clone)]
pub struct DelimitedSequenceLayout {
    pub open: DocIR,
    pub close: DocIR,
    pub items: Vec<Vec<DocIR>>,
    pub strategy: ExpandStrategy,
    pub preserve_multiline: bool,
    pub flat_separator: Vec<DocIR>,
    pub fill_separator: Vec<DocIR>,
    pub break_separator: Vec<DocIR>,
    pub flat_open_padding: Vec<DocIR>,
    pub flat_close_padding: Vec<DocIR>,
    pub grouped_padding: DocIR,
    pub flat_trailing: Vec<DocIR>,
    pub grouped_trailing: DocIR,
    pub custom_break_contents: Option<Vec<DocIR>>,
    pub prefer_custom_break_in_auto: bool,
}

pub fn format_delimited_sequence(layout: DelimitedSequenceLayout) -> Vec<DocIR> {
    if layout.items.is_empty() {
        return vec![layout.open, layout.close];
    }

    let flat_inner = ir::intersperse(layout.items.clone(), layout.flat_separator.clone());
    let fill_inner = ir::fill(build_fill_parts(&layout.items, &layout.fill_separator));
    let break_inner = ir::intersperse(layout.items, layout.break_separator);
    let flat_doc = build_flat_doc(
        &layout.open,
        &layout.close,
        &layout.flat_open_padding,
        flat_inner,
        &layout.flat_trailing,
        &layout.flat_close_padding,
    );
    let break_contents = layout
        .custom_break_contents
        .unwrap_or_else(|| default_break_contents(break_inner, layout.grouped_trailing.clone()));

    match layout.strategy {
        ExpandStrategy::Never => flat_doc,
        ExpandStrategy::Always => {
            format_expanded_delimited_sequence(layout.open, layout.close, break_contents)
        }
        ExpandStrategy::Auto if layout.preserve_multiline => {
            format_expanded_delimited_sequence(layout.open, layout.close, break_contents)
        }
        ExpandStrategy::Auto if layout.prefer_custom_break_in_auto => {
            let gid = ir::next_group_id();
            let break_doc = ir::list(vec![
                layout.open,
                ir::indent(break_contents),
                ir::hard_line(),
                layout.close,
            ]);
            vec![ir::group_with_id(
                vec![ir::if_break_with_group(break_doc, ir::list(flat_doc), gid)],
                gid,
            )]
        }
        ExpandStrategy::Auto => vec![ir::group(vec![
            layout.open,
            ir::indent(vec![
                layout.grouped_padding.clone(),
                fill_inner,
                layout.grouped_trailing,
            ]),
            layout.grouped_padding,
            layout.close,
        ])],
    }
}

fn format_expanded_delimited_sequence(open: DocIR, close: DocIR, inner: Vec<DocIR>) -> Vec<DocIR> {
    vec![ir::group_break(vec![
        open,
        ir::indent(inner),
        ir::hard_line(),
        close,
    ])]
}

fn default_break_contents(inner: Vec<DocIR>, trailing: DocIR) -> Vec<DocIR> {
    vec![ir::hard_line(), ir::list(inner), trailing]
}

fn build_flat_doc(
    open: &DocIR,
    close: &DocIR,
    open_padding: &[DocIR],
    inner: Vec<DocIR>,
    trailing: &[DocIR],
    close_padding: &[DocIR],
) -> Vec<DocIR> {
    let mut docs = vec![open.clone()];
    docs.extend(open_padding.to_vec());
    docs.extend(inner);
    docs.extend(trailing.to_vec());
    docs.extend(close_padding.to_vec());
    docs.push(close.clone());
    docs
}

fn build_fill_parts(items: &[Vec<DocIR>], separator: &[DocIR]) -> Vec<DocIR> {
    let mut parts = Vec::with_capacity(items.len().saturating_mul(2));

    for (index, item) in items.iter().enumerate() {
        parts.push(ir::list(item.clone()));
        if index + 1 < items.len() {
            parts.push(ir::list(separator.to_vec()));
        }
    }

    parts
}
