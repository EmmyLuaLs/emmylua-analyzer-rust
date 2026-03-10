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

/// Format a block (statement list + blank line normalization + comment handling).
///
/// Iterates all child nodes of the Block (including Statements and Comments),
/// processing them in their original CST order.
/// When `=` alignment is enabled, consecutive alignable statements are grouped
/// into an AlignGroup IR node so the Printer can align their `=` signs.
pub fn format_block(ctx: &FormatContext, block: &LuaBlock) -> Vec<DocIR> {
    // Pass 1: collect all children
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

    // Pass 2: emit IR, grouping consecutive alignable statements
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
                    let normalized = blank_lines.min(ctx.config.max_blank_lines);
                    for _ in 0..normalized {
                        docs.push(ir::hard_line());
                    }
                }

                docs.extend(format_comment(comment));

                if !is_first || !docs.is_empty() {
                    docs.push(ir::hard_line());
                }
                is_first = false;
                i += 1;
            }
            BlockChild::Statement(stat) => {
                // Try to form an alignment group if enabled
                if ctx.config.align_continuous_assign_statement && is_eq_alignable(stat) {
                    let group_start = i;
                    let mut group_end = i + 1;

                    // Scan forward for consecutive alignable statements (no blank lines between).
                    // Skip interleaved Comment children (they're trailing comments consumed later).
                    while group_end < children.len() {
                        match &children[group_end] {
                            BlockChild::Statement(next_stat) => {
                                if is_eq_alignable(next_stat) {
                                    let blank_lines = count_blank_lines_before(next_stat.syntax());
                                    if blank_lines == 0 {
                                        group_end += 1;
                                        continue;
                                    }
                                }
                                break;
                            }
                            BlockChild::Comment(_) => {
                                // Skip trailing comment nodes when scanning for alignment group
                                group_end += 1;
                                continue;
                            }
                        }
                    }

                    if group_end - group_start >= 2 {
                        // Emit = alignment group
                        if !is_first {
                            let blank_lines =
                                count_blank_lines_before(children[group_start].syntax());
                            let normalized = blank_lines.min(ctx.config.max_blank_lines);
                            for _ in 0..normalized {
                                docs.push(ir::hard_line());
                            }
                        }

                        let mut entries = Vec::new();
                        for child in children.iter().take(group_end).skip(group_start) {
                            if let BlockChild::Statement(s) = child {
                                // Extract trailing comment for IR-level alignment
                                let trailing = if ctx.config.align_continuous_line_comment {
                                    extract_trailing_comment(s.syntax()).map(
                                        |(trail_docs, range)| {
                                            consumed_comment_ranges.push(range);
                                            trail_docs
                                        },
                                    )
                                } else {
                                    None
                                };

                                if let Some((before, mut after)) = format_stat_eq_split(ctx, s) {
                                    // When not using trailing alignment, attach as LineSuffix
                                    if trailing.is_none()
                                        && let Some((trailing_ir, range)) =
                                            format_trailing_comment(s.syntax())
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
                                    let mut content = format_stat(ctx, s);
                                    if trailing.is_none()
                                        && let Some((trailing_ir, range)) =
                                            format_trailing_comment(s.syntax())
                                    {
                                        content.push(trailing_ir);
                                        consumed_comment_ranges.push(range);
                                    }
                                    entries.push(AlignEntry::Line { content, trailing });
                                }
                            }
                        }

                        docs.push(ir::align_group(entries));
                        docs.push(ir::hard_line());
                        is_first = false;
                        i = group_end;
                        continue;
                    }
                }

                // Try to form a comment-only alignment group
                if ctx.config.align_continuous_line_comment
                    && extract_trailing_comment(stat.syntax()).is_some()
                {
                    let group_start = i;
                    let mut group_end = i + 1;
                    while group_end < children.len() {
                        match &children[group_end] {
                            BlockChild::Statement(next_stat) => {
                                let blank_lines = count_blank_lines_before(next_stat.syntax());
                                if blank_lines > 0 {
                                    break;
                                }
                                if extract_trailing_comment(next_stat.syntax()).is_some() {
                                    group_end += 1;
                                } else {
                                    break;
                                }
                            }
                            BlockChild::Comment(_) => {
                                group_end += 1;
                                continue;
                            }
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
                            let normalized = blank_lines.min(ctx.config.max_blank_lines);
                            for _ in 0..normalized {
                                docs.push(ir::hard_line());
                            }
                        }

                        let mut entries = Vec::new();
                        for child in children.iter().take(group_end).skip(group_start) {
                            if let BlockChild::Statement(s) = child {
                                let trailing = extract_trailing_comment(s.syntax()).map(
                                    |(trail_docs, range)| {
                                        consumed_comment_ranges.push(range);
                                        trail_docs
                                    },
                                );
                                entries.push(AlignEntry::Line {
                                    content: format_stat(ctx, s),
                                    trailing,
                                });
                            }
                        }

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
                    let normalized = blank_lines.min(ctx.config.max_blank_lines);
                    for _ in 0..normalized {
                        docs.push(ir::hard_line());
                    }
                }

                let stat_docs = format_stat(ctx, stat);
                docs.extend(stat_docs);

                if let Some((trailing_ir, range)) = format_trailing_comment(stat.syntax()) {
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
