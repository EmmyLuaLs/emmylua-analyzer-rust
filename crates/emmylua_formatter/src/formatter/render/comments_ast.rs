use super::FormatContext;
use crate::formatter::model::{FormatPlan, TokenSpacingExpected};
use crate::ir;
use crate::ir::DocIR;
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaComment, LuaDocDescriptionOwner, LuaDocTag, LuaSyntaxId,
    LuaSyntaxToken, LuaTokenKind,
};
use std::collections::HashMap;

pub(crate) fn render_comment_via_ast(
    ctx: &FormatContext,
    c: &LuaComment,
    plan: &FormatPlan,
) -> Vec<DocIR> {
    let mut lines = render_all_lines(ctx, c, plan);
    let mut tg = tag_line_indices(ctx, c, &lines);
    if ctx
        .config
        .should_align_emmy_doc_multiline_alias_descriptions()
        && let Some(ml) = multiline_continue_group(ctx, c, &lines)
    {
        tg.entry("alias_multiline_description".into())
            .or_default()
            .extend(ml);
    }
    let groups: Vec<Vec<usize>> = tg.into_values().filter(|g| g.len() > 1).collect();
    if !groups.is_empty() {
        lines = apply_alignment(plan, c, lines, groups);
    }
    join_lines(lines)
}

fn join_lines(lines: Vec<Vec<DocIR>>) -> Vec<DocIR> {
    let mut o = Vec::new();
    for (i, l) in lines.into_iter().enumerate() {
        if i > 0 {
            o.push(ir::hard_line());
        }
        o.extend(l);
    }
    o
}

fn render_all_lines(ctx: &FormatContext, c: &LuaComment, plan: &FormatPlan) -> Vec<Vec<DocIR>> {
    let mut lines: Vec<Vec<DocIR>> = Vec::new();
    let mut body: Vec<DocIR> = Vec::new();
    let mut prefix: Option<LuaSyntaxToken> = None;
    let mut prev: Option<LuaSyntaxToken> = None;
    let mut had_ws = false;
    let mut multi_ws: Option<String> = None;
    let mut gap: Option<String> = None;
    let mut live = false;
    let mut seen_prefix = false;
    let mut preserve_spacing = false;

    for el in c.syntax().descendants_with_tokens() {
        let Some(tok) = el.into_token() else {
            continue;
        };
        let kind = tok.kind().to_token();
        match kind {
            LuaTokenKind::TkNormalStart
            | LuaTokenKind::TkLongCommentStart
            | LuaTokenKind::TkDocLongStart
            | LuaTokenKind::TkDocStart
            | LuaTokenKind::TkDocContinue
            | LuaTokenKind::TkDocContinueOr => {
                if seen_prefix && prev.is_some() {
                    // Prefix-like token embedded in body — use raw source text.
                    if gap.is_none() {
                        if let Some(ref mw_text) = multi_ws {
                            body.push(ir::text(mw_text.as_str()));
                        } else {
                            for _ in 0..usize::from(had_ws) {
                                body.push(ir::space());
                            }
                        }
                    }
                    if let Some(g) = gap.take()
                        && !g.is_empty()
                    {
                        body.push(ir::text(&g));
                    }
                    body.push(ir::source_token(tok.clone()));
                    prev = Some(tok);
                    had_ws = false;
                    multi_ws = None;
                } else {
                    prefix = Some(tok);
                    prev = None;
                    had_ws = false;
                    multi_ws = None;
                    gap = None;
                    live = true;
                    seen_prefix = true;
                    preserve_spacing = false;
                }
            }
            LuaTokenKind::TkEndOfLine => {
                lines.push(render_line(
                    ctx,
                    plan,
                    prefix.as_ref(),
                    gap.take(),
                    std::mem::take(&mut body),
                ));
                prefix = None;
                prev = None;
                had_ws = false;
                multi_ws = None;
                gap = None;
                live = false;
                seen_prefix = false;
                preserve_spacing = false;
            }
            LuaTokenKind::TkWhitespace => {
                if !seen_prefix {
                    continue;
                } // indentation
                had_ws = true;
                let txt = tok.text();
                if txt.len() > 1 {
                    multi_ws = Some(txt.to_string());
                }
                if prev.is_none() && txt.len() > 1 {
                    gap = Some(txt.to_string());
                }
            }
            _ => {
                if gap.is_none() {
                    if let Some(ref mw_text) = multi_ws {
                        let is_desc = prefix
                            .as_ref()
                            .is_some_and(|p| p.kind().to_token() == LuaTokenKind::TkNormalStart);
                        if is_desc || preserve_spacing {
                            body.push(ir::text(mw_text.as_str()));
                        } else {
                            body.push(ir::space());
                        }
                    } else if preserve_spacing {
                        for _ in 0..usize::from(had_ws) {
                            body.push(ir::space());
                        }
                    } else {
                        let spaces = inter_token_spaces(plan, prev.as_ref(), &tok, had_ws);
                        for _ in 0..spaces {
                            body.push(ir::space());
                        }
                    }
                }
                if let Some(g) = gap.take()
                    && !g.is_empty()
                {
                    body.push(ir::text(&g));
                }
                body.push(format_body_token(plan, &tok));
                if !preserve_spacing && prev.is_none() {
                    let k = tok.kind().to_token();
                    if k == LuaTokenKind::TkTagAlias
                        && !ctx.config.should_align_emmy_doc_declaration_tags()
                    {
                        preserve_spacing = true;
                    }
                }
                prev = Some(tok);
                had_ws = false;
                multi_ws = None;
                live = true;
            }
        }
    }
    if live {
        lines.push(render_line(ctx, plan, prefix.as_ref(), gap, body));
    }
    lines
}

fn render_line(
    ctx: &FormatContext,
    plan: &FormatPlan,
    prefix: Option<&LuaSyntaxToken>,
    gap: Option<String>,
    mut body: Vec<DocIR>,
) -> Vec<DocIR> {
    let tok = match prefix {
        Some(t) => t,
        None => return body,
    };
    let kind = tok.kind().to_token();
    let mut docs = Vec::new();

    match kind {
        LuaTokenKind::TkLongCommentStart | LuaTokenKind::TkDocLongStart => {
            docs.push(ir::source_token(tok.clone()));
            docs.append(&mut body);
        }
        LuaTokenKind::TkNormalStart if count_dashes(tok.text()) >= 3 => {
            if body.first().is_some_and(|d| match d {
                DocIR::SourceToken(t) => t.text().starts_with('|'),
                DocIR::Text(t) => t.starts_with('|'),
                _ => false,
            }) {
                docs.push(ir::text("--- | "));

                if let Some(first) = body.first() {
                    let inner = match first {
                        DocIR::SourceToken(t) => t.text()[1..].to_string(),
                        DocIR::Text(t) if t.starts_with('|') => t[1..].to_string(),
                        _ => String::new(),
                    };
                    if !inner.is_empty() {
                        body[0] = ir::text(inner);
                    }
                }
                docs.append(&mut body);
            } else {
                let mut raw = tok.text().to_string();
                if ctx.config.emmy_doc.space_after_description_dash {
                    // Ensure space after dashes even when body has no leading ws.
                    if !raw.ends_with(' ') {
                        raw.push(' ');
                    }
                } else {
                    raw = raw.trim_end().to_string();
                }
                docs.push(ir::text(&raw));
                if let Some(g) = gap
                    && !g.is_empty()
                {
                    docs.push(ir::text(&g));
                }
                docs.append(&mut body);
            }
        }
        _ => {
            let mut raw = tok.text().to_string();

            if kind == LuaTokenKind::TkDocContinueOr {
                raw = normalize_continue_or_prefix(ctx, &raw);

                if !body_has_leading_whitespace(&body) && !body.is_empty() {
                    body.insert(0, ir::space());
                }
            }
            let body_has_ws = body_has_leading_whitespace(&body);
            if body_has_ws {
                docs.push(ir::text(&raw));
                if let Some(g) = gap
                    && !g.is_empty()
                {
                    docs.push(ir::text(&g));
                }
                docs.append(&mut body);
            } else {
                let replacement = plan.spacing.token_replace(LuaSyntaxId::from_token(tok));
                let formatted = replacement.map(|r| r.to_string()).unwrap_or_else(|| {
                    let dashes = count_dashes(&raw);
                    match dashes {
                        2 if ctx.config.comments.space_after_comment_dash => "-- ".into(),
                        3 if ctx.config.emmy_doc.space_after_description_dash => "--- ".into(),
                        _ => raw.clone(),
                    }
                });
                docs.push(ir::text(&formatted));
                if let Some(g) = gap
                    && !g.is_empty()
                {
                    docs.push(ir::text(&g));
                }
                docs.append(&mut body);
            }
        }
    }
    docs
}

fn body_has_leading_whitespace(body: &[DocIR]) -> bool {
    match body.first() {
        Some(DocIR::Space) => true,
        Some(DocIR::Text(t)) => !t.is_empty() && t.chars().all(|c| c == ' ' || c == '\t'),
        _ => false,
    }
}

fn tag_line_indices(
    ctx: &FormatContext,
    c: &LuaComment,
    lines: &[Vec<DocIR>],
) -> HashMap<String, Vec<usize>> {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    let mut li = 0;
    for el in c.syntax().descendants_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) if t.kind().to_token() == LuaTokenKind::TkEndOfLine => {
                li += 1
            }
            rowan::NodeOrToken::Node(n) => {
                if let Some(tag) = LuaDocTag::cast(n)
                    && let Some(key) = align_key(ctx, &tag)
                    && li < lines.len()
                {
                    groups.entry(key).or_default().push(li);
                }
            }
            _ => {}
        }
    }
    groups
}

fn multiline_continue_group(
    _ctx: &FormatContext,
    c: &LuaComment,
    lines: &[Vec<DocIR>],
) -> Option<Vec<usize>> {
    let mut indices = Vec::new();
    let mut after_alias = false;
    let mut li = 0;
    for el in c.syntax().descendants_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) if t.kind().to_token() == LuaTokenKind::TkEndOfLine => {
                li += 1
            }
            rowan::NodeOrToken::Node(n) => {
                if let Some(tag) = emmylua_parser::LuaDocTag::cast(n) {
                    after_alias = matches!(tag, LuaDocTag::Alias(_));
                }
            }
            rowan::NodeOrToken::Token(t)
                if t.kind().to_token() == LuaTokenKind::TkDocContinueOr
                    && after_alias
                    && li < lines.len() =>
            {
                indices.push(li);
            }
            _ => {}
        }
    }
    if indices.is_empty() {
        None
    } else {
        Some(indices)
    }
}

fn align_key(ctx: &FormatContext, tag: &LuaDocTag) -> Option<String> {
    let k = match tag {
        LuaDocTag::Class(_) if ctx.config.should_align_emmy_doc_declaration_tags() => "class",
        LuaDocTag::Alias(_) if ctx.config.should_align_emmy_doc_declaration_tags() => "alias",
        LuaDocTag::Field(_) if ctx.config.should_align_emmy_doc_declaration_tags() => "field",
        LuaDocTag::Generic(_) if ctx.config.should_align_emmy_doc_declaration_tags() => "generic",
        LuaDocTag::Param(_) if ctx.config.should_align_emmy_doc_reference_tags() => "param",
        LuaDocTag::Return(_) if ctx.config.should_align_emmy_doc_reference_tags() => "return",
        _ => return None,
    };
    Some(k.into())
}

fn apply_alignment(
    plan: &FormatPlan,
    c: &LuaComment,
    lines: Vec<Vec<DocIR>>,
    groups: Vec<Vec<usize>>,
) -> Vec<Vec<DocIR>> {
    let mut result = lines;
    for group in &groups {
        if group.len() <= 1 {
            continue;
        }
        let mut all_cols: Vec<Vec<String>> = Vec::new();
        for &li in group {
            if let Some(tag) = find_tag_at_line(c, li) {
                let (cols, _) = extract_columns(plan, &tag, c);
                all_cols.push(cols);
            } else {
                // Non-tag line (multiline continue)
                let cols = continue_line_columns(&result[li]);
                all_cols.push(cols);
            }
        }
        if all_cols.is_empty() {
            continue;
        }
        let ncols = all_cols.iter().map(|c| c.len()).max().unwrap_or(0);
        let mut widths = vec![0usize; ncols];
        for cols in &all_cols {
            for (i, c) in cols.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(c.len());
                }
            }
        }
        for (gi, &li) in group.iter().enumerate() {
            if gi >= all_cols.len() {
                continue;
            }
            let cols = &all_cols[gi];
            // For continue-or lines, prefix already has trailing space.
            let is_continue = result[li].first().is_some_and(|d| match d {
                DocIR::Text(t) => {
                    let s = t.trim();
                    s.ends_with('|') || s.ends_with("|+") || s.ends_with("|>")
                }
                _ => false,
            });
            let mut new_line = result[li]
                .first()
                .map(|d| vec![d.clone()])
                .unwrap_or_default();
            for (i, col) in cols.iter().enumerate() {
                if i > 0 || is_continue {
                    new_line.push(ir::space());
                }
                new_line.push(ir::text(col));
                // Pad non-last columns only.
                if i < cols.len() - 1 && i < widths.len() {
                    for _ in 0..widths[i].saturating_sub(col.len()) {
                        new_line.push(ir::space());
                    }
                }
            }
            result[li] = new_line;
        }
    }
    result
}

fn find_tag_at_line(c: &LuaComment, target: usize) -> Option<LuaDocTag> {
    let mut li = 0;
    for el in c.syntax().descendants_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) if t.kind().to_token() == LuaTokenKind::TkEndOfLine => {
                li += 1
            }
            rowan::NodeOrToken::Node(n) if li == target => {
                if let Some(tag) = LuaDocTag::cast(n) {
                    return Some(tag);
                }
            }
            _ => {}
        }
    }
    None
}

fn continue_line_columns(line: &[DocIR]) -> Vec<String> {
    let text: String = line
        .iter()
        .filter_map(|d| match d {
            DocIR::Text(t) => Some(t.as_str()),
            DocIR::Space => Some(" "),
            DocIR::SourceToken(t) => Some(t.text()),
            _ => None,
        })
        .collect();
    // Find start of body: skip past "---", optional space, "|", optional "+"/">" marker, optional space.
    let trimmed = text.trim_start();
    let body_start = trimmed
        .find(|c: char| !matches!(c, '-' | ' ' | '|' | '+' | '>'))
        .unwrap_or(trimmed.len());
    let body = trimmed[body_start..].trim();
    if let Some(hash_idx) = body.find(" #") {
        let value = body[..hash_idx].trim().to_string();
        let desc = body[hash_idx + 1..].to_string();
        vec![value, desc]
    } else {
        vec![body.to_string()]
    }
}

fn extract_columns(
    plan: &FormatPlan,
    tag: &LuaDocTag,
    c: &LuaComment,
) -> (Vec<String>, Option<String>) {
    let desc = tag_description(tag);
    match tag {
        LuaDocTag::Param(t) => {
            let name = t
                .get_name_token()
                .map(|n| n.get_name_text().to_string())
                .unwrap_or_default();
            let nullable = if t.is_nullable() { "?" } else { "" };
            let ty = t
                .get_type()
                .map(|ty| format_node_tokens(plan, ty.syntax()))
                .unwrap_or_default();
            let mut cols = vec!["param".into(), format!("{name}{nullable}"), ty];
            if !desc.is_empty() {
                cols.push(desc);
            }
            (cols, Some("param".into()))
        }
        LuaDocTag::Return(t) => {
            let pairs: Vec<String> = t
                .get_info_list()
                .into_iter()
                .map(|(ty, name)| {
                    let ts = format_node_tokens(plan, ty.syntax());
                    match name {
                        Some(n) => format!("{} {}", ts, n.get_name_text()),
                        None => ts,
                    }
                })
                .collect();
            let types = if pairs.is_empty() {
                t.get_types()
                    .map(|ty| format_node_tokens(plan, ty.syntax()))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                pairs.join(", ")
            };
            let mut cols = vec!["return".into(), types];
            if !desc.is_empty() {
                cols.push(desc);
            }
            (cols, Some("return".into()))
        }
        LuaDocTag::Class(t) => {
            let name = t
                .get_name_token()
                .map(|n| n.get_name_text().to_string())
                .unwrap_or_default();
            let mut class_name = name.clone();
            if let Some(g) = t.get_generic_decl() {
                class_name.push_str(&format_node_tokens(plan, g.syntax()));
            }
            let mut cols = vec!["class".into(), class_name];
            if let Some(s) = t.get_supers() {
                cols.push(format!(": {}", format_node_tokens(plan, s.syntax())));
            }
            if !desc.is_empty() {
                cols.push(desc);
            }
            (cols, Some("class".into()))
        }
        LuaDocTag::Field(f) => {
            let vis = f
                .get_visibility_token()
                .map(|v| v.get_text().to_string())
                .unwrap_or_default();
            let key = f
                .get_field_key()
                .map(|k| match k {
                    emmylua_parser::LuaDocFieldKey::Name(n) => n.get_name_text().to_string(),
                    emmylua_parser::LuaDocFieldKey::String(s) => format!("[\"{}\"]", s.get_value()),
                    emmylua_parser::LuaDocFieldKey::Integer(i) => {
                        format!("[{}]", i.syntax().text())
                    }
                    emmylua_parser::LuaDocFieldKey::Type(ty) => {
                        format!("[{}]", format_node_tokens(plan, ty.syntax()))
                    }
                })
                .unwrap_or_default();
            let has_q = f.is_nullable();
            let name_col = if vis.is_empty() {
                if has_q && f.syntax().text().to_string().contains(&format!("{key}?")) {
                    format!("{key}?")
                } else {
                    key.clone()
                }
            } else {
                if has_q && f.syntax().text().to_string().contains(&format!("{key}?")) {
                    format!("{vis} {key}?")
                } else {
                    format!("{vis} {key}")
                }
            };
            // Use full node text to get body including parens on function types
            let formatted = format_node_tokens(plan, f.syntax());
            let body = formatted.strip_prefix("field").unwrap_or(&formatted).trim();
            // Strip visibility + key(?nullable) from body to get type (+ desc)
            let body_norm = body.trim_start();
            let rest = if let Some(r) = body_norm.strip_prefix(&name_col) {
                r.trim().to_string()
            } else {
                body_norm.to_string()
            };
            let desc = tag_description(tag);
            let type_str = if desc.is_empty() {
                rest
            } else if let Some(stripped) = rest.strip_suffix(&desc) {
                stripped.trim().to_string()
            } else {
                rest
            };
            let mut cols = vec!["field".into(), name_col, type_str];
            if !desc.is_empty() {
                cols.push(desc);
            }
            (cols, Some("field".into()))
        }
        LuaDocTag::Alias(t) => {
            let name = t
                .get_name_token()
                .map(|n| n.get_name_text().to_string())
                .unwrap_or_default();
            let type_str = t
                .get_type()
                .map(|ty| ty.syntax().text().to_string())
                .unwrap_or_default();
            let name_type = if type_str.is_empty() {
                name.clone()
            } else {
                format!("{} {}", name, type_str)
            };
            let cd = comment_desc_for_tag(c, tag);
            let mut cols = vec!["alias".into(), name_type];
            if !cd.is_empty() {
                cols.push(cd);
            }
            (cols, Some("alias".into()))
        }
        LuaDocTag::Generic(t) => {
            let raw = t.syntax().text().to_string();
            let body = raw
                .strip_prefix("generic")
                .unwrap_or(&raw)
                .trim()
                .to_string();
            let cd = comment_desc_for_tag(c, tag);
            let full_body = if cd.is_empty() {
                body.clone()
            } else {
                format!("{} {}", body, cd)
            };
            let tokens: Vec<&str> = full_body.split_whitespace().collect();
            let mut cols = vec!["generic".into()];
            match tokens.len() {
                0 => {}
                1 => {
                    cols.push(tokens[0].to_string());
                }
                2 => {
                    cols.push(tokens[0].to_string());
                    cols.push(tokens[1].to_string());
                }
                n => {
                    cols.push(tokens[..n - 2].join(" "));
                    cols.push(tokens[n - 2..].join(" "));
                }
            }
            (cols, Some("generic".into()))
        }
        _ => (vec![], None),
    }
}

fn tag_description(tag: &LuaDocTag) -> String {
    let d: Option<emmylua_parser::LuaDocDescription> = match tag {
        LuaDocTag::Class(t) => t.get_description(),
        LuaDocTag::Param(t) => t.get_description(),
        LuaDocTag::Return(t) => t.get_description(),
        LuaDocTag::Field(t) => t.get_description(),
        _ => return String::new(),
    };
    d.map(|x| {
        let full = x.syntax().text().to_string();
        // Only take the first line of the description (inline part)
        full.lines().next().unwrap_or(&full).trim().to_string()
    })
    .unwrap_or_default()
}

/// Find a DocDescription that follows the given tag in the Comment's children.
fn comment_desc_for_tag(c: &LuaComment, tag: &LuaDocTag) -> String {
    let mut found_tag = false;
    for child in c.syntax().children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(n) => {
                if n == *tag.syntax() {
                    found_tag = true;
                    continue;
                }
                if found_tag && let Some(desc) = emmylua_parser::LuaDocDescription::cast(n) {
                    return desc.syntax().text().to_string().trim().to_string();
                }
            }
            rowan::NodeOrToken::Token(t)
                if t.kind().to_token() == LuaTokenKind::TkEndOfLine && found_tag =>
            {
                return String::new();
            } // desc must be on same line
            _ => {}
        }
    }
    String::new()
}

fn format_node_tokens(plan: &FormatPlan, node: &emmylua_parser::LuaSyntaxNode) -> String {
    let mut result = String::new();
    let mut prev: Option<LuaSyntaxToken> = None;
    let mut had_ws = false;
    for el in node.descendants_with_tokens() {
        let Some(tok) = el.into_token() else {
            continue;
        };
        match tok.kind().to_token() {
            LuaTokenKind::TkWhitespace => {
                had_ws = true;
            }
            LuaTokenKind::TkEndOfLine => {}
            _ => {
                let spaces = inter_token_spaces(plan, prev.as_ref(), &tok, had_ws);
                for _ in 0..spaces {
                    result.push(' ');
                }
                let id = LuaSyntaxId::from_token(&tok);
                if let Some(replacement) = plan.spacing.token_replace(id) {
                    result.push_str(replacement);
                } else {
                    result.push_str(tok.text());
                }
                prev = Some(tok);
                had_ws = false;
            }
        }
    }
    result.trim().to_string()
}

fn inter_token_spaces(
    plan: &FormatPlan,
    prev: Option<&LuaSyntaxToken>,
    cur: &LuaSyntaxToken,
    had_source_ws: bool,
) -> usize {
    if had_source_ws && prev.is_some_and(|t| is_tag_keyword(t.kind().to_token())) {
        return 1;
    }
    let cur_id = LuaSyntaxId::from_token(cur);
    if let Some(e) = plan.spacing.left_expected(cur_id) {
        return resolve_spacing(e, had_source_ws);
    }
    if let Some(p) = prev
        && let Some(e) = plan.spacing.right_expected(LuaSyntaxId::from_token(p))
    {
        return resolve_spacing(e, had_source_ws);
    }
    usize::from(had_source_ws)
}

fn is_tag_keyword(k: LuaTokenKind) -> bool {
    matches!(
        k,
        LuaTokenKind::TkTagParam
            | LuaTokenKind::TkTagReturn
            | LuaTokenKind::TkTagClass
            | LuaTokenKind::TkTagField
            | LuaTokenKind::TkTagType
            | LuaTokenKind::TkTagAlias
            | LuaTokenKind::TkTagOverload
            | LuaTokenKind::TkTagGeneric
            | LuaTokenKind::TkDocVisibility
            | LuaTokenKind::TkTagVisibility
    )
}

fn resolve_spacing(e: &TokenSpacingExpected, had_source_ws: bool) -> usize {
    match e {
        TokenSpacingExpected::Space(n) => *n,
        TokenSpacingExpected::MaxSpace(_) => {
            if had_source_ws {
                1
            } else {
                0
            }
        }
    }
}

fn format_body_token(plan: &FormatPlan, tok: &LuaSyntaxToken) -> DocIR {
    let id = LuaSyntaxId::from_token(tok);
    if let Some(replacement) = plan.spacing.token_replace(id) {
        ir::text(replacement)
    } else {
        ir::source_token(tok.clone())
    }
}

fn count_dashes(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b'-').count()
}

fn normalize_continue_or_prefix(ctx: &FormatContext, raw: &str) -> String {
    if !raw.starts_with("---") {
        return raw.to_string();
    }
    let suffix = raw[3..].trim_start();
    let marker = suffix.trim();
    if marker == "|" {
        if ctx.config.emmy_doc.space_after_description_dash {
            format!("--- {marker}")
        } else {
            format!("---{marker}")
        }
    } else {
        if ctx.config.emmy_doc.space_after_description_dash {
            format!("--- {suffix}")
        } else {
            format!("---{suffix}")
        }
    }
}
