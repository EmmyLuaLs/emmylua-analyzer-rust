//! Doc text for the export: descriptions, deprecation reasons and extra tags (`@see`, unknown
//! tags), read from the doc comments. Semantic facts carry types, not text.

use emmylua_code_analysis::{Decl, FileId, Member, SemanticId, SemanticModel, Signature, TypeDef};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaChunk, LuaComment, LuaCommentOwner, LuaDocDescription,
    LuaDocDescriptionOwner, LuaDocTag, LuaStat, LuaSyntaxId, LuaTableField,
};
use rowan::TextSize;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocText {
    pub description: Option<String>,
    /// A `---@deprecated` tag, which the facts don't record for every member.
    pub deprecated: bool,
    pub deprecation_reason: Option<String>,
    /// `(tag name, content)` for `@see` and unknown tags, in source order.
    pub tags: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct SignatureText {
    /// `@param` descriptions by parameter name.
    pub params: Vec<(String, String)>,
    /// Each `@return` value, in order.
    pub returns: Vec<ReturnText>,
}

#[derive(Debug, Clone, Default)]
pub struct ReturnText {
    pub name: Option<String>,
    pub description: Option<String>,
}

impl SignatureText {
    pub fn param_description(&self, name: &str) -> Option<String> {
        self.params
            .iter()
            .find(|(param, _)| param == name)
            .map(|(_, description)| description.clone())
    }
}

/// `---@class`, `---@enum` or `---@alias`: the comment's description, else the tag's own. A type
/// always has a description, empty if none is written (as 0.25 exported it).
pub fn type_def_text(model: &SemanticModel<'_>, def: &TypeDef) -> DocText {
    let Some(comment) = comment_at(model, def.file_id, def.name_range.start()) else {
        return DocText {
            description: Some(String::new()),
            ..Default::default()
        };
    };
    let mut text = comments_text(std::slice::from_ref(&comment));
    if text.description.is_none() {
        let descriptions = comment
            .get_doc_tags()
            .find(|tag| tag.syntax().text_range().contains_range(def.name_range))
            .map(|tag| match tag {
                LuaDocTag::Class(tag) => tag.get_descriptions(),
                LuaDocTag::Enum(tag) => tag.get_descriptions(),
                LuaDocTag::Alias(tag) => tag.get_descriptions(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        text.description = Some(join_descriptions(&descriptions, true).unwrap_or_default());
    }
    text
}

/// A `---@field` member: the tag's own description. A runtime member: the comment on its
/// statement or table field, else the description of an inline `---@type`.
pub fn member_text(model: &SemanticModel<'_>, member: &Member) -> DocText {
    let SemanticId::Member(key) = &member.id else {
        return DocText::default();
    };
    if matches!(member.owner, SemanticId::TypeDef(_)) {
        let descriptions = comment_at(model, key.file_id, key.key_range.start())
            .and_then(|comment| {
                comment.get_doc_tags().find_map(|tag| match tag {
                    LuaDocTag::Field(field)
                        if field.get_field_key_range() == Some(key.key_range) =>
                    {
                        Some(field.get_descriptions())
                    }
                    _ => None,
                })
            })
            .unwrap_or_default();
        return DocText {
            description: join_descriptions(&descriptions, true),
            ..Default::default()
        };
    }

    let comments = without_type_defs(owner_comments_at(model, key.file_id, key.key_range.start()));
    let mut text = comments_text(&comments);
    if text.description.is_none() {
        text.description = comments.iter().find_map(|comment| {
            comment.get_doc_tags().find_map(|tag| match tag {
                LuaDocTag::Type(tag) => description_text(tag.get_description(), true),
                _ => None,
            })
        });
    }
    text
}

/// A declaration: the comment on its statement.
pub fn decl_text(model: &SemanticModel<'_>, file_id: FileId, decl: &Decl) -> DocText {
    decl.owner_syntax
        .map(|syntax| {
            comments_text(&without_type_defs(comments_of_syntax(
                model, file_id, syntax,
            )))
        })
        .unwrap_or_default()
}

/// A comment with `---@class`, `---@enum` or `---@alias` documents the type, not the variable it's
/// on.
fn without_type_defs(comments: Vec<LuaComment>) -> Vec<LuaComment> {
    comments
        .into_iter()
        .filter(|comment| {
            !comment.get_doc_tags().any(|tag| {
                matches!(
                    tag,
                    LuaDocTag::Class(_) | LuaDocTag::Enum(_) | LuaDocTag::Alias(_)
                )
            })
        })
        .collect()
}

/// `@param` and `@return` descriptions, from the comment on the function's statement.
pub fn signature_text(model: &SemanticModel<'_>, signature: &Signature) -> SignatureText {
    let mut text = SignatureText::default();
    let Some(owner) = signature.owner_syntax else {
        return text;
    };
    for comment in comments_of_syntax(model, signature.file_id, owner) {
        for tag in comment.get_doc_tags() {
            match tag {
                LuaDocTag::Param(param) => {
                    let name = if param.is_vararg() {
                        Some("...".to_string())
                    } else {
                        param
                            .get_name_token()
                            .map(|name| name.get_name_text().to_string())
                    };
                    if let Some(name) = name
                        && let Some(description) = description_text(param.get_description(), false)
                    {
                        text.params.push((name, description));
                    }
                }
                LuaDocTag::Return(ret) => {
                    let description = description_text(ret.get_description(), false);
                    for (_, name) in ret.get_info_list() {
                        text.returns.push(ReturnText {
                            name: name.map(|name| name.get_name_text().to_string()),
                            description: description.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    text
}

fn comments_text(comments: &[LuaComment]) -> DocText {
    let mut text = DocText::default();
    for comment in comments {
        if text.description.is_none() {
            text.description = description_text(comment.get_description(), true);
        }
        for tag in comment.get_doc_tags() {
            match tag {
                LuaDocTag::Deprecated(tag) => {
                    text.deprecated = true;
                    text.deprecation_reason = tag.get_description().and_then(|description| {
                        let text = description.get_description_text();
                        let reason = text.lines().next()?.trim_end();
                        (!reason.is_empty()).then(|| reason.to_string())
                    });
                }
                LuaDocTag::See(tag) => {
                    if let Some(content) = tag.get_see_content() {
                        let mut see = content.get_text().to_string();
                        if let Some(description) = tag.get_description() {
                            see.push(' ');
                            see.push_str(&description.get_description_text());
                        }
                        text.tags.push(("see".to_string(), see));
                    }
                }
                LuaDocTag::Other(tag) => {
                    if let Some(name) = tag.get_tag_name() {
                        let content = description_text(tag.get_description(), true);
                        text.tags.push((name, content.unwrap_or_default()));
                    }
                }
                _ => {}
            }
        }
    }
    text
}

fn description_text(
    description: Option<LuaDocDescription>,
    strip_start_char: bool,
) -> Option<String> {
    let text = preprocess_description(&description?.get_description_text(), strip_start_char);
    (!text.is_empty()).then_some(text)
}

/// Several descriptions of one tag (its line and the lines below), as paragraphs.
fn join_descriptions(descriptions: &[LuaDocDescription], strip_start_char: bool) -> Option<String> {
    let text = descriptions
        .iter()
        .map(|description| {
            preprocess_description(&description.get_description_text(), strip_start_char)
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

/// Strips a leading `#` or `@` (the `---@field x T # description` style) and the one space that
/// usually follows `---`, keeping the rest of the indentation.
fn preprocess_description(mut description: &str, strip_start_char: bool) -> String {
    if strip_start_char && description.starts_with(['#', '@']) {
        description = description.trim_start_matches(['#', '@']);
    }
    let mut result = String::new();
    let mut starts_with_one_space = None;
    for line in description.lines() {
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent == line.chars().count() {
            result.push('\n');
            continue;
        }
        let starts_with_one_space = *starts_with_one_space.get_or_insert(indent == 1);
        let line = match line.chars().next() {
            Some(c) if starts_with_one_space && c.is_whitespace() => &line[c.len_utf8()..],
            _ => line,
        };
        result.push_str(line);
        result.push('\n');
    }
    result.trim_end().to_string()
}

/// The doc comment containing an offset (a type's name, a `@field`'s key).
fn comment_at(model: &SemanticModel<'_>, file_id: FileId, offset: TextSize) -> Option<LuaComment> {
    let tree = model.syntax_tree_of(file_id)?;
    let token = tree.get_red_root().token_at_offset(offset).right_biased()?;
    token.parent_ancestors().find_map(LuaComment::cast)
}

/// The comments on the statement or table field containing an offset (a member's key).
fn owner_comments_at(
    model: &SemanticModel<'_>,
    file_id: FileId,
    offset: TextSize,
) -> Vec<LuaComment> {
    let Some(tree) = model.syntax_tree_of(file_id) else {
        return Vec::new();
    };
    let Some(chunk) = LuaChunk::cast(tree.get_red_root()) else {
        return Vec::new();
    };
    let Some(token) = chunk.syntax().token_at_offset(offset).right_biased() else {
        return Vec::new();
    };
    token
        .parent_ancestors()
        .find_map(|node| {
            if let Some(field) = LuaTableField::cast(node.clone()) {
                return Some(field.get_comments());
            }
            LuaStat::cast(node).map(|stat| stat.get_comments())
        })
        .unwrap_or_default()
}

fn comments_of_syntax(
    model: &SemanticModel<'_>,
    file_id: FileId,
    syntax: LuaSyntaxId,
) -> Vec<LuaComment> {
    let Some(tree) = model.syntax_tree_of(file_id) else {
        return Vec::new();
    };
    let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
        return Vec::new();
    };
    match LuaAst::cast(node) {
        Some(LuaAst::LuaTableField(field)) => field.get_comments(),
        Some(ast) => LuaStat::cast(ast.syntax().clone())
            .map(|stat| stat.get_comments())
            .unwrap_or_default(),
        None => Vec::new(),
    }
}
