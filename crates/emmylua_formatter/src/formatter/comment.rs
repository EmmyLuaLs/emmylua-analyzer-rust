use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaComment, LuaDocDescription, LuaDocFieldKey, LuaDocGenericDeclList,
    LuaDocTag, LuaDocTagAlias, LuaDocTagClass, LuaDocTagField, LuaDocTagGeneric, LuaDocTagOverload,
    LuaDocTagParam, LuaDocTagReturn, LuaDocTagType, LuaKind, LuaSyntaxElement, LuaSyntaxKind,
    LuaSyntaxNode, LuaTokenKind,
};
use rowan::TextRange;

use crate::config::LuaFormatConfig;
use crate::ir::{self, DocIR};

use super::trivia::has_non_trivia_before_on_same_line;

/// Format a Comment node.
///
/// Dispatches between three comment types:
/// - Doc comments (`---@...`): walk the syntax tree, normalize whitespace
/// - Long comments (`--[[ ... ]]`): preserve content as-is
/// - Normal comments (`-- ...`): preserve text with trimming
pub fn format_comment(config: &LuaFormatConfig, comment: &LuaComment) -> Vec<DocIR> {
    match classify_comment(comment) {
        CommentKind::Long => vec![ir::source_node_trimmed(comment.syntax().clone())],
        CommentKind::Doc => format_doc_comment(config, comment),
        CommentKind::Normal => format_normal_comment(comment),
    }
}

/// Format a doc comment by walking its syntax tree token-by-token.
///
/// Only flat formatting is used (Text, Space, HardLine) — no Group/SoftLine
/// since comments cannot have breaking rules.
fn format_doc_comment(config: &LuaFormatConfig, comment: &LuaComment) -> Vec<DocIR> {
    let lines = parse_doc_comment_lines(comment);
    let rendered = render_doc_comment_lines(config, &lines);
    let mut docs = Vec::new();
    for (index, line) in rendered.into_iter().enumerate() {
        if index > 0 {
            docs.push(ir::hard_line());
        }
        if !line.is_empty() {
            docs.push(ir::text(line));
        }
    }
    docs
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommentKind {
    Long,
    Doc,
    Normal,
}

fn classify_comment(comment: &LuaComment) -> CommentKind {
    let Some(first_token) = comment.syntax().first_token() else {
        return CommentKind::Normal;
    };

    match first_token.kind().into() {
        LuaTokenKind::TkLongCommentStart => CommentKind::Long,
        LuaTokenKind::TkDocStart
        | LuaTokenKind::TkDocLongStart
        | LuaTokenKind::TkDocContinue
        | LuaTokenKind::TkDocContinueOr => CommentKind::Doc,
        LuaTokenKind::TkNormalStart => {
            if first_token.text().starts_with("---") || comment.get_doc_tags().next().is_some() {
                CommentKind::Doc
            } else {
                CommentKind::Normal
            }
        }
        _ => {
            if comment.get_doc_tags().next().is_some() {
                CommentKind::Doc
            } else {
                CommentKind::Normal
            }
        }
    }
}

fn format_normal_comment(comment: &LuaComment) -> Vec<DocIR> {
    let Some(description) = comment.get_description() else {
        return vec![ir::source_node_trimmed(comment.syntax().clone())];
    };

    let rendered = render_normal_comment_lines(&description);
    let mut docs = Vec::new();
    for (index, line) in rendered.into_iter().enumerate() {
        if index > 0 {
            docs.push(ir::hard_line());
        }
        if !line.is_empty() {
            docs.push(ir::text(line));
        }
    }
    docs
}

fn render_normal_comment_lines(description: &LuaDocDescription) -> Vec<String> {
    let mut lines = Vec::new();
    let mut prefix: Option<String> = None;
    let mut gap = String::new();
    let mut detail = String::new();

    for child in description.syntax().children_with_tokens() {
        let LuaSyntaxElement::Token(token) = child else {
            continue;
        };

        match token.kind().into() {
            LuaTokenKind::TkNormalStart | LuaTokenKind::TKNonStdComment => {
                if let Some(prefix_text) = prefix.take() {
                    lines.push(render_normal_comment_line(&prefix_text, &gap, &detail));
                }
                prefix = Some(token.text().to_string());
                gap.clear();
                detail.clear();
            }
            LuaTokenKind::TkWhitespace => {
                if prefix.is_some() && detail.is_empty() {
                    gap.push_str(token.text());
                } else if !detail.is_empty() {
                    detail.push_str(token.text());
                }
            }
            LuaTokenKind::TkDocDetail => {
                detail.push_str(token.text());
            }
            LuaTokenKind::TkEndOfLine => {
                if let Some(prefix_text) = prefix.take() {
                    lines.push(render_normal_comment_line(&prefix_text, &gap, &detail));
                }
                gap.clear();
                detail.clear();
            }
            _ => {}
        }
    }

    if let Some(prefix_text) = prefix.take() {
        lines.push(render_normal_comment_line(&prefix_text, &gap, &detail));
    }

    lines
}

fn render_normal_comment_line(prefix: &str, gap: &str, detail: &str) -> String {
    let mut line = prefix.trim_end().to_string();
    if !gap.is_empty() || !detail.is_empty() {
        line.push_str(gap);
        line.push_str(detail);
    }
    line.trim_end().to_string()
}

#[derive(Debug, Clone)]
enum DocCommentLine {
    Empty,
    Description(String),
    Class {
        body: String,
        desc: Option<String>,
    },
    Alias {
        body: String,
        desc: Option<String>,
    },
    Type {
        body: String,
        desc: Option<String>,
    },
    Generic {
        body: String,
        desc: Option<String>,
    },
    Overload {
        body: String,
        desc: Option<String>,
    },
    Param {
        name: String,
        ty: String,
        desc: Option<String>,
    },
    Field {
        key: String,
        ty: String,
        desc: Option<String>,
    },
    Return {
        body: String,
        desc: Option<String>,
    },
    Raw(String),
}

#[derive(Default)]
struct PendingDocLine {
    prefix: Option<String>,
    tag: Option<LuaDocTag>,
    description: Option<LuaDocDescription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AlignableDocTagKind {
    Class,
    Alias,
    Type,
    Generic,
    Overload,
    Param,
    Field,
    Return,
}

fn parse_doc_comment_lines(comment: &LuaComment) -> Vec<DocCommentLine> {
    let mut lines = Vec::new();
    let mut pending = PendingDocLine::default();

    for child in comment.syntax().children_with_tokens() {
        match child {
            LuaSyntaxElement::Token(token) => match token.kind().into() {
                LuaTokenKind::TkWhitespace => {}
                LuaTokenKind::TkDocStart
                | LuaTokenKind::TkDocLongStart
                | LuaTokenKind::TkNormalStart
                | LuaTokenKind::TkDocContinue => {
                    pending.prefix = Some(token.text().to_string());
                }
                LuaTokenKind::TkEndOfLine => {
                    lines.push(finalize_doc_comment_line(&mut pending));
                }
                _ => {}
            },
            LuaSyntaxElement::Node(node) => match node.kind().into() {
                LuaSyntaxKind::DocDescription => {
                    pending.description = LuaDocDescription::cast(node);
                }
                syntax_kind if LuaDocTag::can_cast(syntax_kind) => {
                    pending.tag = LuaDocTag::cast(node);
                }
                _ => {}
            },
        }
    }

    if pending.prefix.is_some() || pending.tag.is_some() || pending.description.is_some() {
        lines.push(finalize_doc_comment_line(&mut pending));
    }

    lines
}

fn finalize_doc_comment_line(pending: &mut PendingDocLine) -> DocCommentLine {
    let prefix = pending.prefix.take().unwrap_or_default();
    let tag = pending.tag.take();
    let description = pending.description.take();

    if let Some(tag) = tag {
        build_doc_tag_line(&prefix, tag, description)
    } else if let Some(description) = description {
        let text = normalize_single_line_spaces(&description.get_description_text());
        if text.is_empty() {
            DocCommentLine::Raw(prefix.trim_end().to_string())
        } else {
            DocCommentLine::Description(text)
        }
    } else if prefix.is_empty() {
        DocCommentLine::Empty
    } else {
        DocCommentLine::Raw(prefix.trim_end().to_string())
    }
}

fn build_doc_tag_line(
    prefix: &str,
    tag: LuaDocTag,
    description: Option<LuaDocDescription>,
) -> DocCommentLine {
    if prefix != "---@" {
        return raw_doc_tag_line(prefix, tag.syntax().text().to_string(), description);
    }

    match tag {
        LuaDocTag::Class(class_tag) => {
            build_class_doc_line(prefix, &class_tag, description.clone()).unwrap_or_else(|| {
                raw_doc_tag_line(prefix, class_tag.syntax().text().to_string(), description)
            })
        }
        LuaDocTag::Alias(alias) => build_alias_doc_line(prefix, &alias, description.clone())
            .unwrap_or_else(|| {
                raw_doc_tag_line(prefix, alias.syntax().text().to_string(), description)
            }),
        LuaDocTag::Type(type_tag) => build_type_doc_line(prefix, &type_tag, description.clone())
            .unwrap_or_else(|| {
                raw_doc_tag_line(prefix, type_tag.syntax().text().to_string(), description)
            }),
        LuaDocTag::Generic(generic) => {
            build_generic_doc_line(prefix, &generic, description.clone()).unwrap_or_else(|| {
                raw_doc_tag_line(prefix, generic.syntax().text().to_string(), description)
            })
        }
        LuaDocTag::Overload(overload) => {
            build_overload_doc_line(prefix, &overload, description.clone()).unwrap_or_else(|| {
                raw_doc_tag_line(prefix, overload.syntax().text().to_string(), description)
            })
        }
        LuaDocTag::Param(param) => build_param_doc_line(prefix, &param, description.clone())
            .unwrap_or_else(|| {
                raw_doc_tag_line(prefix, param.syntax().text().to_string(), description)
            }),
        LuaDocTag::Field(field) => build_field_doc_line(prefix, &field, description.clone())
            .unwrap_or_else(|| {
                raw_doc_tag_line(prefix, field.syntax().text().to_string(), description)
            }),
        LuaDocTag::Return(ret) => build_return_doc_line(prefix, &ret, description.clone())
            .unwrap_or_else(|| {
                raw_doc_tag_line(prefix, ret.syntax().text().to_string(), description)
            }),
        other => raw_doc_tag_line(prefix, other.syntax().text().to_string(), description),
    }
}

fn build_class_doc_line(
    _prefix: &str,
    tag: &LuaDocTagClass,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let mut body = tag.get_name_token()?.get_name_text().to_string();
    if let Some(generic_decl) = tag.get_generic_decl() {
        body.push_str(&single_line_syntax_text(&generic_decl)?);
    }
    if let Some(supers) = tag.get_supers() {
        body.push_str(": ");
        body.push_str(&single_line_syntax_text(&supers)?);
    }
    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Class { body, desc })
}

fn build_alias_doc_line(
    _prefix: &str,
    tag: &LuaDocTagAlias,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let body = raw_doc_tag_body_text("alias", tag)?;
    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Alias { body, desc })
}

fn build_type_doc_line(
    _prefix: &str,
    tag: &LuaDocTagType,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let mut parts = Vec::new();
    for ty in tag.get_type_list() {
        parts.push(single_line_syntax_text(&ty)?);
    }
    if parts.is_empty() {
        return None;
    }
    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Type {
        body: parts.join(", "),
        desc,
    })
}

fn build_generic_doc_line(
    _prefix: &str,
    tag: &LuaDocTagGeneric,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let body = generic_decl_list_text(&tag.get_generic_decl_list()?)?;
    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Generic { body, desc })
}

fn build_overload_doc_line(
    _prefix: &str,
    tag: &LuaDocTagOverload,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let body = single_line_syntax_text(&tag.get_type()?)?;
    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Overload { body, desc })
}

fn raw_doc_tag_line(
    prefix: &str,
    body: String,
    description: Option<LuaDocDescription>,
) -> DocCommentLine {
    if body.contains('\n') {
        return DocCommentLine::Raw(format!("{prefix}{body}").trim_end().to_string());
    }

    let mut line = format!("{prefix}{}", normalize_single_line_spaces(&body));
    if let Some(desc) = inline_doc_description_text(description)
        && !desc.is_empty()
    {
        line.push(' ');
        line.push_str(&desc);
    }
    DocCommentLine::Raw(line)
}

fn build_param_doc_line(
    _prefix: &str,
    tag: &LuaDocTagParam,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let mut name = if tag.is_vararg() {
        "...".to_string()
    } else {
        tag.get_name_token()?.get_name_text().to_string()
    };
    if tag.is_nullable() {
        name.push('?');
    }

    let ty = single_line_syntax_text(&tag.get_type()?)?;
    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Param { name, ty, desc })
}

fn build_field_doc_line(
    _prefix: &str,
    tag: &LuaDocTagField,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let mut key = String::new();
    if let Some(visibility) = tag.get_visibility_token() {
        key.push_str(visibility.syntax().text());
        key.push(' ');
    }
    key.push_str(&field_key_text(&tag.get_field_key()?)?);
    if tag.is_nullable() {
        key.push('?');
    }

    let ty = single_line_syntax_text(&tag.get_type()?)?;
    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Field { key, ty, desc })
}

fn build_return_doc_line(
    _prefix: &str,
    tag: &LuaDocTagReturn,
    description: Option<LuaDocDescription>,
) -> Option<DocCommentLine> {
    let mut parts = Vec::new();
    for (ty, name) in tag.get_info_list() {
        let mut part = single_line_syntax_text(&ty)?;
        if let Some(name) = name {
            part.push(' ');
            part.push_str(name.get_name_text());
        }
        parts.push(part);
    }

    if parts.is_empty() {
        parts.push(single_line_syntax_text(&tag.get_first_type()?)?);
    }

    let desc = inline_doc_description_text(description);
    Some(DocCommentLine::Return {
        body: parts.join(", "),
        desc,
    })
}

fn field_key_text(key: &LuaDocFieldKey) -> Option<String> {
    Some(match key {
        LuaDocFieldKey::Name(name) => name.get_name_text().to_string(),
        LuaDocFieldKey::String(string) => format!("[{}]", string.syntax().text()),
        LuaDocFieldKey::Integer(integer) => format!("[{}]", integer.syntax().text()),
        LuaDocFieldKey::Type(typ) => format!("[{}]", single_line_syntax_text(typ)?),
    })
}

fn single_line_syntax_text(node: &impl LuaAstNode) -> Option<String> {
    let text = node.syntax().text().to_string();
    if text.contains('\n') {
        None
    } else {
        Some(normalize_single_line_spaces(&text))
    }
}

fn inline_doc_description_text(description: Option<LuaDocDescription>) -> Option<String> {
    let description = description?;
    let text = normalize_single_line_spaces(&description.get_description_text());
    if text.is_empty() { None } else { Some(text) }
}

fn normalize_single_line_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn generic_decl_list_text(list: &LuaDocGenericDeclList) -> Option<String> {
    let text = single_line_syntax_text(list)?;
    Some(text)
}

fn raw_doc_tag_body_text<T: LuaAstNode>(tag_name: &str, node: &T) -> Option<String> {
    let text = node.syntax().text().to_string();
    if text.contains('\n') {
        return None;
    }

    let body = text.trim().strip_prefix(tag_name)?.trim_start();
    Some(body.trim_end().to_string())
}

fn render_doc_comment_lines(config: &LuaFormatConfig, lines: &[DocCommentLine]) -> Vec<String> {
    let mut rendered = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let kind = alignable_doc_tag_kind(&lines[index]);
        if let Some(kind) = kind
            && should_align_doc_tag_kind(config, kind)
        {
            let mut group_end = index + 1;
            while group_end < lines.len() && alignable_doc_tag_kind(&lines[group_end]) == Some(kind)
            {
                group_end += 1;
            }

            if group_end - index >= 2 {
                rendered.extend(render_aligned_doc_tag_group(
                    config,
                    &lines[index..group_end],
                    kind,
                ));
                index = group_end;
                continue;
            }
        }

        rendered.push(render_single_doc_comment_line(config, &lines[index]));
        index += 1;
    }
    rendered
}

fn should_align_doc_tag_kind(config: &LuaFormatConfig, kind: AlignableDocTagKind) -> bool {
    match kind {
        AlignableDocTagKind::Class
        | AlignableDocTagKind::Alias
        | AlignableDocTagKind::Type
        | AlignableDocTagKind::Generic
        | AlignableDocTagKind::Overload => config.should_align_emmy_doc_declaration_tags(),
        AlignableDocTagKind::Param | AlignableDocTagKind::Field | AlignableDocTagKind::Return => {
            config.should_align_emmy_doc_reference_tags()
        }
    }
}

fn alignable_doc_tag_kind(line: &DocCommentLine) -> Option<AlignableDocTagKind> {
    match line {
        DocCommentLine::Class { .. } => Some(AlignableDocTagKind::Class),
        DocCommentLine::Alias { .. } => Some(AlignableDocTagKind::Alias),
        DocCommentLine::Type { .. } => Some(AlignableDocTagKind::Type),
        DocCommentLine::Generic { .. } => Some(AlignableDocTagKind::Generic),
        DocCommentLine::Overload { .. } => Some(AlignableDocTagKind::Overload),
        DocCommentLine::Param { .. } => Some(AlignableDocTagKind::Param),
        DocCommentLine::Field { .. } => Some(AlignableDocTagKind::Field),
        DocCommentLine::Return { .. } => Some(AlignableDocTagKind::Return),
        _ => None,
    }
}

fn render_aligned_doc_tag_group(
    config: &LuaFormatConfig,
    lines: &[DocCommentLine],
    kind: AlignableDocTagKind,
) -> Vec<String> {
    let gap = " ".repeat(config.emmy_doc.tag_spacing.max(1));
    match kind {
        AlignableDocTagKind::Class => render_body_aligned_doc_group(config, lines, "class"),
        AlignableDocTagKind::Alias => render_alias_doc_group(config, lines),
        AlignableDocTagKind::Type => render_body_aligned_doc_group(config, lines, "type"),
        AlignableDocTagKind::Generic => render_body_aligned_doc_group(config, lines, "generic"),
        AlignableDocTagKind::Overload => render_body_aligned_doc_group(config, lines, "overload"),
        AlignableDocTagKind::Param => {
            let max_name = lines
                .iter()
                .filter_map(|line| match line {
                    DocCommentLine::Param { name, .. } => Some(name.len()),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let max_type = lines
                .iter()
                .filter_map(|line| match line {
                    DocCommentLine::Param { ty, .. } => Some(ty.len()),
                    _ => None,
                })
                .max()
                .unwrap_or(0);

            lines
                .iter()
                .map(|line| match line {
                    DocCommentLine::Param { name, ty, desc } => {
                        let mut rendered = format!(
                            "---@param{gap}{name:<max_name$}{gap}{ty:<max_type$}",
                            gap = gap,
                            name = name,
                            max_name = max_name,
                            ty = ty,
                            max_type = max_type,
                        );
                        if let Some(desc) = desc {
                            rendered.push_str(&gap);
                            rendered.push_str(desc);
                        }
                        rendered.trim_end().to_string()
                    }
                    other => render_single_doc_comment_line(config, other),
                })
                .collect()
        }
        AlignableDocTagKind::Field => {
            let max_key = lines
                .iter()
                .filter_map(|line| match line {
                    DocCommentLine::Field { key, .. } => Some(key.len()),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let max_type = lines
                .iter()
                .filter_map(|line| match line {
                    DocCommentLine::Field { ty, .. } => Some(ty.len()),
                    _ => None,
                })
                .max()
                .unwrap_or(0);

            lines
                .iter()
                .map(|line| match line {
                    DocCommentLine::Field { key, ty, desc } => {
                        let mut rendered = format!(
                            "---@field{gap}{key:<max_key$}{gap}{ty:<max_type$}",
                            gap = gap,
                            key = key,
                            max_key = max_key,
                            ty = ty,
                            max_type = max_type,
                        );
                        if let Some(desc) = desc {
                            rendered.push_str(&gap);
                            rendered.push_str(desc);
                        }
                        rendered.trim_end().to_string()
                    }
                    other => render_single_doc_comment_line(config, other),
                })
                .collect()
        }
        AlignableDocTagKind::Return => {
            let max_body = lines
                .iter()
                .filter_map(|line| match line {
                    DocCommentLine::Return { body, .. } => Some(body.len()),
                    _ => None,
                })
                .max()
                .unwrap_or(0);

            lines
                .iter()
                .map(|line| match line {
                    DocCommentLine::Return { body, desc } => {
                        let mut rendered = format!(
                            "---@return{gap}{body:<max_body$}",
                            gap = gap,
                            body = body,
                            max_body = max_body,
                        );
                        if let Some(desc) = desc {
                            rendered.push_str(&gap);
                            rendered.push_str(desc);
                        }
                        rendered.trim_end().to_string()
                    }
                    other => render_single_doc_comment_line(config, other),
                })
                .collect()
        }
    }
}

fn render_alias_doc_group(config: &LuaFormatConfig, lines: &[DocCommentLine]) -> Vec<String> {
    let gap = " ".repeat(config.emmy_doc.tag_spacing.max(1));
    let max_body = lines
        .iter()
        .filter_map(|line| match line {
            DocCommentLine::Alias { body, .. } => Some(body.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| match line {
            DocCommentLine::Alias { body, desc } => {
                let mut rendered = format!(
                    "---@alias{gap}{body:<max_body$}",
                    gap = gap,
                    body = body,
                    max_body = max_body,
                );
                if let Some(desc) = desc {
                    rendered.push_str(&gap);
                    rendered.push_str(desc);
                }
                rendered.trim_end().to_string()
            }
            other => render_single_doc_comment_line(config, other),
        })
        .collect()
}

fn render_body_aligned_doc_group(
    config: &LuaFormatConfig,
    lines: &[DocCommentLine],
    tag_name: &str,
) -> Vec<String> {
    let gap = " ".repeat(config.emmy_doc.tag_spacing.max(1));
    let max_body = lines
        .iter()
        .filter_map(|line| doc_line_body_and_desc(line).map(|(body, _)| body.len()))
        .max()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if let Some((body, desc)) = doc_line_body_and_desc(line) {
                let mut rendered = format!(
                    "---@{tag_name}{gap}{body:<max_body$}",
                    tag_name = tag_name,
                    gap = gap,
                    body = body,
                    max_body = max_body,
                );
                if let Some(desc) = desc {
                    rendered.push_str(&gap);
                    rendered.push_str(desc);
                }
                rendered.trim_end().to_string()
            } else {
                render_single_doc_comment_line(config, line)
            }
        })
        .collect()
}

fn doc_line_body_and_desc(line: &DocCommentLine) -> Option<(&str, Option<&String>)> {
    match line {
        DocCommentLine::Class { body, desc }
        | DocCommentLine::Alias { body, desc }
        | DocCommentLine::Type { body, desc }
        | DocCommentLine::Generic { body, desc }
        | DocCommentLine::Overload { body, desc }
        | DocCommentLine::Return { body, desc } => Some((body.as_str(), desc.as_ref())),
        _ => None,
    }
}

fn render_single_doc_comment_line(config: &LuaFormatConfig, line: &DocCommentLine) -> String {
    let gap = " ".repeat(config.emmy_doc.tag_spacing.max(1));
    match line {
        DocCommentLine::Empty => String::new(),
        DocCommentLine::Description(text) => {
            if config.emmy_doc.space_after_description_dash {
                format!("--- {text}")
            } else {
                format!("---{text}")
            }
        }
        DocCommentLine::Raw(text) => text.clone(),
        DocCommentLine::Class { body, desc } => {
            let mut rendered = format!("---@class{gap}{body}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
        DocCommentLine::Alias { body, desc } => {
            let mut rendered = format!("---@alias{gap}{body}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
        DocCommentLine::Type { body, desc } => {
            let mut rendered = format!("---@type{gap}{body}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
        DocCommentLine::Generic { body, desc } => {
            let mut rendered = format!("---@generic{gap}{body}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
        DocCommentLine::Overload { body, desc } => {
            let mut rendered = format!("---@overload{gap}{body}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
        DocCommentLine::Param { name, ty, desc } => {
            let mut rendered = format!("---@param{gap}{name}{gap}{ty}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
        DocCommentLine::Field { key, ty, desc } => {
            let mut rendered = format!("---@field{gap}{key}{gap}{ty}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
        DocCommentLine::Return { body, desc } => {
            let mut rendered = format!("---@return{gap}{body}");
            if let Some(desc) = desc {
                rendered.push_str(&gap);
                rendered.push_str(desc);
            }
            rendered
        }
    }
}

/// Collect "orphan" comments in a syntax node.
///
/// When a Block is empty (e.g. `if x then -- comment end`),
/// comments may become direct children of the parent statement node rather than the Block.
/// This function collects those comments and returns the formatted IR.
pub fn collect_orphan_comments(config: &LuaFormatConfig, node: &LuaSyntaxNode) -> Vec<DocIR> {
    let mut docs = Vec::new();
    for child in node.children() {
        if child.kind() == LuaKind::Syntax(LuaSyntaxKind::Comment)
            && let Some(comment) = LuaComment::cast(child)
        {
            if !docs.is_empty() {
                docs.push(ir::hard_line());
            }
            docs.extend(format_comment(config, &comment));
        }
    }
    docs
}
/// Extract a trailing comment on the same line after a syntax node.
/// Returns the raw comment docs (NOT wrapped in LineSuffix) and the text range.
pub fn extract_trailing_comment(node: &LuaSyntaxNode) -> Option<(Vec<DocIR>, TextRange)> {
    for child in node.children() {
        if child.kind() != LuaKind::Syntax(LuaSyntaxKind::Comment)
            || !has_non_trivia_before_on_same_line(&child)
            || has_non_trivia_after_on_same_line(&child)
        {
            continue;
        }

        let comment = LuaComment::cast(child.clone())?;
        if child.text().contains_char('\n') {
            return None;
        }

        let comment_text = render_single_line_comment_text(&comment)
            .unwrap_or_else(|| child.text().to_string().trim_end().to_string());

        return Some((vec![ir::text(comment_text)], child.text_range()));
    }

    let mut next = node.next_sibling_or_token();

    // Look ahead at most 4 elements (skipping whitespace, commas, semicolons)
    for _ in 0..4 {
        let sibling = next.as_ref()?;
        match sibling.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {}
            LuaKind::Token(LuaTokenKind::TkSemicolon) => {}
            LuaKind::Token(LuaTokenKind::TkComma) => {}
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                let comment_node = sibling.as_node()?;
                let comment = LuaComment::cast(comment_node.clone())?;

                // Only single-line comments are treated as trailing comments
                if comment_node.text().contains_char('\n') {
                    return None;
                }

                let comment_text = render_single_line_comment_text(&comment)
                    .unwrap_or_else(|| comment_node.text().to_string().trim_end().to_string());

                let range = comment_node.text_range();
                return Some((vec![ir::text(comment_text)], range));
            }
            _ => return None,
        }
        next = sibling.next_sibling_or_token();
    }

    None
}

fn has_non_trivia_after_on_same_line(node: &LuaSyntaxNode) -> bool {
    let mut next = node.next_sibling_or_token();

    while let Some(element) = next {
        match element.kind() {
            LuaKind::Token(LuaTokenKind::TkWhitespace) => {
                next = element.next_sibling_or_token();
            }
            LuaKind::Token(LuaTokenKind::TkEndOfLine) => {
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

fn render_single_line_comment_text(comment: &LuaComment) -> Option<String> {
    match classify_comment(comment) {
        CommentKind::Long => Some(comment.syntax().text().to_string().trim_end().to_string()),
        CommentKind::Normal => {
            let description = comment.get_description()?;
            let lines = render_normal_comment_lines(&description);
            if lines.len() == 1 {
                lines.into_iter().next()
            } else {
                None
            }
        }
        CommentKind::Doc => None,
    }
}

pub fn trailing_comment_prefix(config: &LuaFormatConfig) -> Vec<DocIR> {
    let gap = config.comments.line_comment_min_spaces_before.max(1);
    (0..gap).map(|_| ir::space()).collect()
}

/// Format a trailing comment as LineSuffix (for non-grouped use).
pub fn format_trailing_comment(
    config: &LuaFormatConfig,
    node: &LuaSyntaxNode,
) -> Option<(DocIR, TextRange)> {
    let (docs, range) = extract_trailing_comment(node)?;
    let mut suffix_content = trailing_comment_prefix(config);
    suffix_content.extend(docs);
    Some((ir::line_suffix(suffix_content), range))
}
