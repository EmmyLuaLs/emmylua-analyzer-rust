use emmylua_parser::{LuaAstNode, LuaComment, LuaTokenKind};
use rowan::TextRange;

use crate::config::ExpandStrategy;
use crate::ir::{self, DocIR, ir_flat_width, ir_has_forced_line_break};
use crate::printer::Printer;

use super::FormatContext;
use super::comments::{format_comment, trailing_comment_prefix};
use super::trivia::has_non_trivia_before_on_same_line_tokenwise;

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

#[derive(Clone, Default)]
pub struct DelimitedSequenceAttachments {
    pub after_open_comment: Option<Vec<DocIR>>,
    pub before_close_comments: Vec<Vec<DocIR>>,
}

#[derive(Default)]
pub struct DelimitedSequenceCommentState {
    attachments: DelimitedSequenceAttachments,
    consumed_comment_ranges: Vec<TextRange>,
    pending_leading_comments: Vec<Vec<DocIR>>,
    has_standalone_comments: bool,
    seen_item: bool,
}

impl DelimitedSequenceCommentState {
    pub fn record_consumed_comment_range(&mut self, range: TextRange) {
        self.consumed_comment_ranges.push(range);
    }

    pub fn should_skip_comment(&self, comment: &LuaComment) -> bool {
        self.consumed_comment_ranges
            .iter()
            .any(|range| *range == comment.syntax().text_range())
    }

    pub fn handle_comment(&mut self, ctx: &FormatContext, comment: &LuaComment) {
        let comment_docs = format_comment(ctx.config, comment);
        if !self.seen_item
            && self.attachments.after_open_comment.is_none()
            && has_non_trivia_before_on_same_line_tokenwise(comment.syntax())
        {
            self.attachments.after_open_comment = Some(comment_docs);
            return;
        }

        self.pending_leading_comments.push(comment_docs);
        self.has_standalone_comments = true;
    }

    pub fn take_leading_comments(&mut self) -> Vec<Vec<DocIR>> {
        std::mem::take(&mut self.pending_leading_comments)
    }

    pub fn mark_item_seen(&mut self) {
        self.seen_item = true;
    }

    pub fn finish(mut self) -> (DelimitedSequenceAttachments, bool) {
        self.attachments.before_close_comments = self.pending_leading_comments;
        (self.attachments, self.has_standalone_comments)
    }
}

#[derive(Clone, Copy)]
pub struct DelimitedSequenceMultilineWrapOptions {
    pub leading_hard_line: bool,
    pub indent_before_close_comments: bool,
}

pub fn push_comment_lines(target: &mut Vec<DocIR>, comments: &[Vec<DocIR>]) {
    for comment_docs in comments {
        target.push(ir::hard_line());
        target.extend(comment_docs.clone());
    }
}

pub fn wrap_multiline_delimited_sequence_docs(
    ctx: &FormatContext,
    open: DocIR,
    close: DocIR,
    inner: Vec<DocIR>,
    trailing: Option<DocIR>,
    attachments: &DelimitedSequenceAttachments,
    options: DelimitedSequenceMultilineWrapOptions,
) -> Vec<DocIR> {
    let mut indented = Vec::new();
    if options.leading_hard_line {
        indented.push(ir::hard_line());
    }
    indented.push(ir::list(inner));
    if let Some(trailing) = trailing {
        indented.push(trailing);
    }
    if !options.indent_before_close_comments {
        push_comment_lines(&mut indented, &attachments.before_close_comments);
    }

    let mut docs = vec![open];
    if let Some(comment_docs) = &attachments.after_open_comment {
        let mut suffix = trailing_comment_prefix(ctx.config);
        suffix.extend(comment_docs.clone());
        docs.push(ir::line_suffix(suffix));
    }
    docs.push(ir::indent(indented));

    if options.indent_before_close_comments && !attachments.before_close_comments.is_empty() {
        let mut closing_comments = Vec::new();
        push_comment_lines(&mut closing_comments, &attachments.before_close_comments);
        docs.push(ir::indent(closing_comments));
    }

    docs.push(ir::hard_line());
    docs.push(close);
    vec![ir::group_break(docs)]
}

#[derive(Clone, Default)]
pub struct SequenceLayoutCandidates {
    pub flat: Option<Vec<DocIR>>,
    pub fill: Option<Vec<DocIR>>,
    pub packed: Option<Vec<DocIR>>,
    pub one_per_line: Option<Vec<DocIR>>,
    pub aligned: Option<Vec<DocIR>>,
    pub preserve: Option<Vec<DocIR>>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SequenceLayoutKind {
    Flat,
    Fill,
    Packed,
    Aligned,
    OnePerLine,
    Preserve,
}

#[derive(Clone)]
struct RankedSequenceCandidate {
    kind: SequenceLayoutKind,
    docs: Vec<DocIR>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SequenceCandidateScore {
    overflow_penalty: usize,
    line_count: usize,
    line_balance_penalty: usize,
    kind_penalty: usize,
    widest_line_slack: usize,
}

#[derive(Clone, Copy, Default)]
pub struct SequenceLayoutPolicy {
    pub allow_alignment: bool,
    pub allow_fill: bool,
    pub allow_preserve: bool,
    pub prefer_preserve_multiline: bool,
    pub force_break_on_standalone_comments: bool,
    pub prefer_balanced_break_lines: bool,
    pub first_line_prefix_width: usize,
}

pub fn choose_sequence_layout(
    ctx: &FormatContext,
    candidates: SequenceLayoutCandidates,
    policy: SequenceLayoutPolicy,
) -> Vec<DocIR> {
    let ordered = ordered_sequence_candidates(candidates, policy);

    if ordered.is_empty() {
        return vec![];
    }

    if ordered.len() == 1 {
        return ordered
            .into_iter()
            .next()
            .map(|candidate| candidate.docs)
            .unwrap_or_default();
    }

    if let Some(flat_candidate) = ordered.first()
        && flat_candidate.kind == SequenceLayoutKind::Flat
        && !ir_has_forced_line_break(&flat_candidate.docs)
        && ir_flat_width(&flat_candidate.docs) + policy.first_line_prefix_width
            <= ctx.config.layout.max_line_width
    {
        return flat_candidate.docs.clone();
    }

    choose_best_sequence_candidate(ctx, ordered, policy)
}

fn ordered_sequence_candidates(
    candidates: SequenceLayoutCandidates,
    policy: SequenceLayoutPolicy,
) -> Vec<RankedSequenceCandidate> {
    let mut ordered = Vec::new();

    if policy.prefer_preserve_multiline {
        if let Some(packed) = candidates.packed.clone() {
            ordered.push(RankedSequenceCandidate {
                kind: SequenceLayoutKind::Packed,
                docs: packed,
            });
        }
        if policy.allow_alignment
            && let Some(aligned) = candidates.aligned.clone()
        {
            ordered.push(RankedSequenceCandidate {
                kind: SequenceLayoutKind::Aligned,
                docs: aligned,
            });
        }
        if let Some(one_per_line) = candidates.one_per_line.clone() {
            ordered.push(RankedSequenceCandidate {
                kind: SequenceLayoutKind::OnePerLine,
                docs: one_per_line,
            });
        }
        push_flat_and_fill_candidates(
            &mut ordered,
            candidates.flat.clone(),
            candidates.fill.clone(),
            policy,
        );
    } else {
        push_flat_and_fill_candidates(
            &mut ordered,
            candidates.flat.clone(),
            candidates.fill.clone(),
            policy,
        );
        if let Some(packed) = candidates.packed.clone() {
            ordered.push(RankedSequenceCandidate {
                kind: SequenceLayoutKind::Packed,
                docs: packed,
            });
        }
        if policy.allow_alignment
            && let Some(aligned) = candidates.aligned.clone()
        {
            ordered.push(RankedSequenceCandidate {
                kind: SequenceLayoutKind::Aligned,
                docs: aligned,
            });
        }
        if let Some(one_per_line) = candidates.one_per_line.clone() {
            ordered.push(RankedSequenceCandidate {
                kind: SequenceLayoutKind::OnePerLine,
                docs: one_per_line,
            });
        }
    }

    if policy.allow_preserve
        && let Some(preserve) = candidates.preserve
    {
        ordered.push(RankedSequenceCandidate {
            kind: SequenceLayoutKind::Preserve,
            docs: preserve,
        });
    }

    ordered
}

fn push_flat_and_fill_candidates(
    ordered: &mut Vec<RankedSequenceCandidate>,
    flat: Option<Vec<DocIR>>,
    fill: Option<Vec<DocIR>>,
    policy: SequenceLayoutPolicy,
) {
    if policy.force_break_on_standalone_comments {
        return;
    }

    if let Some(flat) = flat {
        ordered.push(RankedSequenceCandidate {
            kind: SequenceLayoutKind::Flat,
            docs: flat,
        });
    }

    if policy.allow_fill
        && let Some(fill) = fill
    {
        ordered.push(RankedSequenceCandidate {
            kind: SequenceLayoutKind::Fill,
            docs: fill,
        });
    }
}

fn choose_best_sequence_candidate(
    ctx: &FormatContext,
    candidates: Vec<RankedSequenceCandidate>,
    policy: SequenceLayoutPolicy,
) -> Vec<DocIR> {
    let mut best_docs = None;
    let mut best_score = None;

    for candidate in candidates {
        let score = score_sequence_candidate(ctx, candidate.kind, &candidate.docs, policy);
        if best_score.is_none_or(|current| score < current) {
            best_score = Some(score);
            best_docs = Some(candidate.docs);
        }
    }

    best_docs.unwrap_or_default()
}

fn score_sequence_candidate(
    ctx: &FormatContext,
    kind: SequenceLayoutKind,
    docs: &[DocIR],
    policy: SequenceLayoutPolicy,
) -> SequenceCandidateScore {
    let rendered = Printer::new(ctx.config).print(docs);
    let mut line_count = 0usize;
    let mut overflow_penalty = 0usize;
    let mut widest_line_width = 0usize;
    let mut narrowest_line_width = usize::MAX;

    for line in rendered.lines() {
        line_count += 1;
        let mut line_width = line.len();
        if line_count == 1 {
            line_width += policy.first_line_prefix_width;
        }
        widest_line_width = widest_line_width.max(line_width);
        narrowest_line_width = narrowest_line_width.min(line_width);
        if line_width > ctx.config.layout.max_line_width {
            overflow_penalty += line_width - ctx.config.layout.max_line_width;
        }
    }

    if line_count == 0 {
        line_count = 1;
        narrowest_line_width = 0;
    }

    SequenceCandidateScore {
        overflow_penalty,
        line_count,
        line_balance_penalty: if policy.prefer_balanced_break_lines {
            widest_line_width.saturating_sub(narrowest_line_width)
        } else {
            0
        },
        kind_penalty: sequence_layout_kind_penalty(kind),
        widest_line_slack: ctx
            .config
            .layout
            .max_line_width
            .saturating_sub(widest_line_width.min(ctx.config.layout.max_line_width)),
    }
}

fn sequence_layout_kind_penalty(kind: SequenceLayoutKind) -> usize {
    match kind {
        SequenceLayoutKind::Flat => 0,
        SequenceLayoutKind::Fill => 1,
        SequenceLayoutKind::Packed => 2,
        SequenceLayoutKind::Aligned => 3,
        SequenceLayoutKind::OnePerLine => 4,
        SequenceLayoutKind::Preserve => 10,
    }
}

pub fn format_delimited_sequence(
    ctx: &FormatContext,
    layout: DelimitedSequenceLayout,
) -> Vec<DocIR> {
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
            let gid = ctx.next_group_id();
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

pub fn build_delimited_sequence_flat_candidate(layout: &DelimitedSequenceLayout) -> Vec<DocIR> {
    let flat_inner = ir::intersperse(layout.items.clone(), layout.flat_separator.clone());
    build_flat_doc(
        &layout.open,
        &layout.close,
        &layout.flat_open_padding,
        flat_inner,
        &layout.flat_trailing,
        &layout.flat_close_padding,
    )
}

pub fn build_delimited_sequence_default_break_candidate(
    layout: &DelimitedSequenceLayout,
) -> Vec<DocIR> {
    let break_inner = ir::intersperse(layout.items.clone(), layout.break_separator.clone());
    build_delimited_sequence_break_candidate(
        layout.open.clone(),
        layout.close.clone(),
        default_break_contents(break_inner, layout.grouped_trailing.clone()),
    )
}

pub fn build_delimited_sequence_break_candidate(
    open: DocIR,
    close: DocIR,
    inner: Vec<DocIR>,
) -> Vec<DocIR> {
    format_expanded_delimited_sequence(open, close, inner)
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

#[cfg(test)]
mod tests {
    use super::{
        FormatContext, SequenceLayoutCandidates, SequenceLayoutKind, SequenceLayoutPolicy,
        choose_sequence_layout, score_sequence_candidate,
    };
    use crate::{
        config::{LayoutConfig, LuaFormatConfig},
        ir,
        printer::Printer,
    };

    fn render(config: &LuaFormatConfig, docs: &[crate::ir::DocIR]) -> String {
        Printer::new(config).print(docs)
    }

    #[test]
    fn test_score_prefers_wider_line_when_other_metrics_tie() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 20,
                ..Default::default()
            },
            ..Default::default()
        };
        let ctx = FormatContext::new(&config);

        let wider = vec![ir::list(vec![
            ir::text("alpha beta gamma"),
            ir::hard_line(),
            ir::text("delta"),
        ])];
        let narrower = vec![ir::list(vec![
            ir::text("alpha beta"),
            ir::hard_line(),
            ir::text("gamma delta"),
        ])];

        let wider_score = score_sequence_candidate(
            &ctx,
            SequenceLayoutKind::OnePerLine,
            &wider,
            SequenceLayoutPolicy::default(),
        );
        let narrower_score = score_sequence_candidate(
            &ctx,
            SequenceLayoutKind::OnePerLine,
            &narrower,
            SequenceLayoutPolicy::default(),
        );

        assert!(wider_score < narrower_score);
    }

    #[test]
    fn test_selector_prefers_fill_over_one_per_line_when_both_fit() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 18,
                ..Default::default()
            },
            ..Default::default()
        };
        let ctx = FormatContext::new(&config);

        let selected = choose_sequence_layout(
            &ctx,
            SequenceLayoutCandidates {
                fill: Some(vec![ir::list(vec![
                    ir::text("alpha"),
                    ir::text(", "),
                    ir::text("beta"),
                    ir::hard_line(),
                    ir::text("gamma"),
                ])]),
                one_per_line: Some(vec![ir::list(vec![
                    ir::text("alpha"),
                    ir::hard_line(),
                    ir::text("beta"),
                    ir::hard_line(),
                    ir::text("gamma"),
                ])]),
                ..Default::default()
            },
            SequenceLayoutPolicy {
                allow_fill: true,
                ..Default::default()
            },
        );

        assert_eq!(render(&config, &selected), "alpha, beta\ngamma");
    }

    #[test]
    fn test_selector_prefers_non_overflowing_break_candidate() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 12,
                ..Default::default()
            },
            ..Default::default()
        };
        let ctx = FormatContext::new(&config);

        let selected = choose_sequence_layout(
            &ctx,
            SequenceLayoutCandidates {
                fill: Some(vec![ir::text("alpha_beta_gamma")]),
                one_per_line: Some(vec![ir::list(vec![
                    ir::text("alpha"),
                    ir::hard_line(),
                    ir::text("beta"),
                ])]),
                ..Default::default()
            },
            SequenceLayoutPolicy {
                allow_fill: true,
                ..Default::default()
            },
        );

        assert_eq!(render(&config, &selected), "alpha\nbeta");
    }

    #[test]
    fn test_selector_prefers_balanced_packed_layout_when_line_count_ties() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 28,
                ..Default::default()
            },
            ..Default::default()
        };
        let ctx = FormatContext::new(&config);

        let selected = choose_sequence_layout(
            &ctx,
            SequenceLayoutCandidates {
                fill: Some(vec![ir::list(vec![
                    ir::text("aaaa + bbbb"),
                    ir::hard_line(),
                    ir::text("cccc + dddd + eeee"),
                    ir::hard_line(),
                    ir::text("ffff"),
                ])]),
                packed: Some(vec![ir::list(vec![
                    ir::text("aaaa + bbbb"),
                    ir::hard_line(),
                    ir::text("cccc + dddd"),
                    ir::hard_line(),
                    ir::text("eeee + ffff"),
                ])]),
                ..Default::default()
            },
            SequenceLayoutPolicy {
                allow_fill: true,
                prefer_balanced_break_lines: true,
                ..Default::default()
            },
        );

        assert_eq!(
            render(&config, &selected),
            "aaaa + bbbb\ncccc + dddd\neeee + ffff"
        );
    }

    #[test]
    fn test_prefix_width_can_change_selected_candidate() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 28,
                ..Default::default()
            },
            ..Default::default()
        };
        let ctx = FormatContext::new(&config);

        let selected = choose_sequence_layout(
            &ctx,
            SequenceLayoutCandidates {
                fill: Some(vec![ir::list(vec![
                    ir::text("aaaa + bbbb"),
                    ir::hard_line(),
                    ir::text("+ cccc + dddd + eeee"),
                    ir::hard_line(),
                    ir::text("+ ffff"),
                ])]),
                packed: Some(vec![ir::list(vec![
                    ir::text("aaaa + bbbb"),
                    ir::hard_line(),
                    ir::text("+ cccc + dddd"),
                    ir::hard_line(),
                    ir::text("+ eeee + ffff"),
                ])]),
                ..Default::default()
            },
            SequenceLayoutPolicy {
                allow_fill: true,
                prefer_balanced_break_lines: true,
                first_line_prefix_width: 14,
                ..Default::default()
            },
        );

        assert_eq!(
            render(&config, &selected),
            "aaaa + bbbb\n+ cccc + dddd\n+ eeee + ffff"
        );
    }
}
