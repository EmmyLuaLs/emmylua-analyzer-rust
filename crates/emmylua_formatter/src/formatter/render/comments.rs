use rowan::TextRange;

use super::doc_comments::{
    normalize_doc_comment_block, normalize_mixed_comment_block,
    should_preserve_doc_comment_block_raw,
};
use super::*;

pub(crate) type RenderedTrailingComment = (Vec<DocIR>, TextRange, bool);

pub(crate) fn extract_trailing_comment_rendered(
    ctx: &FormatContext,
    syntax_plan: &SyntaxNodeLayoutPlan,
    node: &LuaSyntaxNode,
    plan: &FormatPlan,
) -> Option<RenderedTrailingComment> {
    let comment = find_inline_trailing_comment_node(node)?;
    if comment.text().contains_char('\n') {
        return None;
    }
    let comment = LuaComment::cast(comment.clone())?;
    let docs = render_comment_with_spacing(ctx, &comment, plan);
    let align_hint = matches!(
        syntax_plan.kind,
        LuaSyntaxKind::LocalStat | LuaSyntaxKind::AssignStat
    ) && trailing_gap_requests_alignment(
        node,
        comment.syntax().text_range(),
        ctx.config.comments.line_comment_min_spaces_before.max(1),
    );
    Some((docs, comment.syntax().text_range(), align_hint))
}

pub(crate) fn append_trailing_comment_suffix(
    ctx: &FormatContext,
    plan: &FormatPlan,
    docs: &mut Vec<DocIR>,
    node: &LuaSyntaxNode,
) {
    let Some(comment_node) = find_inline_trailing_comment_node(node) else {
        return;
    };
    let Some(comment) = LuaComment::cast(comment_node) else {
        return;
    };

    let content_width = crate::ir::ir_flat_width(docs);
    let padding = if ctx.config.comments.line_comment_min_column == 0 {
        ctx.config.comments.line_comment_min_spaces_before.max(1)
    } else {
        ctx.config
            .comments
            .line_comment_min_spaces_before
            .max(1)
            .max(
                ctx.config
                    .comments
                    .line_comment_min_column
                    .saturating_sub(content_width),
            )
    };
    let mut suffix = (0..padding).map(|_| ir::space()).collect::<Vec<_>>();
    suffix.extend(render_comment_with_spacing(ctx, &comment, plan));
    docs.push(ir::line_suffix(suffix));
}

pub(crate) fn append_trailing_statement_suffix(
    ctx: &FormatContext,
    plan: &FormatPlan,
    docs: &mut Vec<DocIR>,
    node: &LuaSyntaxNode,
) {
    append_trailing_statement_semicolon(ctx, plan, docs, node);
    append_trailing_comment_suffix(ctx, plan, docs, node);
}

fn append_trailing_statement_semicolon(
    ctx: &FormatContext,
    plan: &FormatPlan,
    docs: &mut Vec<DocIR>,
    node: &LuaSyntaxNode,
) {
    if !ctx.config.output.preserve_statement_semicolon {
        return;
    }

    let Some(semicolon) = trailing_statement_semicolon_token(node) else {
        return;
    };

    docs.extend(token_left_spacing_docs(plan, Some(&semicolon)));
    docs.push(ir::source_token(semicolon));
}

pub(crate) fn source_order_token_is_trailing_statement_semicolon(
    node: &LuaSyntaxNode,
    token: &LuaSyntaxToken,
) -> bool {
    trailing_statement_semicolon_token(node)
        .is_some_and(|semicolon| semicolon.text_range() == token.text_range())
}

fn trailing_statement_semicolon_token(node: &LuaSyntaxNode) -> Option<LuaSyntaxToken> {
    let children = node.children_with_tokens().collect::<Vec<_>>();
    for child in children.into_iter().rev() {
        match child.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) => continue,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => continue,
            LuaKind::Token(LuaTokenKind::TkSemicolon) => return child.into_token(),
            _ => return None,
        }
    }

    None
}

fn find_inline_trailing_comment_node(node: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    for child in node.children() {
        if child.kind() != LuaKind::Syntax(LuaSyntaxKind::Comment) {
            continue;
        }

        if has_inline_non_trivia_before(&child) && !has_non_trivia_after_in_node(&child) {
            return Some(child);
        }
    }

    let mut next = node.next_sibling_or_token();
    for _ in 0..4 {
        let sibling = next.as_ref()?;
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace)
            | LuaKind::Token(LuaTokenKind::TkSemicolon)
            | LuaKind::Token(LuaTokenKind::TkComma) => {}
            LuaKind::Syntax(LuaSyntaxKind::Comment) => return sibling.as_node().cloned(),
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}

fn has_non_trivia_after_in_node(node: &LuaSyntaxNode) -> bool {
    let mut next = node.next_sibling_or_token();
    while let Some(element) = next {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine) => {
                next = element.next_sibling_or_token();
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                next = element.next_sibling_or_token();
            }
            _ => return true,
        }
    }

    false
}

pub(crate) fn has_inline_non_trivia_before(node: &LuaSyntaxNode) -> bool {
    let mut previous = node.prev_sibling_or_token();
    while let Some(element) = previous {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {
                previous = element.prev_sibling_or_token()
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => previous = element.prev_sibling_or_token(),
            _ => return true,
        }
    }
    false
}

pub(crate) fn has_inline_non_trivia_after(node: &LuaSyntaxNode) -> bool {
    let mut next = node.next_sibling_or_token();
    while let Some(element) = next {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => next = element.next_sibling_or_token(),
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => return false,
            LuaKind::Syntax(LuaSyntaxKind::Comment) => next = element.next_sibling_or_token(),
            _ => return true,
        }
    }
    false
}

pub(crate) fn render_comment_with_spacing(
    ctx: &FormatContext,
    comment: &LuaComment,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    if should_preserve_comment_raw(comment) || should_preserve_doc_comment_block_raw(comment) {
        return vec![ir::source_node_trimmed(comment.syntax().clone())];
    }

    let raw = trim_end_comment_text(comment.syntax().text().to_string());
    let prefix_replacements = collect_comment_line_prefix_replacements(comment, plan);
    let normalized_lines = collect_comment_line_spacing_normalized_texts(comment, plan);
    let lines = if is_pure_doc_comment_block(&raw) {
        normalize_doc_comment_block(
            ctx,
            comment,
            &raw,
            &prefix_replacements,
            normalized_lines.as_slice(),
        )
    } else if contains_doc_comment_line(&raw) {
        normalize_mixed_comment_block(
            ctx,
            comment,
            &raw,
            &prefix_replacements,
            normalized_lines.as_slice(),
        )
    } else {
        normalize_normal_comment_block(ctx, &raw, &prefix_replacements, normalized_lines.as_slice())
    };
    lines
        .into_iter()
        .enumerate()
        .flat_map(|(index, line)| {
            let mut docs = Vec::new();
            if index > 0 {
                docs.push(ir::hard_line());
            }
            if !line.is_empty() {
                docs.push(ir::text(line));
            }
            docs
        })
        .collect()
}

fn trim_end_comment_text(mut text: String) -> String {
    while matches!(text.chars().last(), Some(' ' | '\t' | '\r' | '\n')) {
        text.pop();
    }
    text
}

fn is_pure_doc_comment_block(raw: &str) -> bool {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| line.trim_start().starts_with("---"))
}

fn contains_doc_comment_line(raw: &str) -> bool {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .any(|line| line.trim_start().starts_with("---"))
}

fn collect_comment_line_prefix_replacements(
    comment: &LuaComment,
    plan: &FormatPlan,
) -> Vec<Option<String>> {
    let mut line_prefixes = Vec::new();
    let mut current_prefix = None;
    let mut saw_token_on_line = false;

    for element in comment.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        match token.kind().to_token() {
            LuaTokenKind::TkWhitespace => {}
            LuaTokenKind::TkEndOfLine => {
                line_prefixes.push(current_prefix.take());
                saw_token_on_line = false;
            }
            _ => {
                if !saw_token_on_line {
                    current_prefix = comment_prefix_replacement_for_token(plan, &token);
                    saw_token_on_line = true;
                }
            }
        }
    }

    if saw_token_on_line || current_prefix.is_some() {
        line_prefixes.push(current_prefix);
    }

    line_prefixes
}

fn comment_prefix_replacement_for_token(
    plan: &FormatPlan,
    token: &LuaSyntaxToken,
) -> Option<String> {
    match token.kind().to_token() {
        LuaTokenKind::TkNormalStart
        | LuaTokenKind::TkDocStart
        | LuaTokenKind::TkDocContinue
        | LuaTokenKind::TkDocContinueOr => Some(
            plan.spacing
                .token_replace(LuaSyntaxId::from_token(token))
                .unwrap_or(token.text())
                .to_string(),
        ),
        _ => None,
    }
}

fn normalize_normal_comment_block(
    ctx: &FormatContext,
    raw: &str,
    prefix_replacements: &[Option<String>],
    normalized_lines: &[Option<String>],
) -> Vec<String> {
    let lines: Vec<_> = raw.lines().collect();
    if lines.len() <= 1 {
        return vec![normalize_single_normal_comment_line(
            ctx,
            raw,
            prefix_replacements
                .first()
                .and_then(|prefix| prefix.as_deref()),
            normalized_lines.first().and_then(|line| line.as_deref()),
            false,
        )];
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                String::new()
            } else {
                normalize_single_normal_comment_line(
                    ctx,
                    trimmed,
                    prefix_replacements
                        .get(index)
                        .and_then(|prefix| prefix.as_deref()),
                    normalized_lines.get(index).and_then(|line| line.as_deref()),
                    true,
                )
            }
        })
        .collect()
}

pub(crate) fn normalize_single_normal_comment_line(
    ctx: &FormatContext,
    line: &str,
    prefix_override: Option<&str>,
    _normalized_line: Option<&str>,
    preserve_extra_gap: bool,
) -> String {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("--") || trimmed.starts_with("---") {
        return trimmed.to_string();
    }
    let body_with_gap = &trimmed[2..];
    let preserved_gap = preserved_dash_gap(body_with_gap);
    let prefix = prefix_override.map(str::to_string).unwrap_or_else(|| {
        if ctx.config.comments.space_after_comment_dash {
            "-- ".to_string()
        } else {
            "--".to_string()
        }
    });
    let body = preserved_gap
        .as_ref()
        .map(|gap| &body_with_gap[gap.len()..])
        .unwrap_or_else(|| body_with_gap.trim_start());
    if preserve_extra_gap && let Some(gap) = preserved_gap {
        return if body.is_empty() {
            "--".to_string()
        } else {
            format!("--{gap}{body}")
        };
    }
    if let Some(_gap) = preserved_gap {
        return if body.is_empty() {
            "--".to_string()
        } else {
            format!("{prefix}{body}")
        };
    }
    if prefix.trim_end() == "--"
        && body_with_gap
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        && body.starts_with('[')
    {
        return format!("-- {body}");
    }
    if body.is_empty() {
        prefix.trim_end().to_string()
    } else {
        format!("{prefix}{body}")
    }
}

pub(crate) fn render_direct_body_comment(
    comment: LuaComment,
    ctx: &FormatContext,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    vec![
        ir::indent({
            let mut docs = vec![ir::hard_line()];
            docs.extend(render_comment_with_spacing(ctx, &comment, plan));
            docs
        }),
        ir::hard_line(),
    ]
}

pub(crate) fn comment_is_inline_after_anchor(
    root: &LuaSyntaxNode,
    anchor_token: Option<&LuaSyntaxToken>,
    comment: &LuaSyntaxNode,
) -> bool {
    let Some(anchor_token) = anchor_token else {
        return false;
    };

    let start: usize = anchor_token.text_range().end().into();
    let end: usize = comment.text_range().start().into();
    if end < start {
        return false;
    }

    let text = root.text().to_string();
    !text[start..end].chars().any(|ch| matches!(ch, '\n' | '\r'))
}

fn collect_comment_line_spacing_normalized_texts(
    comment: &LuaComment,
    plan: &FormatPlan,
) -> Vec<Option<String>> {
    let mut lines = Vec::new();
    let mut current_line = Vec::new();

    for element in comment.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        match token.kind().to_token() {
            LuaTokenKind::TkEndOfLine => {
                lines.push(normalize_comment_line_with_spacing(&current_line, plan));
                current_line.clear();
            }
            _ => current_line.push(token),
        }
    }

    if !current_line.is_empty() {
        lines.push(normalize_comment_line_with_spacing(&current_line, plan));
    }

    lines
}

fn normalize_comment_line_with_spacing(
    tokens: &[LuaSyntaxToken],
    plan: &FormatPlan,
) -> Option<String> {
    let mut out = String::new();
    let mut previous_token: Option<&LuaSyntaxToken> = None;
    let mut saw_whitespace = false;

    for token in tokens {
        if token.kind().to_token() == LuaTokenKind::TkWhitespace {
            saw_whitespace = !out.is_empty();
            continue;
        }

        if !out.is_empty() {
            let spacing =
                comment_spacing_between_tokens(plan, previous_token, token, saw_whitespace);
            out.extend(std::iter::repeat_n(' ', spacing));
        }

        out.push_str(comment_token_text(plan, token));
        previous_token = Some(token);
        saw_whitespace = false;
    }

    (!out.is_empty()).then_some(out)
}

fn comment_spacing_between_tokens(
    plan: &FormatPlan,
    previous_token: Option<&LuaSyntaxToken>,
    current_token: &LuaSyntaxToken,
    had_source_whitespace: bool,
) -> usize {
    if had_source_whitespace && previous_token.is_some_and(is_doc_tag_keyword_token) {
        return 1;
    }

    if had_source_whitespace
        && current_token.kind().to_token() == LuaTokenKind::TkLeftBracket
        && previous_token.is_some_and(|token| {
            matches!(
                token.kind().to_token(),
                LuaTokenKind::TkDocVisibility | LuaTokenKind::TkTagVisibility
            )
        })
    {
        return 1;
    }

    let current_id = LuaSyntaxId::from_token(current_token);
    if let Some(expected) = plan.spacing.left_expected(current_id) {
        return resolve_comment_spacing_expected(expected, had_source_whitespace);
    }

    if let Some(previous_token) = previous_token {
        let previous_id = LuaSyntaxId::from_token(previous_token);
        if let Some(expected) = plan.spacing.right_expected(previous_id) {
            return resolve_comment_spacing_expected(expected, had_source_whitespace);
        }
    }

    usize::from(had_source_whitespace)
}

fn resolve_comment_spacing_expected(
    expected: &TokenSpacingExpected,
    had_source_whitespace: bool,
) -> usize {
    match expected {
        TokenSpacingExpected::Space(count) => *count,
        TokenSpacingExpected::MaxSpace(count) => {
            if had_source_whitespace {
                (*count).min(1)
            } else {
                0
            }
        }
    }
}

fn comment_token_text<'a>(plan: &'a FormatPlan, token: &'a LuaSyntaxToken) -> &'a str {
    plan.spacing
        .token_replace(LuaSyntaxId::from_token(token))
        .unwrap_or(token.text())
}

fn is_doc_tag_keyword_token(token: &LuaSyntaxToken) -> bool {
    matches!(
        token.kind().to_token(),
        LuaTokenKind::TkTagClass
            | LuaTokenKind::TkTagAlias
            | LuaTokenKind::TkTagField
            | LuaTokenKind::TkTagType
            | LuaTokenKind::TkTagParam
            | LuaTokenKind::TkTagReturn
            | LuaTokenKind::TkTagGeneric
            | LuaTokenKind::TkTagOverload
            | LuaTokenKind::TkTagVersion
    )
}

pub(crate) fn preserved_dash_gap(text_after_dash: &str) -> Option<String> {
    let gap_len = text_after_dash
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .count();
    if gap_len > 1 {
        Some(text_after_dash[..gap_len].to_string())
    } else {
        None
    }
}

fn should_preserve_comment_raw(comment: &LuaComment) -> bool {
    let raw = comment.syntax().text().to_string();
    if raw.starts_with("----") {
        return true;
    }

    if raw_comment_starts_like_long_comment(raw.as_str()) {
        return true;
    }

    let Some(first_token) = comment.syntax().first_token() else {
        return false;
    };

    matches!(
        first_token.kind().to_token(),
        LuaTokenKind::TkLongCommentStart | LuaTokenKind::TkDocLongStart
    ) || dash_prefix_len(first_token.text()) > 3
}

fn raw_comment_starts_like_long_comment(raw: &str) -> bool {
    let Some(after_dash) = raw.strip_prefix("--") else {
        return false;
    };
    let Some(after_open) = after_dash.strip_prefix('[') else {
        return false;
    };
    let equals_count = after_open.bytes().take_while(|byte| *byte == b'=').count();
    after_open[equals_count..].starts_with('[')
}

fn dash_prefix_len(prefix_text: &str) -> usize {
    prefix_text.bytes().take_while(|byte| *byte == b'-').count()
}
