use crate::config::ExpandStrategy;
use crate::ir::{self, DocIR, ir_flat_width, ir_has_forced_line_break, ir_min_line_count};
use crate::printer::{MeasureLimits, measure_docs, measure_docs_with_limits};

pub type SequenceDocsBuilder = Box<dyn FnOnce() -> Vec<DocIR>>;

#[derive(Clone, Copy, Default)]
pub struct SequenceCandidateHint {
    pub min_line_count: Option<usize>,
    pub has_forced_line_break: Option<bool>,
    pub flat_width: Option<usize>,
}

#[derive(Clone)]
pub struct SequenceComment {
    pub docs: Vec<DocIR>,
    pub inline_after_previous: bool,
}

use super::FormatContext;

#[derive(Clone)]
pub enum SequenceEntry {
    Item(Vec<DocIR>),
    Comment(SequenceComment),
    Separator {
        docs: Vec<DocIR>,
        after_docs: Vec<DocIR>,
    },
}

pub fn render_sequence(docs: &mut Vec<DocIR>, entries: &[SequenceEntry], mut line_start: bool) {
    let mut pending_docs_before_item: Vec<DocIR> = Vec::new();

    for entry in entries {
        match entry {
            SequenceEntry::Item(item_docs) => {
                if !line_start && !pending_docs_before_item.is_empty() {
                    docs.extend(pending_docs_before_item.clone());
                }
                docs.extend(item_docs.clone());
                line_start = false;
                pending_docs_before_item.clear();
            }
            SequenceEntry::Comment(comment) => {
                if comment.inline_after_previous && !line_start {
                    let mut suffix = vec![ir::space()];
                    suffix.extend(comment.docs.clone());
                    docs.push(ir::line_suffix(suffix));
                    docs.push(ir::hard_line());
                } else {
                    if !line_start {
                        docs.push(ir::hard_line());
                    }
                    docs.extend(comment.docs.clone());
                    docs.push(ir::hard_line());
                }
                line_start = true;
                pending_docs_before_item.clear();
            }
            SequenceEntry::Separator {
                docs: separator_docs,
                after_docs,
            } => {
                docs.extend(separator_docs.clone());
                line_start = false;
                pending_docs_before_item = after_docs.clone();
            }
        }
    }
}

pub fn sequence_has_comment(entries: &[SequenceEntry]) -> bool {
    entries
        .iter()
        .any(|entry| matches!(entry, SequenceEntry::Comment(..)))
}

pub fn sequence_ends_with_comment(entries: &[SequenceEntry]) -> bool {
    matches!(entries.last(), Some(SequenceEntry::Comment(..)))
}

pub fn sequence_starts_with_inline_comment(entries: &[SequenceEntry]) -> bool {
    matches!(
        entries.first(),
        Some(SequenceEntry::Comment(SequenceComment {
            inline_after_previous: true,
            ..
        }))
    )
}

#[derive(Default)]
pub struct SequenceLayoutCandidates {
    pub flat: Option<Vec<DocIR>>,
    pub fill: Option<Vec<DocIR>>,
    pub packed: Option<Vec<DocIR>>,
    pub one_per_line: Option<Vec<DocIR>>,
    pub aligned: Option<Vec<DocIR>>,
    pub preserve: Option<Vec<DocIR>>,
    pub flat_builder: Option<SequenceDocsBuilder>,
    pub fill_builder: Option<SequenceDocsBuilder>,
    pub packed_builder: Option<SequenceDocsBuilder>,
    pub one_per_line_builder: Option<SequenceDocsBuilder>,
    pub aligned_builder: Option<SequenceDocsBuilder>,
    pub preserve_builder: Option<SequenceDocsBuilder>,
    pub flat_hint: Option<SequenceCandidateHint>,
    pub fill_hint: Option<SequenceCandidateHint>,
    pub packed_hint: Option<SequenceCandidateHint>,
    pub one_per_line_hint: Option<SequenceCandidateHint>,
    pub aligned_hint: Option<SequenceCandidateHint>,
    pub preserve_hint: Option<SequenceCandidateHint>,
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

struct RankedSequenceCandidate {
    kind: SequenceLayoutKind,
    docs: Option<Vec<DocIR>>,
    builder: Option<SequenceDocsBuilder>,
    hint: SequenceCandidateHint,
}

impl RankedSequenceCandidate {
    fn docs(&mut self) -> &[DocIR] {
        if self.docs.is_none()
            && let Some(builder) = self.builder.take()
        {
            self.docs = Some(builder());
        }

        self.docs.as_deref().unwrap_or(&[])
    }

    fn into_docs(mut self) -> Vec<DocIR> {
        if let Some(docs) = self.docs.take() {
            docs
        } else if let Some(builder) = self.builder.take() {
            builder()
        } else {
            Vec::new()
        }
    }

    fn min_line_count(&mut self) -> usize {
        self.hint
            .min_line_count
            .unwrap_or_else(|| ir_min_line_count(self.docs()))
    }

    fn has_forced_line_break(&mut self) -> bool {
        self.hint
            .has_forced_line_break
            .unwrap_or_else(|| ir_has_forced_line_break(self.docs()))
    }

    fn flat_width(&mut self) -> usize {
        self.hint
            .flat_width
            .unwrap_or_else(|| ir_flat_width(self.docs()))
    }
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
}

pub fn choose_sequence_layout(
    ctx: &FormatContext,
    candidates: SequenceLayoutCandidates,
    policy: SequenceLayoutPolicy,
) -> Vec<DocIR> {
    let mut ordered = ordered_sequence_candidates(candidates, policy);

    if ordered.is_empty() {
        return vec![];
    }

    if ordered.len() == 1 {
        return ordered
            .into_iter()
            .next()
            .map(RankedSequenceCandidate::into_docs)
            .unwrap_or_default();
    }

    let flat_candidate_fits = ordered.first_mut().is_some_and(|flat_candidate| {
        flat_candidate.kind == SequenceLayoutKind::Flat
            && !flat_candidate.has_forced_line_break()
            && flat_candidate.flat_width() + policy.first_line_prefix_width
                <= ctx.config.layout.max_line_width
    });
    if flat_candidate_fits {
        return ordered
            .into_iter()
            .next()
            .map(RankedSequenceCandidate::into_docs)
            .unwrap_or_default();
    }

    choose_best_sequence_candidate(ctx, ordered, policy)
}

fn ordered_sequence_candidates(
    candidates: SequenceLayoutCandidates,
    policy: SequenceLayoutPolicy,
) -> Vec<RankedSequenceCandidate> {
    let SequenceLayoutCandidates {
        flat,
        fill,
        packed,
        one_per_line,
        aligned,
        preserve,
        flat_builder,
        fill_builder,
        packed_builder,
        one_per_line_builder,
        aligned_builder,
        preserve_builder,
        flat_hint,
        fill_hint,
        packed_hint,
        one_per_line_hint,
        aligned_hint,
        preserve_hint,
    } = candidates;
    let mut ordered = Vec::new();

    if policy.prefer_preserve_multiline {
        push_sequence_candidate(
            &mut ordered,
            SequenceLayoutKind::Packed,
            packed,
            packed_builder,
            packed_hint.unwrap_or_default(),
        );
        push_sequence_candidate_if_allowed(
            &mut ordered,
            policy.allow_alignment,
            SequenceLayoutKind::Aligned,
            aligned,
            aligned_builder,
            aligned_hint.unwrap_or_default(),
        );
        push_sequence_candidate(
            &mut ordered,
            SequenceLayoutKind::OnePerLine,
            one_per_line,
            one_per_line_builder,
            one_per_line_hint.unwrap_or_default(),
        );
        push_flat_and_fill_candidates(
            &mut ordered,
            flat,
            fill,
            flat_builder,
            fill_builder,
            flat_hint,
            fill_hint,
            policy,
        );
    } else {
        push_flat_and_fill_candidates(
            &mut ordered,
            flat,
            fill,
            flat_builder,
            fill_builder,
            flat_hint,
            fill_hint,
            policy,
        );
        push_sequence_candidate(
            &mut ordered,
            SequenceLayoutKind::Packed,
            packed,
            packed_builder,
            packed_hint.unwrap_or_default(),
        );
        push_sequence_candidate_if_allowed(
            &mut ordered,
            policy.allow_alignment,
            SequenceLayoutKind::Aligned,
            aligned,
            aligned_builder,
            aligned_hint.unwrap_or_default(),
        );
        push_sequence_candidate(
            &mut ordered,
            SequenceLayoutKind::OnePerLine,
            one_per_line,
            one_per_line_builder,
            one_per_line_hint.unwrap_or_default(),
        );
    }

    push_sequence_candidate_if_allowed(
        &mut ordered,
        policy.allow_preserve,
        SequenceLayoutKind::Preserve,
        preserve,
        preserve_builder,
        preserve_hint.unwrap_or_default(),
    );

    ordered
}

fn push_sequence_candidate(
    ordered: &mut Vec<RankedSequenceCandidate>,
    kind: SequenceLayoutKind,
    docs: Option<Vec<DocIR>>,
    builder: Option<SequenceDocsBuilder>,
    hint: SequenceCandidateHint,
) {
    if docs.is_some() || builder.is_some() {
        ordered.push(RankedSequenceCandidate {
            kind,
            docs,
            builder,
            hint,
        });
    }
}

fn push_sequence_candidate_if_allowed(
    ordered: &mut Vec<RankedSequenceCandidate>,
    allowed: bool,
    kind: SequenceLayoutKind,
    docs: Option<Vec<DocIR>>,
    builder: Option<SequenceDocsBuilder>,
    hint: SequenceCandidateHint,
) {
    if allowed {
        push_sequence_candidate(ordered, kind, docs, builder, hint);
    }
}

fn push_flat_and_fill_candidates(
    ordered: &mut Vec<RankedSequenceCandidate>,
    flat: Option<Vec<DocIR>>,
    fill: Option<Vec<DocIR>>,
    flat_builder: Option<SequenceDocsBuilder>,
    fill_builder: Option<SequenceDocsBuilder>,
    flat_hint: Option<SequenceCandidateHint>,
    fill_hint: Option<SequenceCandidateHint>,
    policy: SequenceLayoutPolicy,
) {
    if policy.force_break_on_standalone_comments {
        return;
    }
    push_sequence_candidate(
        ordered,
        SequenceLayoutKind::Flat,
        flat,
        flat_builder,
        flat_hint.unwrap_or_default(),
    );
    push_sequence_candidate_if_allowed(
        ordered,
        policy.allow_fill,
        SequenceLayoutKind::Fill,
        fill,
        fill_builder,
        fill_hint.unwrap_or_default(),
    );
}

fn choose_best_sequence_candidate(
    ctx: &FormatContext,
    candidates: Vec<RankedSequenceCandidate>,
    policy: SequenceLayoutPolicy,
) -> Vec<DocIR> {
    let mut best_docs = None;
    let mut best_score = None;

    for mut candidate in candidates {
        let kind = candidate.kind;
        let min_line_count = candidate.min_line_count();

        if let Some(current_best) = best_score
            && candidate_can_be_pruned(min_line_count, kind, current_best)
        {
            continue;
        }

        let docs = candidate.docs();
        let score = score_sequence_candidate(ctx, kind, docs, policy, best_score);
        if best_score.is_none_or(|current| score < current) {
            best_score = Some(score);
            best_docs = Some(candidate.into_docs());
        }
    }

    best_docs.unwrap_or_default()
}

fn candidate_can_be_pruned(
    min_line_count: usize,
    kind: SequenceLayoutKind,
    best_score: SequenceCandidateScore,
) -> bool {
    if best_score.overflow_penalty != 0 {
        return false;
    }

    if min_line_count > best_score.line_count {
        return true;
    }

    min_line_count == best_score.line_count
        && best_score.line_balance_penalty == 0
        && sequence_layout_kind_penalty(kind) > best_score.kind_penalty
}

fn score_sequence_candidate(
    ctx: &FormatContext,
    kind: SequenceLayoutKind,
    docs: &[DocIR],
    policy: SequenceLayoutPolicy,
    best_score: Option<SequenceCandidateScore>,
) -> SequenceCandidateScore {
    let candidate_kind_penalty = sequence_layout_kind_penalty(kind);
    let limits = best_score.map(|current_best| MeasureLimits {
        max_lines: (current_best.overflow_penalty == 0
            && candidate_kind_penalty >= current_best.kind_penalty)
            .then_some(current_best.line_count),
        max_overflow_penalty: Some(current_best.overflow_penalty),
        first_line_prefix_width: policy.first_line_prefix_width,
    });
    let (metrics, truncated) = if let Some(limits) = limits {
        measure_docs_with_limits(ctx.config, docs, limits)
    } else {
        (measure_docs(ctx.config, docs), false)
    };

    if truncated {
        let overflow_penalty = limits
            .and_then(|limits| limits.max_overflow_penalty)
            .unwrap_or_default();
        return SequenceCandidateScore {
            overflow_penalty: overflow_penalty.saturating_add(1),
            line_count: limits
                .and_then(|limits| limits.max_lines)
                .unwrap_or_default()
                .saturating_add(1),
            line_balance_penalty: usize::MAX,
            kind_penalty: candidate_kind_penalty,
            widest_line_slack: ctx.config.layout.max_line_width,
        };
    }

    let mut line_count = 0usize;
    let mut overflow_penalty = 0usize;
    let mut widest_line_width = 0usize;
    let mut narrowest_line_width = usize::MAX;

    for (index, measured_width) in metrics.line_widths.iter().enumerate() {
        line_count += 1;
        let mut line_width = *measured_width;
        if index == 0 {
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
        kind_penalty: candidate_kind_penalty,
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
    _ctx: &FormatContext,
    layout: DelimitedSequenceLayout,
) -> Vec<DocIR> {
    if layout.items.is_empty() {
        return vec![layout.open, layout.close];
    }

    let DelimitedSequenceLayout {
        open,
        close,
        items,
        strategy,
        preserve_multiline,
        flat_separator,
        fill_separator,
        break_separator,
        flat_open_padding,
        flat_close_padding,
        grouped_padding,
        flat_trailing,
        grouped_trailing,
    } = layout;

    match strategy {
        ExpandStrategy::Never => build_flat_doc(
            &open,
            &close,
            &flat_open_padding,
            build_interspersed_docs(&items, &flat_separator),
            &flat_trailing,
            &flat_close_padding,
        ),
        ExpandStrategy::Always => format_expanded_delimited_sequence(
            open,
            close,
            default_break_contents(
                build_interspersed_docs(&items, &break_separator),
                grouped_trailing,
            ),
        ),
        ExpandStrategy::Auto if preserve_multiline => format_expanded_delimited_sequence(
            open,
            close,
            default_break_contents(
                build_interspersed_docs(&items, &break_separator),
                grouped_trailing,
            ),
        ),
        ExpandStrategy::Auto => vec![ir::group(vec![
            open,
            ir::indent(vec![
                grouped_padding.clone(),
                ir::fill(build_fill_parts(&items, &fill_separator)),
                grouped_trailing,
            ]),
            grouped_padding,
            close,
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

fn build_interspersed_docs(items: &[Vec<DocIR>], separator: &[DocIR]) -> Vec<DocIR> {
    let mut docs = Vec::new();

    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            docs.extend(separator.to_vec());
        }
        docs.extend(item.clone());
    }

    docs
}
