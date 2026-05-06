use crate::diagnostics::Diagnostics;
use crate::doc_text::{DocBlockView, is_doc_continue_or_line, is_doc_tag_line};
use crate::model::{
    EntrySelector, ExtractedComment, ExtractedEntry, ReplaceStrategy, ReplaceTarget, SourceSpan,
};
use std::collections::{HashMap, HashSet};

/// 基于分析结果为单文件计算“可替换的目标列表”。
///
/// 注意：这里不关心具体翻译文本，仅计算每个 key 对应的替换范围和替换策略。
pub(crate) fn compute_replace_targets(
    file_label: &str,
    content: &str,
    comments: &[ExtractedComment],
    entries: &[ExtractedEntry],
    diagnostics: &mut Diagnostics,
) -> Vec<ReplaceTarget> {
    let mut targets: Vec<ReplaceTarget> = Vec::new();
    let mut used_spans: HashSet<(usize, usize)> = HashSet::new();

    let mut comment_ctx: HashMap<SourceSpan, CommentReplaceContext> = HashMap::new();
    for c in comments {
        let ctx = CommentReplaceContext::new(&c.raw);
        comment_ctx.insert(c.span, ctx);
    }

    for entry in entries {
        let Some((start, end, strategy)) = compute_replace_target(content, &comment_ctx, entry)
        else {
            diagnostics.missing_replacement_target(file_label, &entry.locale_key);
            continue;
        };
        if !used_spans.insert((start, end)) {
            continue;
        }
        targets.push(ReplaceTarget {
            locale_key: entry.locale_key.clone(),
            comment_span: entry.comment_span,
            selector: entry.selector.clone(),
            start,
            end,
            strategy,
        });
    }

    targets.sort_by_key(|t| t.start);
    targets
}

struct CommentReplaceContext {
    view: DocBlockView,
}

impl CommentReplaceContext {
    fn new(raw: &str) -> Self {
        Self {
            view: DocBlockView::new(raw),
        }
    }

    fn raw(&self) -> &str {
        &self.view.raw
    }
}

fn compute_replace_target(
    file_content: &str,
    ctx_map: &HashMap<SourceSpan, CommentReplaceContext>,
    entry: &ExtractedEntry,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let ctx = ctx_map.get(&entry.comment_span)?;
    match &entry.selector {
        EntrySelector::Desc => desc_replace_target(ctx, entry.comment_span),
        EntrySelector::Param { name } => tag_attached_replace_target_for_param(
            ctx,
            entry.comment_span,
            name,
            &entry.raw,
            file_content,
        ),
        EntrySelector::Return { index } => tag_attached_replace_target_for_return(
            ctx,
            entry.comment_span,
            *index,
            &entry.raw,
            file_content,
        ),
        EntrySelector::Field { name } => tag_attached_replace_target_for_field(
            ctx,
            entry.comment_span,
            name,
            &entry.raw,
            file_content,
        ),
        EntrySelector::Item { value } => union_item_replace_target(ctx, entry.comment_span, value),
        EntrySelector::ReturnItem { value, .. } => {
            union_item_replace_target(ctx, entry.comment_span, value)
        }
    }
}

fn desc_replace_target(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let (rel_start, rel_end) = if let Some((start, end)) = ctx.view.indexes.desc_block {
        let start_off = ctx.view.lines.get(start)?.start;
        let end_off = if end < ctx.view.lines.len() {
            ctx.view.lines.get(end)?.start
        } else {
            ctx.raw().len()
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
            indent: ctx.view.indexes.default_indent.clone(),
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
    let name = crate::doc_text::normalize_optional_name(name);
    let tag_idx = *ctx.view.indexes.param_line.get(&name)?;

    tag_attached_replace_target_after(ctx, comment_span, tag_idx, raw_desc, file_content)
}

fn tag_attached_replace_target_for_field(
    ctx: &CommentReplaceContext,
    comment_span: SourceSpan,
    name: &str,
    raw_desc: &str,
    file_content: &str,
) -> Option<(usize, usize, ReplaceStrategy)> {
    let name = crate::doc_text::normalize_optional_name(name);
    let tag_idx = *ctx.view.indexes.field_line.get(&name)?;
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
        .view
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
    let li = *ctx.view.lines.get(tag_idx)?;
    let line_text = li.text(ctx.raw());
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
    let li = *ctx.view.lines.get(tag_idx)?;
    let line_text = li.text(ctx.raw());
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
    let indent = ctx.view.lines.get(tag_idx)?.indent(ctx.raw());

    let mut start = tag_idx + 1;
    while start < ctx.view.lines.len() && ctx.view.lines[start].text(ctx.raw()).trim().is_empty() {
        start += 1;
    }

    let mut end = start;
    while end < ctx.view.lines.len() {
        let t = ctx.view.lines[end].trim_start_text(ctx.raw());
        if is_doc_tag_line(t) || is_doc_continue_or_line(t) {
            break;
        }
        if crate::doc_text::is_doc_comment_line(t) {
            end += 1;
            continue;
        }
        break;
    }

    let (abs_start, abs_end) = if start < end {
        let rel_s = ctx.view.lines.get(start)?.start;
        let rel_e = if end < ctx.view.lines.len() {
            ctx.view.lines.get(end)?.start
        } else {
            ctx.raw().len()
        };
        (comment_span.start + rel_s, comment_span.start + rel_e)
    } else {
        let rel_insert = if start < ctx.view.lines.len() {
            ctx.view.lines.get(start)?.start
        } else {
            ctx.raw().len()
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
    let line_idx = ctx.view.indexes.union_line.get(value).copied()?;
    if ctx.view.lines.is_empty() {
        return None;
    }
    let li = ctx.view.lines.get(line_idx.min(ctx.view.lines.len() - 1))?;
    let line_text = li.text(ctx.raw());
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
