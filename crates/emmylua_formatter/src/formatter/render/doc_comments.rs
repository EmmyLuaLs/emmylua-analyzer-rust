use std::collections::HashMap;

use super::comments::{normalize_single_normal_comment_line, preserved_dash_gap};
use super::*;

#[derive(Clone)]
enum DocBlockLine {
    Description(DocDescriptionLine),
    Tag(DocTagLine),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DocLinePrefixKind {
    Start,
    Continue,
    ContinueOr,
    Unknown,
}

struct DocBlockLineInput<'a> {
    raw_line: &'a str,
    normalized_line: Option<&'a str>,
    structured_tag: Option<StructuredDocTagColumns>,
    prefix_kind: DocLinePrefixKind,
}

struct DocCommentLineMap<'a> {
    raw_lines: Vec<&'a str>,
    line_start_offsets: Vec<usize>,
    prefix_kinds: Vec<DocLinePrefixKind>,
}

#[derive(Clone)]
enum DocDescriptionKind {
    Plain,
    ContinueOr(String),
}

#[derive(Clone)]
struct DocDescriptionLine {
    kind: DocDescriptionKind,
    content: String,
    preserve_spacing: bool,
    gap_after_dash: Option<String>,
    columns: Vec<String>,
    align_key: Option<String>,
}

#[derive(Clone)]
struct DocTagLine {
    tag: String,
    raw_rest: String,
    columns: Vec<String>,
    align_key: Option<String>,
    preserve_body_spacing: bool,
    gap_after_dash: Option<String>,
}

#[derive(Clone)]
struct StructuredDocTagColumns {
    tag: String,
    head_columns: Vec<String>,
    description: Option<String>,
    use_normalized_head_as_single_column: bool,
}

pub(crate) fn should_preserve_doc_comment_block_raw(comment: &LuaComment) -> bool {
    let raw = comment.syntax().text().to_string();
    raw.lines().any(|line| {
        let trimmed = line.trim_start();
        (trimmed.starts_with("---@type") || trimmed.starts_with("--- @type"))
            && trimmed.contains(" --")
    })
}

pub(crate) fn normalize_doc_comment_block(
    ctx: &FormatContext,
    comment: &LuaComment,
    raw: &str,
    prefix_replacements: &[Option<String>],
    normalized_lines: &[Option<String>],
) -> Vec<String> {
    let line_inputs = collect_doc_block_line_inputs(ctx, comment, raw, normalized_lines);
    let mut parsed = Vec::with_capacity(line_inputs.len());

    for line_input in &line_inputs {
        parsed.push(parse_doc_block_line(ctx, line_input));
    }

    let parsed = annotate_multiline_alias_continue_lines(ctx, parsed);

    let mut widths: HashMap<String, Vec<usize>> = HashMap::new();
    for line in &parsed {
        let (align_key, columns) = match line {
            DocBlockLine::Tag(tag) => (tag.align_key.as_ref(), &tag.columns),
            DocBlockLine::Description(line)
                if matches!(line.kind, DocDescriptionKind::ContinueOr(_)) =>
            {
                (line.align_key.as_ref(), &line.columns)
            }
            DocBlockLine::Description(_) => (None, &Vec::new()),
        };
        let Some(key) = align_key else {
            continue;
        };
        let entry = widths
            .entry(key.clone())
            .or_insert_with(|| vec![0; columns.len().saturating_sub(1)]);
        if entry.len() < columns.len().saturating_sub(1) {
            entry.resize(columns.len().saturating_sub(1), 0);
        }
        for (index, column) in columns
            .iter()
            .take(columns.len().saturating_sub(1))
            .enumerate()
        {
            entry[index] = entry[index].max(column.len());
        }
    }

    parsed
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            format_doc_block_line(
                ctx,
                line,
                &widths,
                prefix_replacements
                    .get(index)
                    .and_then(|prefix| prefix.as_deref()),
            )
        })
        .collect()
}

pub(crate) fn normalize_mixed_comment_block(
    ctx: &FormatContext,
    comment: &LuaComment,
    raw: &str,
    prefix_replacements: &[Option<String>],
    normalized_lines: &[Option<String>],
) -> Vec<String> {
    let line_inputs = collect_doc_block_line_inputs(ctx, comment, raw, normalized_lines);
    let parsed = annotate_multiline_alias_continue_lines(
        ctx,
        line_inputs
            .iter()
            .map(|line_input| parse_doc_block_line(ctx, line_input))
            .collect(),
    );

    let mut widths: HashMap<String, Vec<usize>> = HashMap::new();
    for line in &parsed {
        let (align_key, columns) = match line {
            DocBlockLine::Tag(tag) => (tag.align_key.as_ref(), &tag.columns),
            DocBlockLine::Description(line)
                if matches!(line.kind, DocDescriptionKind::ContinueOr(_)) =>
            {
                (line.align_key.as_ref(), &line.columns)
            }
            DocBlockLine::Description(_) => (None, &Vec::new()),
        };
        let Some(key) = align_key else {
            continue;
        };
        let entry = widths
            .entry(key.clone())
            .or_insert_with(|| vec![0; columns.len().saturating_sub(1)]);
        if entry.len() < columns.len().saturating_sub(1) {
            entry.resize(columns.len().saturating_sub(1), 0);
        }
        for (index, column) in columns
            .iter()
            .take(columns.len().saturating_sub(1))
            .enumerate()
        {
            entry[index] = entry[index].max(column.len());
        }
    }

    line_inputs
        .iter()
        .enumerate()
        .map(|(index, line_input)| {
            let trimmed = line_input.raw_line.trim_start();
            if trimmed.is_empty() {
                return String::new();
            }

            if trimmed.starts_with("---") {
                return format_doc_block_line(
                    ctx,
                    parsed[index].clone(),
                    &widths,
                    prefix_replacements
                        .get(index)
                        .and_then(|prefix| prefix.as_deref()),
                );
            }

            normalize_single_normal_comment_line(
                ctx,
                trimmed,
                prefix_replacements
                    .get(index)
                    .and_then(|prefix| prefix.as_deref()),
                normalized_lines.get(index).and_then(|line| line.as_deref()),
                true,
            )
        })
        .collect()
}

fn collect_doc_block_line_inputs<'a>(
    ctx: &FormatContext,
    comment: &'a LuaComment,
    raw: &'a str,
    normalized_lines: &'a [Option<String>],
) -> Vec<DocBlockLineInput<'a>> {
    let line_map = collect_doc_comment_line_map(comment, raw);
    let structured_tags_by_line =
        collect_structured_doc_tag_columns_by_line(ctx, comment, &line_map);

    line_map
        .raw_lines
        .into_iter()
        .enumerate()
        .map(|(index, raw_line)| DocBlockLineInput {
            raw_line,
            normalized_line: normalized_lines.get(index).and_then(|line| line.as_deref()),
            structured_tag: structured_tags_by_line.get(index).cloned().flatten(),
            prefix_kind: line_map
                .prefix_kinds
                .get(index)
                .copied()
                .unwrap_or(DocLinePrefixKind::Unknown),
        })
        .collect()
}

fn parse_doc_block_line(ctx: &FormatContext, line_input: &DocBlockLineInput) -> DocBlockLine {
    let raw_suffix = strip_doc_line_prefix(line_input.raw_line, line_input.prefix_kind);
    let trimmed = raw_suffix.trim_start();
    let gap_after_dash = preserved_dash_gap(raw_suffix);
    let normalized_suffix = strip_doc_line_prefix(
        line_input.normalized_line.unwrap_or(line_input.raw_line),
        line_input.prefix_kind,
    );
    let normalized_trimmed = normalized_suffix.trim_start();

    if is_continue_or_doc_line(line_input.prefix_kind, trimmed) {
        let marker = doc_continue_marker(trimmed);
        return DocBlockLine::Description(DocDescriptionLine {
            kind: DocDescriptionKind::ContinueOr(marker.to_string()),
            content: normalized_continue_line_content(normalized_trimmed).to_string(),
            preserve_spacing: false,
            gap_after_dash,
            columns: Vec::new(),
            align_key: None,
        });
    }

    let tag_rest = if line_input.prefix_kind == DocLinePrefixKind::Start {
        Some(trimmed.strip_prefix('@').unwrap_or(trimmed))
    } else {
        trimmed.strip_prefix('@')
    };

    if let Some(rest) = tag_rest {
        let normalized_rest = if line_input.prefix_kind == DocLinePrefixKind::Start {
            normalized_trimmed
                .strip_prefix('@')
                .unwrap_or(normalized_trimmed)
        } else {
            normalized_trimmed
                .strip_prefix('@')
                .unwrap_or(rest)
                .trim_start()
        };
        return DocBlockLine::Tag(parse_doc_tag_line(
            ctx,
            normalized_rest.trim_start(),
            rest.trim_start(),
            gap_after_dash,
            line_input.structured_tag.as_ref(),
        ));
    }

    let preserve_spacing = gap_after_dash.is_some();
    let content = if preserve_spacing {
        raw_suffix.to_string()
    } else {
        strip_single_comment_gap(raw_suffix).to_string()
    };
    DocBlockLine::Description(DocDescriptionLine {
        kind: DocDescriptionKind::Plain,
        content,
        preserve_spacing,
        gap_after_dash,
        columns: Vec::new(),
        align_key: None,
    })
}

fn parse_doc_tag_line(
    ctx: &FormatContext,
    rest: &str,
    raw_rest_source: &str,
    gap_after_dash: Option<String>,
    structured_tag: Option<&StructuredDocTagColumns>,
) -> DocTagLine {
    let mut parts = rest.split_whitespace();
    let tag = parts.next().unwrap_or_default().to_string();
    let normalized_rest = rest
        .strip_prefix(tag.as_str())
        .unwrap_or("")
        .trim_start()
        .to_string();
    let raw_rest = raw_rest_source
        .strip_prefix(tag.as_str())
        .unwrap_or("")
        .trim_start()
        .to_string();
    let structured_tag = structured_tag.filter(|structured| structured.tag == tag);
    let (normalized_head, raw_description) = if structured_tag.is_some() {
        (normalized_rest.clone(), None)
    } else {
        split_doc_tag_description(&normalized_rest, &raw_rest)
    };
    let structured_description = structured_tag
        .and_then(|structured| structured.description.clone())
        .and_then(first_structured_description_line);
    let mut columns = structured_tag
        .map(|structured| {
            if structured.use_normalized_head_as_single_column {
                structured_single_head_columns(&normalized_head, structured_description.as_deref())
            } else {
                structured_columns_from_normalized_head(
                    &normalized_head,
                    &structured.head_columns,
                    structured_description.as_deref(),
                )
            }
        })
        .unwrap_or_else(|| match tag.as_str() {
            "param" => parse_param_columns(&normalized_head),
            "field" => parse_field_columns(&normalized_head),
            "return" => parse_return_columns(&normalized_head),
            "class" => split_columns(&normalized_head, &[1]),
            "alias" => parse_alias_columns(&normalized_head),
            "generic" => parse_generic_columns(&normalized_head),
            "type" | "overload" => vec![normalized_head.clone()],
            _ => vec![collapse_spaces(&normalized_head)],
        });
    if let Some(description) = raw_description.or(structured_description) {
        columns.push(description);
    }
    columns.retain(|column| !column.is_empty());

    let align_key = match tag.as_str() {
        "class" | "alias" | "field" | "generic"
            if ctx.config.should_align_emmy_doc_declaration_tags() =>
        {
            Some(tag.clone())
        }
        "param" | "return" if ctx.config.should_align_emmy_doc_reference_tags() => {
            Some(tag.clone())
        }
        _ => None,
    };

    let preserve_body_spacing = tag == "alias" && !ctx.config.emmy_doc.align_tag_columns;

    DocTagLine {
        tag,
        raw_rest,
        columns,
        align_key,
        preserve_body_spacing,
        gap_after_dash,
    }
}

fn format_doc_block_line(
    ctx: &FormatContext,
    line: DocBlockLine,
    widths: &HashMap<String, Vec<usize>>,
    prefix_override: Option<&str>,
) -> String {
    match line {
        DocBlockLine::Description(line) => match line.kind {
            DocDescriptionKind::Plain => {
                if line.preserve_spacing {
                    format!("---{}", line.content)
                } else {
                    let prefix = prefix_override.map(str::to_string).unwrap_or_else(|| {
                        if ctx.config.emmy_doc.space_after_description_dash {
                            "--- ".to_string()
                        } else {
                            "---".to_string()
                        }
                    });
                    if line.content.is_empty() {
                        prefix.trim_end().to_string()
                    } else {
                        format!("{prefix}{}", line.content)
                    }
                }
            }
            DocDescriptionKind::ContinueOr(marker) => {
                let prefix = if let Some(gap_after_dash) = line.gap_after_dash.as_deref() {
                    format!("---{gap_after_dash}{marker}")
                } else {
                    prefix_override
                        .map(|prefix| {
                            if prefix.contains('|') {
                                prefix.to_string()
                            } else {
                                normalized_doc_continue_marker_prefix(ctx, &marker)
                            }
                        })
                        .unwrap_or_else(|| normalized_doc_continue_marker_prefix(ctx, &marker))
                };
                if let Some(key) = &line.align_key {
                    let Some((first, rest)) = line.columns.split_first() else {
                        return prefix;
                    };
                    let mut rendered = prefix;
                    rendered.push(' ');
                    rendered.push_str(first);
                    for (index, column) in rest.iter().enumerate() {
                        let source_index = index;
                        let padding = widths
                            .get(key)
                            .and_then(|widths| widths.get(source_index))
                            .map(|width| width.saturating_sub(line.columns[source_index].len()) + 1)
                            .unwrap_or(1);
                        rendered.extend(std::iter::repeat_n(' ', padding));
                        rendered.push_str(column);
                    }
                    return rendered;
                }
                if line.content.is_empty() {
                    prefix
                } else {
                    let separator = if prefix.ends_with(' ') { "" } else { " " };
                    format!("{prefix}{separator}{}", line.content)
                }
            }
        },
        DocBlockLine::Tag(tag) => {
            let prefix = if let Some(gap_after_dash) = tag.gap_after_dash.as_deref() {
                format!("---{gap_after_dash}@{}", tag.tag)
            } else if let Some(prefix) = prefix_override {
                format_doc_tag_prefix_override(prefix, &tag.tag)
            } else if ctx.config.emmy_doc.space_between_tag_columns {
                format!("--- @{}", tag.tag)
            } else {
                format!("---@{}", tag.tag)
            };
            if tag.preserve_body_spacing {
                return if tag.raw_rest.is_empty() {
                    prefix
                } else {
                    format!("{prefix} {}", tag.raw_rest)
                };
            }
            let Some(key) = &tag.align_key else {
                return if tag.columns.is_empty() {
                    prefix
                } else {
                    format!("{prefix} {}", tag.columns.join(" "))
                };
            };
            let target_widths = widths.get(key);
            let mut rendered = prefix;
            if let Some((first, rest)) = tag.columns.split_first() {
                rendered.push(' ');
                rendered.push_str(first);
                for (index, column) in rest.iter().enumerate() {
                    let source_index = index;
                    let padding = target_widths
                        .and_then(|widths: &Vec<usize>| widths.get(source_index))
                        .map(|width: &usize| {
                            width.saturating_sub(tag.columns[source_index].len()) + 1
                        })
                        .unwrap_or(1);
                    rendered.extend(std::iter::repeat_n(' ', padding));
                    rendered.push_str(column);
                }
            }
            rendered
        }
    }
}

fn annotate_multiline_alias_continue_lines(
    ctx: &FormatContext,
    parsed: Vec<DocBlockLine>,
) -> Vec<DocBlockLine> {
    let mut in_alias_block = false;

    parsed
        .into_iter()
        .map(|line| match line {
            DocBlockLine::Tag(tag) => {
                in_alias_block = tag.tag == "alias";
                DocBlockLine::Tag(tag)
            }
            DocBlockLine::Description(mut line) => {
                if in_alias_block
                    && matches!(line.kind, DocDescriptionKind::ContinueOr(_))
                    && ctx
                        .config
                        .should_align_emmy_doc_multiline_alias_descriptions()
                {
                    let columns = parse_multiline_alias_continue_columns(&line.content);
                    if columns.len() > 1 {
                        line.align_key = Some("alias_multiline_description".to_string());
                        line.columns = columns;
                    }
                }

                if matches!(line.kind, DocDescriptionKind::Plain) {
                    in_alias_block = false;
                }

                DocBlockLine::Description(line)
            }
        })
        .collect()
}

fn collect_doc_comment_line_map<'a>(comment: &LuaComment, raw: &'a str) -> DocCommentLineMap<'a> {
    let comment_start = comment.syntax().text_range().start();
    let mut raw_lines = Vec::new();
    let mut line_start_offsets = vec![0usize];
    let mut prefix_kinds = vec![DocLinePrefixKind::Unknown];
    let mut current_line_start = 0usize;
    let mut current_line_index = 0usize;
    let mut saw_non_whitespace = false;

    for element in comment.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };

        let relative_start =
            u32::from(token.text_range().start()).saturating_sub(u32::from(comment_start)) as usize;
        let relative_end =
            u32::from(token.text_range().end()).saturating_sub(u32::from(comment_start)) as usize;

        match token.kind().to_token() {
            LuaTokenKind::TkEndOfLine => {
                raw_lines.push(raw.get(current_line_start..relative_start).unwrap_or(""));
                current_line_start = relative_end.min(raw.len());
                current_line_index += 1;
                line_start_offsets.push(current_line_start);
                prefix_kinds.push(DocLinePrefixKind::Unknown);
                saw_non_whitespace = false;
            }
            LuaTokenKind::TkWhitespace => {}
            kind if !saw_non_whitespace => {
                if let Some(prefix_kind) = doc_line_prefix_kind_from_token(kind)
                    && let Some(slot) = prefix_kinds.get_mut(current_line_index)
                {
                    *slot = prefix_kind;
                }
                saw_non_whitespace = true;
            }
            _ => {
                saw_non_whitespace = true;
            }
        }
    }

    if raw_lines.len() < prefix_kinds.len() {
        raw_lines.push(raw.get(current_line_start..raw.len()).unwrap_or(""));
    }

    DocCommentLineMap {
        raw_lines,
        line_start_offsets,
        prefix_kinds,
    }
}

fn strip_doc_line_prefix(line: &str, prefix_kind: DocLinePrefixKind) -> &str {
    let trimmed = line.trim_start();
    match prefix_kind {
        DocLinePrefixKind::Start => trimmed
            .strip_prefix("---@")
            .or_else(|| trimmed.strip_prefix("---"))
            .unwrap_or(trimmed),
        DocLinePrefixKind::Continue | DocLinePrefixKind::Unknown => {
            trimmed.strip_prefix("---").unwrap_or(trimmed)
        }
        DocLinePrefixKind::ContinueOr => trimmed
            .strip_prefix("---|+")
            .or_else(|| trimmed.strip_prefix("---|>"))
            .or_else(|| trimmed.strip_prefix("---|"))
            .or_else(|| trimmed.strip_prefix("---"))
            .unwrap_or(trimmed),
    }
}

fn is_continue_or_doc_line(prefix_kind: DocLinePrefixKind, trimmed_content: &str) -> bool {
    prefix_kind == DocLinePrefixKind::ContinueOr || trimmed_content.starts_with('|')
}

fn doc_line_prefix_kind_from_token(token_kind: LuaTokenKind) -> Option<DocLinePrefixKind> {
    match token_kind {
        LuaTokenKind::TkDocStart | LuaTokenKind::TkDocLongStart => Some(DocLinePrefixKind::Start),
        LuaTokenKind::TkDocContinue => Some(DocLinePrefixKind::Continue),
        LuaTokenKind::TkDocContinueOr => Some(DocLinePrefixKind::ContinueOr),
        _ => None,
    }
}

fn split_columns(input: &str, head_sizes: &[usize]) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut columns = Vec::new();
    let mut index = 0;
    for head_size in head_sizes {
        if index >= tokens.len() {
            break;
        }
        let end = (index + *head_size).min(tokens.len());
        columns.push(tokens[index..end].join(" "));
        index = end;
    }
    if index < tokens.len() {
        columns.push(tokens[index..].join(" "));
    }
    columns
}

fn parse_field_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let visibility = matches!(
        tokens.first().copied(),
        Some("public" | "private" | "protected")
    );
    if visibility && tokens.len() >= 2 {
        if let Some((name, ty)) = split_attached_field_name_and_type(tokens[1]) {
            let mut columns = vec![format!("{} {}", tokens[0], name), ty.to_string()];
            if tokens.len() >= 3 {
                columns.push(tokens[2..].join(" "));
            }
            return columns;
        }
        if tokens
            .get(2)
            .is_some_and(|token| !looks_like_field_type_start(token))
        {
            return vec![
                format!("{} {}", tokens[0], tokens[1]),
                tokens[2..].join(" "),
            ];
        }
        let mut columns = vec![format!("{} {}", tokens[0], tokens[1])];
        if tokens.len() >= 3 {
            columns.push(tokens[2].to_string());
        }
        if tokens.len() >= 4 {
            columns.push(tokens[3..].join(" "));
        }
        columns
    } else {
        if let Some((name, ty)) = split_attached_field_name_and_type(tokens[0]) {
            let mut columns = vec![name.to_string(), ty.to_string()];
            if tokens.len() >= 2 {
                columns.push(tokens[1..].join(" "));
            }
            return columns;
        }
        if tokens.len() >= 2 && !looks_like_field_type_start(tokens[1]) {
            vec![tokens[0].to_string(), tokens[1..].join(" ")]
        } else {
            split_columns(input, &[1, 1])
        }
    }
}

fn split_attached_field_name_and_type(token: &str) -> Option<(&str, &str)> {
    let split_index = token.find('(')?;
    if split_index == 0 {
        return None;
    }

    let (name, ty) = token.split_at(split_index);
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.'))
    {
        return None;
    }

    Some((name, ty))
}

fn parse_param_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }

    if tokens.len() >= 3 && matches!(tokens[1], "sync" | "async") && tokens[2].starts_with("fun") {
        return vec![tokens[0].to_string(), tokens[1..].join(" ")];
    }

    split_columns(input, &[1, 1])
}

fn parse_return_columns(input: &str) -> Vec<String> {
    parse_return_columns_without_raw_description(input)
}

fn parse_return_columns_without_raw_description(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![tokens[0].to_string()],
        2 => vec![tokens.join(" ")],
        _ => vec![
            tokens[..tokens.len() - 1].join(" "),
            tokens[tokens.len() - 1].to_string(),
        ],
    }
}

fn parse_alias_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![tokens[0].to_string()],
        2 => vec![tokens.join(" ")],
        _ => vec![tokens[..2].join(" "), tokens[2..].join(" ")],
    }
}

fn parse_multiline_alias_continue_columns(input: &str) -> Vec<String> {
    let Some(hash_index) = input.find(" #") else {
        return vec![input.trim().to_string()];
    };

    let value = input[..hash_index].trim();
    let description = input[hash_index + 2..].trim_start();
    if value.is_empty() || description.is_empty() {
        return vec![input.trim().to_string()];
    }

    vec![value.to_string(), format!("# {description}")]
}

fn looks_like_field_type_start(token: &str) -> bool {
    if token.is_empty() || token.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return false;
    }

    token.starts_with("fun(")
        || token.starts_with('[')
        || token.starts_with('"')
        || token.contains('|')
        || token.contains('?')
        || token.contains('<')
        || token.contains('{')
        || matches!(
            token,
            "any"
                | "boolean"
                | "function"
                | "global"
                | "integer"
                | "lightuserdata"
                | "nil"
                | "number"
                | "self"
                | "string"
                | "table"
                | "thread"
                | "unknown"
                | "userdata"
                | "void"
        )
}

fn split_doc_tag_description(normalized_input: &str, raw_input: &str) -> (String, Option<String>) {
    let normalized_head = normalized_input
        .find('#')
        .map(|index| normalized_input[..index].trim_end().to_string())
        .unwrap_or_else(|| normalized_input.to_string());

    let raw_description = raw_input.find('#').and_then(|index| {
        let description = raw_input[index..].trim_start();
        (!description.is_empty()).then(|| description.to_string())
    });

    (normalized_head, raw_description)
}

fn doc_continue_marker(text: &str) -> &str {
    if text.starts_with("|+") {
        "|+"
    } else if text.starts_with("|>") {
        "|>"
    } else {
        "|"
    }
}

fn strip_doc_continue_marker(text: &str) -> Option<&str> {
    text.strip_prefix("|+")
        .or_else(|| text.strip_prefix("|>"))
        .or_else(|| text.strip_prefix('|'))
}

fn normalized_continue_line_content(text: &str) -> &str {
    strip_doc_continue_marker(text).unwrap_or(text).trim_start()
}

fn format_doc_tag_prefix_override(prefix: &str, tag: &str) -> String {
    let tag = tag.strip_prefix('@').unwrap_or(tag);
    if prefix.contains('@') {
        format!("{prefix}{tag}")
    } else {
        format!("{prefix}@{tag}")
    }
}

fn normalized_doc_continue_marker_prefix(ctx: &FormatContext, marker: &str) -> String {
    if ctx.config.emmy_doc.space_after_description_dash {
        format!("--- {marker}")
    } else {
        format!("---{marker}")
    }
}

fn strip_single_comment_gap(text_after_dash: &str) -> &str {
    text_after_dash
        .strip_prefix(' ')
        .or_else(|| text_after_dash.strip_prefix('\t'))
        .unwrap_or(text_after_dash)
}

fn parse_generic_columns(input: &str) -> Vec<String> {
    let tokens: Vec<_> = input.split_whitespace().collect();
    match tokens.len() {
        0 => Vec::new(),
        1 => vec![tokens[0].to_string()],
        2 => vec![tokens[0].to_string(), tokens[1].to_string()],
        _ => vec![
            tokens[..tokens.len() - 2].join(" "),
            tokens[tokens.len() - 2..].join(" "),
        ],
    }
}

fn collect_structured_doc_tag_columns_by_line(
    ctx: &FormatContext,
    comment: &LuaComment,
    line_map: &DocCommentLineMap,
) -> Vec<Option<StructuredDocTagColumns>> {
    let mut structured_tags = vec![None; line_map.raw_lines.len()];
    let comment_start = comment.syntax().text_range().start();

    for child in comment.syntax().children() {
        let Some(tag) = LuaDocTag::cast(child.clone()) else {
            continue;
        };
        let relative_start =
            u32::from(child.text_range().start()).saturating_sub(u32::from(comment_start)) as usize;
        let line_index = line_index_for_offset(&line_map.line_start_offsets, relative_start);
        let Some(columns) = structured_doc_tag_columns_from_ast(ctx, &tag) else {
            continue;
        };
        if let Some(slot) = structured_tags.get_mut(line_index) {
            *slot = Some(columns);
        }
    }

    structured_tags
}

fn line_index_for_offset(line_start_offsets: &[usize], offset: usize) -> usize {
    match line_start_offsets.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    }
}

fn structured_doc_tag_columns_from_ast(
    ctx: &FormatContext,
    tag: &LuaDocTag,
) -> Option<StructuredDocTagColumns> {
    match tag {
        LuaDocTag::Class(tag) => Some(StructuredDocTagColumns {
            tag: "class".to_string(),
            head_columns: structured_class_columns(tag),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: false,
        }),
        LuaDocTag::Alias(tag) => Some(StructuredDocTagColumns {
            tag: "alias".to_string(),
            head_columns: Vec::new(),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: true,
        }),
        LuaDocTag::Generic(tag) => Some(StructuredDocTagColumns {
            tag: "generic".to_string(),
            head_columns: Vec::new(),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: true,
        }),
        LuaDocTag::Type(tag) => {
            let head_columns = structured_type_columns(ctx, tag);
            Some(StructuredDocTagColumns {
                tag: "type".to_string(),
                use_normalized_head_as_single_column: head_columns.is_empty(),
                head_columns,
                description: tag.get_description().map(|it| it.get_description_text()),
            })
        }
        LuaDocTag::Overload(tag) => Some(StructuredDocTagColumns {
            tag: "overload".to_string(),
            head_columns: Vec::new(),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: true,
        }),
        LuaDocTag::Param(tag) => Some(StructuredDocTagColumns {
            tag: "param".to_string(),
            head_columns: structured_param_columns(tag),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: false,
        }),
        LuaDocTag::Field(tag) => Some(StructuredDocTagColumns {
            tag: "field".to_string(),
            head_columns: structured_field_columns(tag),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: false,
        }),
        LuaDocTag::Return(tag) => Some(StructuredDocTagColumns {
            tag: "return".to_string(),
            head_columns: structured_return_columns(tag),
            description: tag.get_description().map(|it| it.get_description_text()),
            use_normalized_head_as_single_column: should_use_normalized_single_return_head(tag),
        }),
        _ => None,
    }
}

fn should_use_normalized_single_return_head(tag: &LuaDocTagReturn) -> bool {
    let info_list = tag.get_info_list();
    info_list.len() == 1 && info_list[0].1.is_none()
}

fn structured_class_columns(tag: &LuaDocTagClass) -> Vec<String> {
    let mut head = String::new();
    if let Some(type_flag) = tag.get_type_flag() {
        head.push_str(type_flag.syntax().text().to_string().trim());
        head.push(' ');
    }
    if let Some(name) = tag.get_name_token() {
        head.push_str(name.get_name_text());
    }
    if let Some(generic_decl) = tag.get_generic_decl() {
        head.push_str(generic_decl.syntax().text().to_string().trim());
    }
    if let Some(supers) = tag.get_supers() {
        let supers_text = supers
            .get_types()
            .map(|ty| ty.syntax().text().to_string().trim().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if !supers_text.is_empty() {
            head.push_str(": ");
            head.push_str(&supers_text);
        }
    }

    (!head.is_empty()).then_some(head).into_iter().collect()
}

fn structured_type_columns(ctx: &FormatContext, tag: &LuaDocTagType) -> Vec<String> {
    let mut types = tag.get_type_list();
    let Some(first_type) = types.next() else {
        return Vec::new();
    };

    if types.next().is_some() {
        return Vec::new();
    }

    match first_type {
        LuaDocType::Object(object) => vec![format_doc_object_type_inline(ctx, &object)],
        _ => Vec::new(),
    }
}

fn format_doc_object_type_inline(ctx: &FormatContext, object: &LuaDocObjectType) -> String {
    let fields = object
        .get_fields()
        .map(|field| field.syntax().text().to_string().trim().to_string())
        .collect::<Vec<_>>();

    if fields.is_empty() {
        return "{}".to_string();
    }

    if ctx.config.spacing.space_inside_braces {
        format!("{{ {} }}", fields.join(", "))
    } else {
        format!("{{{}}}", fields.join(", "))
    }
}

fn structured_single_head_columns(
    normalized_head: &str,
    structured_description: Option<&str>,
) -> Vec<String> {
    let head = structured_description
        .and_then(|description| strip_structured_description_suffix(normalized_head, description))
        .unwrap_or_else(|| normalized_head.trim().to_string());

    if head.is_empty() {
        Vec::new()
    } else {
        vec![head]
    }
}

fn strip_structured_description_suffix(normalized_head: &str, description: &str) -> Option<String> {
    let trimmed_head = normalized_head.trim_end();
    let trimmed_description = description.trim();
    if trimmed_description.is_empty() {
        return Some(trimmed_head.to_string());
    }

    trimmed_head
        .strip_suffix(trimmed_description)
        .map(str::trim_end)
        .map(str::to_string)
}

fn structured_columns_from_normalized_head(
    normalized_head: &str,
    structured_head_columns: &[String],
    structured_description: Option<&str>,
) -> Vec<String> {
    let normalized_head = structured_description
        .and_then(|description| strip_structured_description_suffix(normalized_head, description))
        .unwrap_or_else(|| normalized_head.trim().to_string());

    match structured_head_columns.len() {
        0 => Vec::new(),
        1 => structured_head_columns.to_vec(),
        2 => {
            let raw_first = structured_head_columns[0].trim();
            let normalized_first = collapse_spaces(raw_first);
            let first_variants = structured_first_column_variants(raw_first, &normalized_first);
            let rest = first_variants
                .iter()
                .find_map(|candidate| normalized_head.strip_prefix(candidate.as_str()))
                .map(str::trim_start)
                .unwrap_or(normalized_head.as_str());

            if rest.is_empty() {
                vec![normalized_first]
            } else {
                vec![normalized_first, rest.to_string()]
            }
        }
        _ => structured_head_columns.to_vec(),
    }
}

fn structured_first_column_variants(raw_first: &str, normalized_first: &str) -> Vec<String> {
    let mut variants = vec![raw_first.to_string()];
    if normalized_first != raw_first {
        variants.push(normalized_first.to_string());
    }

    if let Some(base) = normalized_first.strip_suffix('?') {
        variants.push(format!("{base} ?"));
    }

    variants.sort();
    variants.dedup();
    variants
}

fn structured_param_columns(tag: &LuaDocTagParam) -> Vec<String> {
    let mut name = tag
        .get_name_token()
        .map(|token| token.get_name_text().to_string())
        .or_else(|| tag.is_vararg().then(|| "...".to_string()))
        .unwrap_or_default();
    if tag.is_nullable() {
        name.push('?');
    }

    let type_text = tag
        .get_type()
        .map(|type_node| type_node.syntax().text().to_string().trim().to_string())
        .unwrap_or_default();

    if type_text.is_empty() {
        (!name.is_empty()).then_some(name).into_iter().collect()
    } else {
        vec![name, type_text]
    }
}

fn structured_field_columns(tag: &LuaDocTagField) -> Vec<String> {
    let mut key = String::new();
    if let Some(visibility) = tag.get_visibility_token() {
        key.push_str(visibility.get_text());
        key.push(' ');
    }
    if let Some(field_key) = tag.get_field_key() {
        key.push_str(field_key_text(&field_key).trim());
    }
    if tag.is_nullable() {
        key.push('?');
    }

    let type_text = tag
        .get_type()
        .map(|type_node| type_node.syntax().text().to_string().trim().to_string())
        .unwrap_or_default();

    if type_text.is_empty() {
        (!key.is_empty()).then_some(key).into_iter().collect()
    } else {
        vec![key, type_text]
    }
}

fn field_key_text(field_key: &LuaDocFieldKey) -> String {
    match field_key {
        LuaDocFieldKey::Name(name) => name.get_name_text().to_string(),
        LuaDocFieldKey::String(string) => format!("[{}]", string.get_text()),
        LuaDocFieldKey::Integer(integer) => format!("[{}]", integer.get_text()),
        LuaDocFieldKey::Type(typ) => {
            format!("[{}]", typ.syntax().text().to_string().trim())
        }
    }
}

fn structured_return_columns(tag: &LuaDocTagReturn) -> Vec<String> {
    let head = tag
        .get_info_list()
        .into_iter()
        .map(|(type_node, name_token)| match name_token {
            Some(name_token) => format!(
                "{} {}",
                type_node.syntax().text().to_string().trim(),
                name_token.get_name_text()
            ),
            None => type_node.syntax().text().to_string().trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    if head.is_empty() {
        Vec::new()
    } else {
        vec![head]
    }
}

fn first_structured_description_line(text: String) -> Option<String> {
    text.lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
