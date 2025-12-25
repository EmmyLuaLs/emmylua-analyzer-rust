use crate::comment_syntax::{
    LineInfo, build_tag_line_indexes, is_doc_tag_line, normalize_optional_name,
    split_lines_with_offsets,
};
use crate::extractor::analyze_lua_doc_file;
use crate::model::{ExtractedEntry, ExtractedKind, SourceSpan};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct ReplaceTarget {
    pub key: String,
    pub start: usize,
    pub end: usize,
    pub strategy: ReplaceStrategy,
}

/// 基于 analyzer 的精确 span，为单文件计算“可替换的目标列表”。
///
/// 注意：这里不关心具体翻译文本，仅计算每个 key 对应的替换范围和替换策略。
pub(crate) fn compute_replace_targets(
    content: &str,
    file_name: &str,
    include_empty: bool,
) -> Vec<ReplaceTarget> {
    let analyzed = analyze_lua_doc_file(file_name, content, include_empty);

    let mut targets: Vec<ReplaceTarget> = Vec::new();
    let mut used_spans: HashSet<(usize, usize)> = HashSet::new();

    let mut comment_ctx: HashMap<SourceSpan, CommentReplaceContext> = HashMap::new();
    for c in analyzed.comments {
        let ctx = CommentReplaceContext::new(c.raw);
        comment_ctx.insert(c.span, ctx);
    }

    for entry in analyzed.entries {
        let Some((start, end, strategy)) = compute_replace_target(content, &comment_ctx, &entry)
        else {
            continue;
        };
        if !used_spans.insert((start, end)) {
            continue;
        }
        targets.push(ReplaceTarget {
            key: entry.locale_key,
            start,
            end,
            strategy,
        });
    }

    targets.sort_by_key(|t| t.start);
    targets
}

#[derive(Debug, Clone)]
pub(crate) enum ReplaceStrategy {
    DocBlock { indent: String },
    LineCommentTail { prefix: String },
}

struct CommentReplaceContext {
    raw: String,
    lines: Vec<LineInfo>,
    indexes: crate::comment_syntax::TagLineIndexes,
}

impl CommentReplaceContext {
    fn new(raw: String) -> Self {
        let lines = split_lines_with_offsets(&raw);
        let indexes = build_tag_line_indexes(&raw, &lines);
        Self {
            raw,
            lines,
            indexes,
        }
    }
}

fn compute_replace_target(
    file_content: &str,
    ctx_map: &HashMap<SourceSpan, CommentReplaceContext>,
    entry: &ExtractedEntry,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let ctx = ctx_map.get(&entry.comment_span)?;
    match &entry.kind {
        ExtractedKind::Desc => desc_replace_target(ctx, entry.comment_span),
        ExtractedKind::Param { name } => tag_attached_replace_target_for_param(
            ctx,
            entry.comment_span,
            name,
            &entry.raw,
            file_content,
        ),
        ExtractedKind::Return { index } => tag_attached_replace_target_for_return(
            ctx,
            entry.comment_span,
            *index,
            &entry.raw,
            file_content,
        ),
        ExtractedKind::Field { name } => tag_attached_replace_target_for_field(
            ctx,
            entry.comment_span,
            name,
            &entry.raw,
            file_content,
        ),
        ExtractedKind::Item { value } => union_item_replace_target(ctx, entry.comment_span, value),
        ExtractedKind::ReturnItem { value, .. } => {
            union_item_replace_target(ctx, entry.comment_span, value)
        }
    }
}

fn desc_replace_target(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let (rel_start, rel_end) = if let Some((start, end)) = ctx.indexes.desc_block {
        let start_off = ctx.lines.get(start)?.start;
        let end_off = if end < ctx.lines.len() {
            ctx.lines.get(end)?.start
        } else {
            ctx.raw.len()
        };
        (start_off, end_off)
    } else {
        (0, 0)
    };

    let start = comment_span.start + rel_start;
    let end = comment_span.start + rel_end;
    Some((
        start,
        end,
        ReplaceStrategy::DocBlock {
            indent: ctx.indexes.default_indent.clone(),
        },
    ))
}

fn tag_attached_replace_target_for_param(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    name: &str,
    raw_desc: &str,
    file_content: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let name = normalize_optional_name(name);
    let tag_idx = *ctx.indexes.param_line.get(&name)?;

    tag_attached_replace_target_after(ctx, comment_span, tag_idx, raw_desc, file_content)
}

fn tag_attached_replace_target_for_field(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    name: &str,
    raw_desc: &str,
    file_content: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let name = normalize_optional_name(name);
    let tag_idx = *ctx.indexes.field_line.get(&name)?;
    tag_attached_replace_target_after(ctx, comment_span, tag_idx, raw_desc, file_content)
}

fn tag_attached_replace_target_for_return(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    index: usize,
    raw_desc: &str,
    file_content: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let tag_idx = ctx
        .indexes
        .return_lines
        .get(index.saturating_sub(1))
        .copied()?;
    tag_attached_replace_target_after(ctx, comment_span, tag_idx, raw_desc, file_content)
}

fn tag_attached_replace_target_after(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    tag_idx: usize,
    raw_desc: &str,
    file_content: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    if let Some(inline) =
        inline_tag_description_replace_target(ctx, comment_span, tag_idx, raw_desc)
    {
        return Some(inline);
    }

    if raw_desc.trim().is_empty()
        && let Some(insert) = inline_tag_description_insert_target(ctx, comment_span, tag_idx)
    {
        return Some(insert);
    }

    attached_doc_block_target_after(ctx, comment_span, tag_idx, file_content)
}

fn inline_tag_description_replace_target(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    tag_idx: usize,
    raw_desc: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let desc = raw_desc.trim();
    if desc.is_empty() || desc.contains('\n') {
        return None;
    }
    let li = *ctx.lines.get(tag_idx)?;
    let line_text = li.text(&ctx.raw);
    let pos = line_text.rfind(desc)?;
    let start = comment_span.start + li.start + pos;
    let end = start + desc.len();
    Some((
        start,
        end,
        ReplaceStrategy::LineCommentTail {
            prefix: "".to_string(),
        },
    ))
}

fn inline_tag_description_insert_target(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    tag_idx: usize,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let li = *ctx.lines.get(tag_idx)?;
    let line_text = li.text(&ctx.raw);
    let start = comment_span.start + li.end;
    let prefix = if line_text.ends_with(|c: char| c.is_whitespace()) {
        ""
    } else {
        " "
    };
    Some((
        start,
        start,
        ReplaceStrategy::LineCommentTail {
            prefix: prefix.to_string(),
        },
    ))
}

fn attached_doc_block_target_after(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    tag_idx: usize,
    file_content: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let indent = ctx.lines.get(tag_idx)?.indent(&ctx.raw);

    let mut start = tag_idx + 1;
    while start < ctx.lines.len() && ctx.lines[start].text(&ctx.raw).trim().is_empty() {
        start += 1;
    }

    let mut end = start;
    while end < ctx.lines.len() {
        let t = ctx.lines[end].trim_start_text(&ctx.raw);
        if is_doc_tag_line(t) || t.starts_with("---|") {
            break;
        }
        if t.starts_with("---") {
            end += 1;
            continue;
        }
        break;
    }

    let (abs_start, abs_end) = if start < end {
        let rel_s = ctx.lines.get(start)?.start;
        let rel_e = if end < ctx.lines.len() {
            ctx.lines.get(end)?.start
        } else {
            ctx.raw.len()
        };
        (comment_span.start + rel_s, comment_span.start + rel_e)
    } else {
        let rel_insert = if start < ctx.lines.len() {
            ctx.lines.get(start)?.start
        } else {
            ctx.raw.len()
        };
        let mut abs_insert = comment_span.start + rel_insert;
        if abs_insert == comment_span.end {
            abs_insert = advance_past_line_break(file_content, abs_insert);
        }
        (abs_insert, abs_insert)
    };

    Some((abs_start, abs_end, ReplaceStrategy::DocBlock { indent }))
}

fn union_item_replace_target(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    value: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let line_idx = ctx.indexes.union_line.get(value).copied()?;
    if ctx.lines.is_empty() {
        return None;
    }
    let li = ctx.lines.get(line_idx.min(ctx.lines.len() - 1))?;
    let line_text = li.text(&ctx.raw);
    if let Some(hash_pos) = line_text.find('#') {
        let start = comment_span.start + li.start + hash_pos + 1;
        let end = comment_span.start + li.end;
        Some((
            start,
            end,
            ReplaceStrategy::LineCommentTail {
                prefix: " ".to_string(),
            },
        ))
    } else {
        let start = comment_span.start + li.end;
        Some((
            start,
            start,
            ReplaceStrategy::LineCommentTail {
                prefix: " # ".to_string(),
            },
        ))
    }
}

fn advance_past_line_break(s: &str, offset: usize) -> usize {
    let bytes = s.as_bytes();
    if offset < bytes.len() && bytes[offset] == b'\r' {
        if offset + 1 < bytes.len() && bytes[offset + 1] == b'\n' {
            return offset + 2;
        }
        return offset + 1;
    }
    if offset < bytes.len() && bytes[offset] == b'\n' {
        return offset + 1;
    }
    offset
}
