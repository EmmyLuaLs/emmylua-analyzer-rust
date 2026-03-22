use emmylua_parser::{
    LuaAstNode, LuaBlock, LuaComment, LuaKind, LuaStat, LuaSyntaxKind, LuaSyntaxNode,
};
use rowan::TextRange;

use crate::ir::{self, AlignEntry, DocIR};

use super::FormatContext;
use super::comment::{extract_trailing_comment, format_comment, format_trailing_comment};
use super::statement::{format_stat, format_stat_eq_split, is_eq_alignable};
use super::trivia::count_blank_lines_before;

/// A collected block child for two-pass processing
enum BlockChild {
    Comment(LuaComment),
    Statement(LuaStat),
}

impl BlockChild {
    fn syntax(&self) -> &LuaSyntaxNode {
        match self {
            BlockChild::Comment(c) => c.syntax(),
            BlockChild::Statement(s) => s.syntax(),
        }
    }
}

fn same_stat_kind(left: &LuaStat, right: &LuaStat) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn should_break_on_blank_lines(child: &BlockChild) -> bool {
    count_blank_lines_before(child.syntax()) > 0
}

fn can_join_comment_alignment_group(
    ctx: &FormatContext,
    anchor: &LuaStat,
    child: &BlockChild,
) -> bool {
    if should_break_on_blank_lines(child) {
        return false;
    }

    match child {
        BlockChild::Comment(_) => ctx.config.comments.align_across_standalone_comments,
        BlockChild::Statement(next_stat) => {
            if extract_trailing_comment(ctx.config, next_stat.syntax()).is_none() {
                return false;
            }
            if ctx.config.comments.align_same_kind_only && !same_stat_kind(anchor, next_stat) {
                return false;
            }
            true
        }
    }
}

fn can_join_eq_alignment_group(ctx: &FormatContext, anchor: &LuaStat, child: &BlockChild) -> bool {
    if should_break_on_blank_lines(child) {
        return false;
    }

    match child {
        BlockChild::Comment(_) => ctx.config.comments.align_across_standalone_comments,
        BlockChild::Statement(next_stat) => {
            if !is_eq_alignable(ctx.config, next_stat) {
                return false;
            }
            if ctx.config.comments.align_same_kind_only && !same_stat_kind(anchor, next_stat) {
                return false;
            }
            true
        }
    }
}

fn build_eq_alignment_entries(
    ctx: &FormatContext,
    children: &[BlockChild],
    consumed_comment_ranges: &mut Vec<TextRange>,
) -> Vec<AlignEntry> {
    let mut entries = Vec::new();

    for child in children {
        match child {
            BlockChild::Comment(comment) => {
                if consumed_comment_ranges
                    .iter()
                    .any(|range| *range == comment.syntax().text_range())
                {
                    continue;
                }
                entries.push(AlignEntry::Line {
                    content: format_comment(ctx.config, comment),
                    trailing: None,
                });
            }
            BlockChild::Statement(stat) => {
                let trailing = if ctx.config.should_align_statement_line_comments() {
                    extract_trailing_comment(ctx.config, stat.syntax()).map(
                        |(trail_docs, range)| {
                            consumed_comment_ranges.push(range);
                            trail_docs
                        },
                    )
                } else {
                    None
                };

                if let Some((before, mut after)) = format_stat_eq_split(ctx, stat) {
                    if trailing.is_none()
                        && let Some((trailing_ir, range)) =
                            format_trailing_comment(ctx.config, stat.syntax())
                    {
                        after.push(trailing_ir);
                        consumed_comment_ranges.push(range);
                    }
                    entries.push(AlignEntry::Aligned {
                        before,
                        after,
                        trailing,
                    });
                } else {
                    let mut content = format_stat(ctx, stat);
                    if trailing.is_none()
                        && let Some((trailing_ir, range)) =
                            format_trailing_comment(ctx.config, stat.syntax())
                    {
                        content.push(trailing_ir);
                        consumed_comment_ranges.push(range);
                    }
                    entries.push(AlignEntry::Line { content, trailing });
                }
            }
        }
    }

    entries
}

fn build_comment_alignment_entries(
    ctx: &FormatContext,
    children: &[BlockChild],
    consumed_comment_ranges: &mut Vec<TextRange>,
) -> Vec<AlignEntry> {
    let mut entries = Vec::new();

    for child in children {
        match child {
            BlockChild::Comment(comment) => {
                if consumed_comment_ranges
                    .iter()
                    .any(|range| *range == comment.syntax().text_range())
                {
                    continue;
                }
                entries.push(AlignEntry::Line {
                    content: format_comment(ctx.config, comment),
                    trailing: None,
                });
            }
            BlockChild::Statement(stat) => {
                let trailing = extract_trailing_comment(ctx.config, stat.syntax()).map(
                    |(trail_docs, range)| {
                        consumed_comment_ranges.push(range);
                        trail_docs
                    },
                );
                entries.push(AlignEntry::Line {
                    content: format_stat(ctx, stat),
                    trailing,
                });
            }
        }
    }

    entries
}

/// Format a block (statement list + blank line normalization + comment handling).
///
/// Iterates all child nodes of the Block (including Statements and Comments),
/// processing them in their original CST order.
/// When `=` alignment is enabled, consecutive alignable statements are grouped
/// into an AlignGroup IR node so the Printer can align their `=` signs.
pub fn format_block(ctx: &FormatContext, block: &LuaBlock) -> Vec<DocIR> {
    let children: Vec<BlockChild> = block
        .syntax()
        .children()
        .filter_map(|child| match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                LuaComment::cast(child).map(BlockChild::Comment)
            }
            _ => LuaStat::cast(child).map(BlockChild::Statement),
        })
        .collect();

    let mut docs: Vec<DocIR> = Vec::new();
    let mut is_first = true;
    let mut consumed_comment_ranges: Vec<TextRange> = Vec::new();
    let mut i = 0;

    while i < children.len() {
        match &children[i] {
            BlockChild::Comment(comment) => {
                if consumed_comment_ranges
                    .iter()
                    .any(|r| *r == comment.syntax().text_range())
                {
                    i += 1;
                    continue;
                }

                if !is_first {
                    let blank_lines = count_blank_lines_before(comment.syntax());
                    let normalized = blank_lines.min(ctx.config.layout.max_blank_lines);
                    for _ in 0..normalized {
                        docs.push(ir::hard_line());
                    }
                }

                docs.extend(format_comment(ctx.config, comment));

                if !is_first || !docs.is_empty() {
                    docs.push(ir::hard_line());
                }
                is_first = false;
                i += 1;
            }
            BlockChild::Statement(stat) => {
                // Try to form an alignment group if enabled
                if ctx.config.align.continuous_assign_statement && is_eq_alignable(ctx.config, stat)
                {
                    let group_start = i;
                    let mut group_end = i + 1;
                    while group_end < children.len() {
                        if can_join_eq_alignment_group(ctx, stat, &children[group_end]) {
                            group_end += 1;
                        } else {
                            break;
                        }
                    }

                    let stmt_count = children[group_start..group_end]
                        .iter()
                        .filter(|child| matches!(child, BlockChild::Statement(_)))
                        .count();

                    if stmt_count >= 2 {
                        // Emit = alignment group
                        if !is_first {
                            let blank_lines =
                                count_blank_lines_before(children[group_start].syntax());
                            let normalized = blank_lines.min(ctx.config.layout.max_blank_lines);
                            for _ in 0..normalized {
                                docs.push(ir::hard_line());
                            }
                        }

                        let entries = build_eq_alignment_entries(
                            ctx,
                            &children[group_start..group_end],
                            &mut consumed_comment_ranges,
                        );

                        docs.push(ir::align_group(entries));
                        docs.push(ir::hard_line());
                        is_first = false;
                        i = group_end;
                        continue;
                    }
                }

                // Try to form a comment-only alignment group
                if ctx.config.should_align_statement_line_comments()
                    && extract_trailing_comment(ctx.config, stat.syntax()).is_some()
                {
                    let group_start = i;
                    let mut group_end = i + 1;
                    while group_end < children.len() {
                        if can_join_comment_alignment_group(ctx, stat, &children[group_end]) {
                            group_end += 1;
                        } else {
                            break;
                        }
                    }

                    let stmt_count = children[group_start..group_end]
                        .iter()
                        .filter(|c| matches!(c, BlockChild::Statement(_)))
                        .count();

                    if stmt_count >= 2 {
                        if !is_first {
                            let blank_lines =
                                count_blank_lines_before(children[group_start].syntax());
                            let normalized = blank_lines.min(ctx.config.layout.max_blank_lines);
                            for _ in 0..normalized {
                                docs.push(ir::hard_line());
                            }
                        }

                        let entries = build_comment_alignment_entries(
                            ctx,
                            &children[group_start..group_end],
                            &mut consumed_comment_ranges,
                        );

                        docs.push(ir::align_group(entries));
                        docs.push(ir::hard_line());
                        is_first = false;
                        i = group_end;
                        continue;
                    }
                }

                // Normal (non-aligned) statement
                if !is_first {
                    let blank_lines = count_blank_lines_before(stat.syntax());
                    let normalized = blank_lines.min(ctx.config.layout.max_blank_lines);
                    for _ in 0..normalized {
                        docs.push(ir::hard_line());
                    }
                }

                let stat_docs = format_stat(ctx, stat);
                docs.extend(stat_docs);

                if let Some((trailing_ir, range)) =
                    format_trailing_comment(ctx.config, stat.syntax())
                {
                    docs.push(trailing_ir);
                    consumed_comment_ranges.push(range);
                }

                if !is_first || !docs.is_empty() {
                    docs.push(ir::hard_line());
                }
                is_first = false;
                i += 1;
            }
        }
    }

    // Remove trailing excess HardLines
    while matches!(docs.last(), Some(DocIR::HardLine)) {
        docs.pop();
    }

    docs
}
