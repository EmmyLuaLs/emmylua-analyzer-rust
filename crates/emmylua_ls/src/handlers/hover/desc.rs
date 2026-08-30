//! # desc -- Extract comment descriptions for declarations / members / types (pure salsa + parser)
//!
//! - decl / types: plain-text description from the owner statement (or table field) comment block, plus extra tag rendering (e.g. `@see`);
//! - `@field` members: description text from the `@field` tag itself; runtime members: description from the owner statement comment block;
//! - parameter declarations: description text from the `@param` tag.

use emmylua_code_analysis::{SalsaSemanticModel, SemanticId, TypeDef};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaChunk, LuaComment, LuaCommentOwner, LuaDocDescriptionOwner, LuaDocTag,
    LuaDocTagField, LuaDocTagParam, LuaDocTagReturn, LuaDocTagReturnOverload, LuaStat,
    LuaTableField,
};

pub struct HoverDescription {
    /// Plain-text description.
    pub text: Option<String>,
    /// Rendered extra tag lines (markdown: `@*see* a.b.c`).
    pub tags: Vec<String>,
    /// Owner class hint line (`&nbsp;&nbsp;in class \`X\``, rendered before the separator).
    pub owner_line: Option<String>,
    /// Overload blocks (each one ` ```lua ... ``` `, rendered after the description/extra tags).
    pub overload_blocks: Vec<String>,
}

impl HoverDescription {
    fn empty() -> Self {
        Self {
            text: None,
            tags: Vec::new(),
            owner_line: None,
            overload_blocks: Vec::new(),
        }
    }

    #[allow(unused)]
    fn is_empty(&self) -> bool {
        self.text.is_none() && self.tags.is_empty()
    }
}

pub fn decl_description(model: &SalsaSemanticModel<'_>, decl: &SemanticId) -> HoverDescription {
    let Some(decls) = model.decls() else {
        return HoverDescription::empty();
    };
    let Some(decl_info) = decls.iter().find(|d| &d.id == decl) else {
        return HoverDescription::empty();
    };
    let decl_info = decl_info.clone();

    // Parameter declaration: `---@param name type desc` -> desc.
    if matches!(decl_info.kind, emmylua_code_analysis::DeclKind::Param) {
        if let Some(param_desc) = param_tag_description(model, &decl_info.name) {
            return HoverDescription {
                text: Some(param_desc),
                tags: Vec::new(),
                owner_line: None,
                overload_blocks: Vec::new(),
            };
        }
        return HoverDescription::empty();
    }

    // General declarations: comment block on the owner statement.
    let Some(comments) = comments_of_syntax(model, decl_info.owner_syntax) else {
        return HoverDescription::empty();
    };
    comments_description(&comments)
}

pub fn member_description(model: &SalsaSemanticModel<'_>, member: &SemanticId) -> HoverDescription {
    let Some((member_info, member_file_id)) = member_decl(model, member) else {
        return HoverDescription::empty();
    };

    // `@field` members: the tag's own description.
    if matches!(member_info.owner, SemanticId::TypeDef(_)) {
        if let Some(desc) = field_tag_description(
            model,
            member_file_id,
            &member_info.owner,
            &member_info.key,
            member_info.id.member_key_range(),
        ) {
            return HoverDescription {
                text: Some(desc),
                tags: Vec::new(),
                owner_line: None,
                overload_blocks: Vec::new(),
            };
        }
        return HoverDescription::empty();
    }

    // Runtime members: comment block on the owner statement in the declaring file.
    let Some(tree) = model.syntax_tree_of(member_file_id) else {
        return HoverDescription::empty();
    };
    let Some(range) = member_info.id.member_key_range() else {
        return HoverDescription::empty();
    };
    let root = tree.get_red_root();
    let Some(chunk) = LuaChunk::cast(root) else {
        return HoverDescription::empty();
    };
    let Some(token) = chunk.syntax().token_at_offset(range.start()).right_biased() else {
        return HoverDescription::empty();
    };
    let Some(owner_ast) = token.parent_ancestors().find_map(|node| {
        if let Some(stat) = LuaStat::cast(node.clone()) {
            return LuaAst::cast(stat.syntax().clone());
        }
        if let Some(table_field) = LuaTableField::cast(node.clone()) {
            return Some(LuaAst::LuaTableField(table_field));
        }
        None
    }) else {
        return HoverDescription::empty();
    };
    let comments = comments_of_ast(&owner_ast);
    if let Some(text) = inline_type_description(&comments) {
        return HoverDescription {
            text: Some(text),
            tags: Vec::new(),
            owner_line: None,
            overload_blocks: Vec::new(),
        };
    }
    comments_description(&comments)
}

/// Member declaration (cross-file: the Member identity carries its own file_id).
fn member_decl(
    model: &SalsaSemanticModel<'_>,
    member: &SemanticId,
) -> Option<(emmylua_code_analysis::Member, emmylua_code_analysis::FileId)> {
    let SemanticId::Member(member_key) = member else {
        return None;
    };
    let facts = model.file_facts_of(member_key.file_id)?;
    let member_info = facts.member_by_id(member)?.clone();
    Some((member_info, member_key.file_id))
}

pub fn type_def_description(model: &SalsaSemanticModel<'_>, def: &TypeDef) -> HoverDescription {
    let Some(tree) = model.syntax_tree_of(def.file_id) else {
        return HoverDescription::empty();
    };
    let Some(token) = tree
        .get_red_root()
        .token_at_offset(def.name_range.start())
        .right_biased()
    else {
        return HoverDescription::empty();
    };
    // Comment block for `---@class X`.
    let Some(comment) = token.parent_ancestors().find_map(LuaComment::cast) else {
        return HoverDescription::empty();
    };
    comments_description(&[comment])
}

/// Clean up multiline descriptions: trim each line, keep newlines.
fn normalize_description(text: &str) -> String {
    text.lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Clean up `@field` descriptions: strip the common `#` prefix.
fn clean_field_description(text: &str) -> String {
    normalize_description(text)
        .lines()
        .map(|line| line.trim_start_matches('#').trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Extract hover tag lines for `@param` / `@return` / `@return_overload` from a comment block.
fn clean_tag_description(desc: &str) -> String {
    desc.trim().trim_start_matches('@').trim().to_string()
}

pub(crate) fn signature_tags_from_comments(comments: &[LuaComment]) -> Vec<String> {
    let mut tags = Vec::new();
    for comment in comments {
        for tag in comment.get_doc_tags() {
            if let Some(param) = LuaDocTagParam::cast(tag.syntax().clone()) {
                let Some(name) = param
                    .get_name_token()
                    .map(|token| token.get_name_text().to_string())
                else {
                    continue;
                };
                let desc = param
                    .get_description()
                    .map(|desc| desc.get_description_text())
                    .unwrap_or_default();
                if !desc.is_empty() {
                    tags.push(format!(
                        "@*param* `{}` — {}",
                        name,
                        clean_tag_description(&desc)
                    ));
                }
            } else if let Some(return_tag) = LuaDocTagReturn::cast(tag.syntax().clone()) {
                let desc = return_tag
                    .get_description()
                    .map(|desc| desc.get_description_text())
                    .unwrap_or_default();
                for (ty, name) in return_tag.get_info_list() {
                    let type_text = ty.syntax().text().to_string().trim().to_string();
                    if let Some(name) = name {
                        if !desc.is_empty() {
                            tags.push(format!(
                                "@*return* `{}`  — {}",
                                name.get_name_text(),
                                clean_tag_description(&desc)
                            ));
                        }
                    } else if !desc.is_empty() {
                        tags.push(format!(
                            "@*return* `{}`  — {}",
                            type_text,
                            clean_tag_description(&desc)
                        ));
                    }
                }
            } else if let Some(overload) = LuaDocTagReturnOverload::cast(tag.syntax().clone()) {
                let types = overload
                    .get_types()
                    .map(|ty| ty.syntax().text().to_string().trim().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let desc = overload
                    .get_description()
                    .map(|desc| desc.get_description_text())
                    .unwrap_or_default();
                if desc.is_empty() {
                    tags.push(format!("@*return_overload* `{}`", types));
                } else {
                    tags.push(format!(
                        "@*return_overload* `{}` — {}",
                        types,
                        clean_tag_description(&desc)
                    ));
                }
            }
        }
    }
    tags
}

pub(crate) fn decl_signature_tags(
    model: &SalsaSemanticModel<'_>,
    decl: &SemanticId,
) -> Vec<String> {
    let Some(decls) = model.decls() else {
        return Vec::new();
    };
    let Some(decl_info) = decls.iter().find(|d| &d.id == decl) else {
        return Vec::new();
    };
    let Some(comments) = comments_of_syntax(model, decl_info.owner_syntax) else {
        return Vec::new();
    };
    signature_tags_from_comments(&comments)
}

pub(crate) fn member_signature_tags(
    model: &SalsaSemanticModel<'_>,
    member: &SemanticId,
) -> Vec<String> {
    let Some((member_info, member_file_id)) = member_decl(model, member) else {
        return Vec::new();
    };
    let Some(tree) = model.syntax_tree_of(member_file_id) else {
        return Vec::new();
    };
    let Some(range) = member_info.id.member_key_range() else {
        return Vec::new();
    };
    let root = tree.get_red_root();
    let Some(chunk) = LuaChunk::cast(root) else {
        return Vec::new();
    };
    let Some(token) = chunk.syntax().token_at_offset(range.start()).right_biased() else {
        return Vec::new();
    };
    let Some(owner_ast) = token.parent_ancestors().find_map(|node| {
        if let Some(stat) = LuaStat::cast(node.clone()) {
            return LuaAst::cast(stat.syntax().clone());
        }
        if let Some(table_field) = LuaTableField::cast(node.clone()) {
            return Some(LuaAst::LuaTableField(table_field));
        }
        None
    }) else {
        return Vec::new();
    };
    let comments = comments_of_ast(&owner_ast);
    signature_tags_from_comments(&comments)
}

/// Comment block -> description (plain text + extra tag lines).
fn comments_description(comments: &[LuaComment]) -> HoverDescription {
    let mut text = None;
    let mut tags = Vec::new();
    for comment in comments {
        if text.is_none() {
            text = comment_text(comment);
        }
        for tag in comment.get_doc_tags() {
            if let Some(rendered) = render_extra_tag(&tag) {
                tags.push(rendered);
            }
        }
    }
    HoverDescription {
        text,
        tags,
        owner_line: None,
        overload_blocks: Vec::new(),
    }
}

/// Comment text: doc description nodes take priority; comments with doc tags do not fall back to raw text; otherwise strip the comment prefix from raw text.
fn comment_text(comment: &LuaComment) -> Option<String> {
    if let Some(desc) = comment.get_description() {
        let text = desc.get_description_text();
        if !text.trim().is_empty() {
            return Some(normalize_description(&text));
        }
    }
    if comment.get_doc_tags().next().is_some() {
        return None;
    }
    let raw = comment.syntax().text().to_string();
    let trimmed = raw.trim();
    let without_prefix = trimmed
        .strip_prefix("---")
        .or_else(|| trimmed.strip_prefix("--"))
        .unwrap_or(trimmed)
        .trim();
    if without_prefix.is_empty() {
        None
    } else {
        Some(without_prefix.to_string())
    }
}

/// Description text for `@param name`.
fn param_tag_description(model: &SalsaSemanticModel<'_>, param_name: &str) -> Option<String> {
    let chunk = model.chunk()?;
    for comment in chunk.descendants::<LuaComment>() {
        for tag in comment.get_doc_tags() {
            if let Some(doc_param) = LuaDocTagParam::cast(tag.syntax().clone())
                && let Some(name_token) = doc_param.get_name_token()
                && name_token.get_name_text() == param_name
                && let Some(desc) = doc_param.get_description()
            {
                return Some(normalize_description(&desc.get_description_text()));
            }
        }
    }
    None
}

/// Description text for an inline table field `---@type T desc`.
fn inline_type_description(comments: &[LuaComment]) -> Option<String> {
    for comment in comments {
        for tag in comment.get_doc_tags() {
            if let LuaDocTag::Type(type_tag) = tag
                && let Some(desc) = type_tag.get_description()
            {
                let text = normalize_description(&desc.get_description_text());
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

/// Description text for `@field key` (owner type definition, cross-file).
fn field_tag_description(
    model: &SalsaSemanticModel<'_>,
    file_id: emmylua_code_analysis::FileId,
    owner: &SemanticId,
    key: &emmylua_code_analysis::LuaMemberKey,
    member_key_range: Option<rowan::TextRange>,
) -> Option<String> {
    let tree = model.syntax_tree_of(file_id)?;
    let root = tree.get_red_root();
    let chunk = LuaChunk::cast(root)?;
    let key_text = key.to_path();
    for comment in chunk.descendants::<LuaComment>() {
        for tag in comment.get_doc_tags() {
            if let Some(field) = LuaDocTagField::cast(tag.syntax().clone()) {
                let name = field
                    .get_field_key()
                    .map(|key| match &key {
                        emmylua_parser::LuaDocFieldKey::Name(t) => t.get_name_text().to_string(),
                        emmylua_parser::LuaDocFieldKey::String(t) => t.get_value(),
                        emmylua_parser::LuaDocFieldKey::Integer(t) => {
                            t.get_number_value().to_string()
                        }
                        emmylua_parser::LuaDocFieldKey::Type(_) => String::new(),
                    })
                    .unwrap_or_default();
                let matches_owner = model
                    .file_facts_of(file_id)
                    .and_then(|facts| facts.type_defs.iter().find(|def| def.id == *owner))
                    .is_some();
                let matches_range = member_key_range.is_none_or(|range| {
                    field
                        .get_field_key_range()
                        .is_some_and(|field_range| field_range == range)
                });
                if matches_owner
                    && matches_range
                    && name == key_text
                    && let Some(desc) = field.get_description()
                {
                    return Some(clean_field_description(&desc.get_description_text()));
                }
            }
        }
    }
    None
}

/// Render extra tags: `@see a.b.c` -> `@*see* a.b.c`; `@xyz content` -> `@*xyz* content`.
fn render_extra_tag(tag: &LuaDocTag) -> Option<String> {
    let text = tag.syntax().text().to_string();
    let trimmed = text.trim();
    let body = trimmed.strip_prefix('@').unwrap_or(trimmed);
    let (name, mut content) = match body.find(char::is_whitespace) {
        Some(index) => (&body[..index], body[index..].trim().to_string()),
        None => (body, String::new()),
    };
    if let LuaDocTag::Other(other) = tag
        && let Some(desc) = other.get_description()
    {
        let desc_text = desc.get_description_text();
        if !desc_text.trim().is_empty() {
            content = normalize_description(&desc_text);
        }
    }
    // Main tags such as @param/@field/@class are not rendered as extra tags.
    if matches!(
        name,
        "param"
            | "field"
            | "class"
            | "alias"
            | "enum"
            | "type"
            | "return"
            | "return_overload"
            | "generic"
            | "overload"
            | "attribute"
            | "module"
            | "diagnostic"
            | "deprecated"
            | "version"
            | "cast"
            | "source"
            | "schema"
            | "namespace"
            | "using"
            | "meta"
            | "nodiscard"
            | "readonly"
            | "operator"
            | "async"
            | "as"
            | "visibility"
            | "return_cast"
            | "language"
    ) {
        return None;
    }
    if content.is_empty() {
        Some(format!("@*{}*", name))
    } else {
        Some(format!("@*{}* {}", name, content))
    }
}

/// `LuaAst` -> comment block (comment-owner variant).
fn comments_of_ast(ast: &LuaAst) -> Vec<LuaComment> {
    match ast {
        LuaAst::LuaTableField(field) => field.get_comments(),
        LuaAst::LuaTableExpr(expr) => expr.get_comments(),
        LuaAst::LuaNameExpr(expr) => expr.get_comments(),
        _ => LuaStat::cast(ast.syntax().clone())
            .map(|stat| stat.get_comments())
            .unwrap_or_default(),
    }
}

fn comments_of_syntax(
    model: &SalsaSemanticModel<'_>,
    owner_syntax: Option<emmylua_parser::LuaSyntaxId>,
) -> Option<Vec<LuaComment>> {
    let chunk = model.chunk()?;
    let node = owner_syntax?.to_node_from_root(&chunk.syntax())?;
    let ast = LuaAst::cast(node)?;
    Some(comments_of_ast(&ast))
}
