use crate::diagnostics::Diagnostics;
use crate::doc_text::{
    comment_payload, doc_tag_payload, is_doc_continue_or_line, is_doc_tag_line,
    parse_union_item_value_from_line_trim,
};
use crate::keys::{
    locale_key_desc, locale_key_field, locale_key_item, locale_key_param, locale_key_return,
    locale_key_return_item, map_symbol_for_locale_key,
};
use crate::model::{AnalyzedDocFile, EntrySelector, ExtractedComment, ExtractedEntry, SourceSpan};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaChunk, LuaComment, LuaDocDescriptionOwner,
    LuaDocMultiLineUnionType, LuaDocTag, LuaDocType, LuaExpr, LuaIndexExpr, LuaLiteralToken,
    LuaParser, LuaVarExpr, ParserConfig,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
struct RootContext {
    span: SourceSpan,
    symbol: String,
    base: String,
    version_suffix: Option<String>,
}

type LocaleBaseMap = HashMap<(SourceSpan, String), String>;

pub fn analyze_lua_doc_file(
    file_label: &str,
    content: &str,
    include_empty: bool,
    diagnostics: &mut Diagnostics,
) -> AnalyzedDocFile {
    let tree = LuaParser::parse(content, ParserConfig::default());
    let chunk = tree.get_chunk_node();
    let module_map = build_module_table_to_class_map(&chunk);

    let mut comments: Vec<LuaComment> = chunk.descendants::<LuaComment>().collect();
    comments.sort_by_key(|c| c.syntax().text_range().start());

    #[derive(Debug, Clone)]
    struct CommentRecord {
        comment: LuaComment,
        span: SourceSpan,
        raw: String,
    }

    // 有些 std 文档会把 `@version` 单独放在上一条注释里（紧挨着真正的 doc block）。
    // 这种 `@version` 注释通常没有 owner，因此当它与下一条“有 owner 的注释”相邻且中间只有空白时，
    // 我们把 version 后缀透传给下一条注释。
    let mut pending_version: Option<(String, usize)> = None; // (suffix, end_offset)

    let mut records: Vec<CommentRecord> = Vec::with_capacity(comments.len());
    let mut roots: Vec<RootContext> = Vec::new();

    for comment in comments {
        let range = comment.syntax().text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        let span = SourceSpan { start, end };

        let raw_slice = content.get(start..end).unwrap_or("");
        let raw_comment = raw_slice.to_string();

        let direct_version = extract_version_suffix(&comment, &raw_comment);
        let has_owner = comment.get_owner().is_some();

        let mut effective_version = direct_version.clone();
        if effective_version.is_none()
            && let Some((pending, pending_end)) = pending_version.as_ref()
            && has_owner
            && is_whitespace_between(content, *pending_end, start)
        {
            effective_version = Some(pending.clone());
        }

        for symbol in root_symbols_for_comment(&comment, &raw_comment) {
            let base = map_symbol_for_locale_key(&symbol, &module_map);
            roots.push(RootContext {
                span,
                symbol,
                base,
                version_suffix: effective_version.clone(),
            });
        }

        records.push(CommentRecord {
            comment,
            span,
            raw: raw_comment,
        });

        if has_owner {
            pending_version = None;
        } else if let Some(v) = direct_version {
            pending_version = Some((v, end));
        }
    }

    let locale_base_map = build_locale_base_map(&roots);

    let mut out_comments: Vec<ExtractedComment> = Vec::new();
    let mut out_entries: Vec<ExtractedEntry> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for r in records {
        let mut comment_entries: Vec<ExtractedEntry> = Vec::new();
        extract_from_comment(
            file_label,
            &r.comment,
            &r.raw,
            include_empty,
            r.span,
            &module_map,
            &locale_base_map,
            &mut comment_entries,
            &mut seen,
            diagnostics,
        );

        if !comment_entries.is_empty() {
            out_entries.extend(comment_entries.iter().cloned());
            out_comments.push(ExtractedComment {
                span: r.span,
                raw: r.raw,
                entries: comment_entries,
            });
        }
    }

    AnalyzedDocFile {
        comments: out_comments,
        entries: out_entries,
    }
}

fn build_locale_base_map(roots: &[RootContext]) -> LocaleBaseMap {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, r) in roots.iter().enumerate() {
        groups.entry(r.base.clone()).or_default().push(i);
    }

    let mut map: LocaleBaseMap = HashMap::new();
    for (base, idxs) in groups {
        if idxs.len() <= 1 {
            if let Some(i) = idxs.first() {
                let r = &roots[*i];
                map.insert((r.span, r.symbol.clone()), base.clone());
            }
            continue;
        }

        let mut used_suffix: HashMap<String, usize> = HashMap::new();
        let mut no_version_seq: usize = 0;
        for i in idxs {
            let r = &roots[i];
            let mut suffix = if let Some(v) = &r.version_suffix {
                v.clone()
            } else {
                no_version_seq += 1;
                format!("@@{no_version_seq}")
            };

            let c = used_suffix.entry(suffix.clone()).or_insert(0);
            *c += 1;
            if *c > 1 {
                suffix.push_str(&format!("@@{c}"));
            }

            map.insert((r.span, r.symbol.clone()), format!("{base}{suffix}"));
        }
    }

    map
}

#[allow(clippy::too_many_arguments)]
fn extract_from_comment(
    file_label: &str,
    comment: &LuaComment,
    raw_comment: &str,
    include_empty: bool,
    comment_span: SourceSpan,
    module_map: &HashMap<String, String>,
    locale_base_map: &LocaleBaseMap,
    out: &mut Vec<ExtractedEntry>,
    seen: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    let owner_symbol = comment.get_owner().and_then(owner_symbol_from_ast);
    let tags: Vec<LuaDocTag> = comment.get_doc_tags().collect();

    let mut class_name: Option<String> = None;
    let mut alias_name: Option<String> = None;
    for tag in &tags {
        match tag {
            LuaDocTag::Class(class_tag) => {
                let text = class_tag.syntax().text().to_string();
                class_name = class_name.or_else(|| parse_tag_primary_name(&text));
            }
            LuaDocTag::Alias(alias_tag) => {
                let text = alias_tag.syntax().text().to_string();
                alias_name = alias_name.or_else(|| parse_tag_primary_name(&text));
            }
            _ => {}
        }
    }

    // 优先从原始源码行提取（支持 `std.readmode` 这类带点的名字）。
    alias_name = alias_name.or_else(|| extract_tag_name_from_raw(raw_comment, "alias"));
    class_name = class_name.or_else(|| extract_tag_name_from_raw(raw_comment, "class"));

    // 1) owner/函数文档：desc/param/return（以及 return 多行 union item）
    if let Some(symbol) = owner_symbol.as_deref() {
        let base = map_symbol_for_locale_key(symbol, module_map);
        let locale_base = locale_base_map
            .get(&(comment_span, symbol.to_string()))
            .cloned()
            .unwrap_or_else(|| base.clone());
        let desc_raw = comment
            .get_description()
            .map(|d| d.get_description_text())
            .or_else(|| extract_owner_description_fallback(raw_comment))
            .unwrap_or_default();
        let desc_text = preprocess_description(&desc_raw);
        push_entry(
            file_label,
            out,
            seen,
            ExtractedEntry {
                locale_key: locale_key_desc(&locale_base),
                selector: EntrySelector::Desc,
                comment_span,
                raw: desc_raw,
                value: desc_text,
            },
            include_empty,
            diagnostics,
        );

        let raw_returns = return_sections_from_raw(raw_comment);
        let mut raw_returns_by_line: HashMap<usize, ReturnSection> = raw_returns
            .iter()
            .cloned()
            .map(|section| (section.line_start, section))
            .collect();
        let ast_return_descriptions =
            return_descriptions_by_raw_line(&tags, comment_span, &raw_returns);
        let mut return_index: usize = 0;
        for tag in &tags {
            match tag {
                LuaDocTag::Param(param) => {
                    let Some(name_token) = param.get_name_token() else {
                        continue;
                    };
                    let name = name_token.get_name_text().to_string();
                    let raw_desc = param
                        .get_description()
                        .map(|d| d.get_description_text())
                        .unwrap_or_default();
                    let text = preprocess_description(&raw_desc);
                    push_entry(
                        file_label,
                        out,
                        seen,
                        ExtractedEntry {
                            locale_key: locale_key_param(&locale_base, &name),
                            selector: EntrySelector::Param { name: name.clone() },
                            comment_span,
                            raw: raw_desc,
                            value: text,
                        },
                        include_empty,
                        diagnostics,
                    );
                }
                LuaDocTag::Return(ret) => {
                    let line_start = return_tag_line_start(ret, comment_span, &raw_returns);
                    let section = line_start.and_then(|line| raw_returns_by_line.remove(&line));
                    let raw_desc = line_start
                        .and_then(|line| ast_return_descriptions.get(&line).cloned())
                        .or_else(|| ret.get_description().map(|d| d.get_description_text()))
                        .unwrap_or_default();
                    return_index += 1;
                    push_return_entries(
                        file_label,
                        &locale_base,
                        return_index,
                        raw_desc,
                        section.map(|section| section.items).unwrap_or_default(),
                        comment_span,
                        include_empty,
                        out,
                        seen,
                        diagnostics,
                    );
                }
                _ => {}
            }
        }

        let mut remaining_returns: Vec<ReturnSection> = raw_returns_by_line.into_values().collect();
        remaining_returns.sort_by_key(|section| section.line_start);
        for section in remaining_returns {
            return_index += 1;
            push_return_entries(
                file_label,
                &locale_base,
                return_index,
                ast_return_descriptions
                    .get(&section.line_start)
                    .cloned()
                    .unwrap_or_default(),
                section.items,
                comment_span,
                include_empty,
                out,
                seen,
                diagnostics,
            );
        }

        return;
    }

    // 2) class/table 文档：desc/field
    if let Some(class_symbol) = class_name.as_deref() {
        let base = map_symbol_for_locale_key(class_symbol, module_map);
        let locale_base = locale_base_map
            .get(&(comment_span, class_symbol.to_string()))
            .cloned()
            .unwrap_or_else(|| base.clone());
        let desc_raw = comment
            .get_description()
            .map(|d| d.get_description_text())
            .or_else(|| extract_owner_description_fallback(raw_comment))
            .unwrap_or_default();
        let text = preprocess_description(&desc_raw);
        push_entry(
            file_label,
            out,
            seen,
            ExtractedEntry {
                locale_key: locale_key_desc(&locale_base),
                selector: EntrySelector::Desc,
                comment_span,
                raw: desc_raw,
                value: text,
            },
            include_empty,
            diagnostics,
        );

        for tag in &tags {
            if let LuaDocTag::Field(field) = tag {
                let field_key = field.get_field_key();
                let Some(field_name) = field_key.and_then(format_doc_field_key) else {
                    continue;
                };
                let raw_desc = field
                    .get_description()
                    .map(|d| d.get_description_text())
                    .unwrap_or_default();
                let text = preprocess_description(&raw_desc);
                push_entry(
                    file_label,
                    out,
                    seen,
                    ExtractedEntry {
                        locale_key: locale_key_field(&locale_base, &field_name),
                        selector: EntrySelector::Field {
                            name: field_name.clone(),
                        },
                        comment_span,
                        raw: raw_desc,
                        value: text,
                    },
                    include_empty,
                    diagnostics,
                );
            }
        }
    }

    // 3) alias：desc + 多行 union 枚举项（item.<value>）
    if let Some(alias_symbol) = alias_name.as_deref() {
        let base = map_symbol_for_locale_key(alias_symbol, module_map);
        let locale_base = locale_base_map
            .get(&(comment_span, alias_symbol.to_string()))
            .cloned()
            .unwrap_or_else(|| base.clone());
        let desc_raw = comment
            .get_description()
            .map(|d| d.get_description_text())
            .or_else(|| extract_owner_description_fallback(raw_comment))
            .unwrap_or_default();
        let text = preprocess_description(&desc_raw);
        push_entry(
            file_label,
            out,
            seen,
            ExtractedEntry {
                locale_key: locale_key_desc(&locale_base),
                selector: EntrySelector::Desc,
                comment_span,
                raw: desc_raw,
                value: text,
            },
            include_empty,
            diagnostics,
        );

        if let Some(union) = comment.descendants::<LuaDocMultiLineUnionType>().next() {
            for field in union.get_fields() {
                let Some(field_type) = field.get_type() else {
                    continue;
                };
                let Some(value) = literal_value_from_doc_type(&field_type) else {
                    continue;
                };
                let raw_desc = field
                    .get_description()
                    .map(|d| d.get_description_text())
                    .unwrap_or_default();
                let text = preprocess_description(&raw_desc);
                push_entry(
                    file_label,
                    out,
                    seen,
                    ExtractedEntry {
                        locale_key: locale_key_item(&locale_base, &value),
                        selector: EntrySelector::Item {
                            value: value.clone(),
                        },
                        comment_span,
                        raw: raw_desc,
                        value: text,
                    },
                    include_empty,
                    diagnostics,
                );
            }
        }
    }
}

fn push_entry(
    file_label: &str,
    out: &mut Vec<ExtractedEntry>,
    seen: &mut HashSet<String>,
    entry: ExtractedEntry,
    include_empty: bool,
    diagnostics: &mut Diagnostics,
) {
    if entry.value.trim().is_empty() && !include_empty {
        return;
    }
    if !seen.insert(entry.locale_key.clone()) {
        diagnostics.duplicate_locale_key(file_label, &entry.locale_key);
        return;
    }
    out.push(entry);
}

#[allow(clippy::too_many_arguments)]
fn push_return_entries(
    file_label: &str,
    locale_base: &str,
    return_index: usize,
    raw_desc: String,
    items: Vec<(String, String)>,
    comment_span: SourceSpan,
    include_empty: bool,
    out: &mut Vec<ExtractedEntry>,
    seen: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    let ident = return_index.to_string();
    let text = preprocess_description(&raw_desc);
    push_entry(
        file_label,
        out,
        seen,
        ExtractedEntry {
            locale_key: locale_key_return(locale_base, &ident),
            selector: EntrySelector::Return {
                index: return_index,
            },
            comment_span,
            raw: raw_desc,
            value: text,
        },
        include_empty,
        diagnostics,
    );

    for (value, raw_item_desc) in items {
        let text = preprocess_description(&raw_item_desc);
        push_entry(
            file_label,
            out,
            seen,
            ExtractedEntry {
                locale_key: locale_key_return_item(locale_base, &ident, &value),
                selector: EntrySelector::ReturnItem {
                    index: return_index,
                    value: value.clone(),
                },
                comment_span,
                raw: raw_item_desc,
                value: text,
            },
            include_empty,
            diagnostics,
        );
    }
}

#[derive(Debug, Clone)]
struct ReturnSection {
    line_start: usize,
    line_end: usize,
    items: Vec<(String, String)>,
}

fn return_descriptions_by_raw_line(
    tags: &[LuaDocTag],
    comment_span: SourceSpan,
    sections: &[ReturnSection],
) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    for tag in tags {
        let LuaDocTag::Return(ret) = tag else {
            continue;
        };
        let range = ret.syntax().text_range();
        let abs_start: usize = range.start().into();
        let rel_start = abs_start.saturating_sub(comment_span.start);
        let Some(section) = sections
            .iter()
            .find(|section| rel_start >= section.line_start && rel_start <= section.line_end)
        else {
            continue;
        };
        let raw_desc = ret
            .get_description()
            .map(|d| d.get_description_text())
            .unwrap_or_default();
        out.insert(section.line_start, raw_desc);
    }
    out
}

fn return_tag_line_start(
    ret: &emmylua_parser::LuaDocTagReturn,
    comment_span: SourceSpan,
    sections: &[ReturnSection],
) -> Option<usize> {
    let range = ret.syntax().text_range();
    let abs_start: usize = range.start().into();
    let rel_start = abs_start.saturating_sub(comment_span.start);
    sections
        .iter()
        .find(|section| rel_start >= section.line_start && rel_start <= section.line_end)
        .map(|section| section.line_start)
}

fn build_module_table_to_class_map(chunk: &LuaChunk) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for comment in chunk.descendants::<LuaComment>() {
        let Some(owner_ast) = comment.get_owner() else {
            continue;
        };

        // 仅对“模块表/全局对象”做映射（例如 `io = {}` -> `io -> iolib`）。
        // 对 `local x` 这类局部变量的 `---@class ...` 不做映射。
        if matches!(
            owner_ast,
            LuaAst::LuaLocalStat(_) | LuaAst::LuaLocalFuncStat(_)
        ) {
            continue;
        }

        let Some(owner) = owner_symbol_from_ast(owner_ast) else {
            continue;
        };
        for tag in comment.get_doc_tags() {
            if let LuaDocTag::Class(class_tag) = tag {
                let Some(class_name) = class_tag
                    .get_name_token()
                    .map(|t| t.get_name_text().to_string())
                else {
                    continue;
                };
                map.insert(owner.clone(), class_name);
            }
        }
    }
    map
}

fn root_symbols_for_comment(comment: &LuaComment, raw_comment: &str) -> Vec<String> {
    let owner_symbol = comment.get_owner().and_then(owner_symbol_from_ast);
    if let Some(symbol) = owner_symbol {
        return vec![symbol];
    }

    let tags: Vec<LuaDocTag> = comment.get_doc_tags().collect();
    let mut class_name: Option<String> = None;
    let mut alias_name: Option<String> = None;
    for tag in &tags {
        match tag {
            LuaDocTag::Class(class_tag) => {
                let text = class_tag.syntax().text().to_string();
                class_name = class_name.or_else(|| parse_tag_primary_name(&text));
            }
            LuaDocTag::Alias(alias_tag) => {
                let text = alias_tag.syntax().text().to_string();
                alias_name = alias_name.or_else(|| parse_tag_primary_name(&text));
            }
            _ => {}
        }
    }

    // 优先从原始源码行提取（支持 `std.readmode` 这类带点的名字）。
    alias_name = alias_name.or_else(|| extract_tag_name_from_raw(raw_comment, "alias"));
    class_name = class_name.or_else(|| extract_tag_name_from_raw(raw_comment, "class"));

    let mut out: Vec<String> = Vec::new();
    if let Some(class_symbol) = class_name {
        out.push(class_symbol);
    }
    if let Some(alias_symbol) = alias_name {
        out.push(alias_symbol);
    }
    out
}

fn return_sections_from_raw(raw_comment: &str) -> Vec<ReturnSection> {
    let lines = crate::doc_text::split_lines_with_offsets(raw_comment);
    let mut sections = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start_text(raw_comment);
        let Some(after_return) = doc_tag_payload(t, "@return") else {
            continue;
        };

        let mut items: Vec<(String, String)> = Vec::new();
        // 仅处理 `@return`（无类型列表）+ 后续 `---| ... # ...` 的写法。
        if after_return.trim().is_empty() {
            let mut j = i + 1;
            while j < lines.len() && lines[j].text(raw_comment).trim().is_empty() {
                j += 1;
            }
            while j < lines.len() {
                let lt = lines[j].trim_start_text(raw_comment);
                if is_doc_tag_line(lt) {
                    break;
                }
                if is_doc_continue_or_line(lt) {
                    if let Some(value) = parse_union_item_value_from_line_trim(lt) {
                        let desc = lt
                            .split_once('#')
                            .map(|(_, after)| after.trim().to_string())
                            .unwrap_or_default();
                        items.push((value, desc));
                    }
                    j += 1;
                    continue;
                }
                if lt.trim().is_empty() {
                    j += 1;
                    continue;
                }
                break;
            }
        }

        sections.push(ReturnSection {
            line_start: line.start,
            line_end: line.end,
            items,
        });
    }

    sections
}

fn preprocess_description(description: &str) -> String {
    // 行为尽量对齐 crates/emmylua_code_analysis/src/compilation/analyzer/doc/mod.rs
    let mut description = description;
    if description.starts_with(['#', '@']) {
        description = description.trim_start_matches(['#', '@']);
    }

    let mut result = String::new();
    let lines = description.lines();
    let mut start_with_one_space: Option<bool> = None;
    for mut line in lines {
        let indent_count = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent_count == line.len() {
            result.push('\n');
            continue;
        }

        if start_with_one_space.is_none() {
            start_with_one_space = Some(indent_count == 1);
        }

        if let Some(true) = start_with_one_space {
            let mut chars = line.chars();
            if let Some(first) = chars.next()
                && first.is_whitespace()
            {
                line = chars.as_str();
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    result.trim_end().to_string()
}

fn extract_version_suffix(comment: &LuaComment, raw_comment: &str) -> Option<String> {
    // 优先从 AST tag 里提取；无法提取时再从 raw 做简单兜底。
    for tag in comment.get_doc_tags() {
        if let LuaDocTag::Version(version_tag) = tag {
            let raw = version_tag.syntax().text().to_string();
            if let Some(remainder) = extract_version_remainder(&raw)
                && !remainder.is_empty()
            {
                let compact = remainder.split_whitespace().collect::<String>();
                return Some(format!("@{compact}"));
            }
        }
    }

    // 兜底：逐行解析 `---@version ...` / `--- @version ...` / `-- @version ...`。
    for line in raw_comment.lines() {
        let Some(remainder) = doc_tag_payload(line.trim_start(), "@version") else {
            continue;
        };
        let remainder = remainder.trim();
        if remainder.is_empty() {
            return None;
        }
        let compact = remainder.split_whitespace().collect::<String>();
        return Some(format!("@{compact}"));
    }

    None
}

fn extract_version_remainder(tag_text: &str) -> Option<String> {
    let s = tag_text.trim();
    if let Some(after) = s.strip_prefix("@version") {
        return Some(after.trim().to_string());
    }
    if let Some(after) = s.strip_prefix("version") {
        return Some(after.trim().to_string());
    }
    if let Some(at) = s.find("@version") {
        let after = &s[(at + "@version".len())..];
        return Some(after.trim().to_string());
    }
    None
}

fn parse_tag_primary_name(tag_text: &str) -> Option<String> {
    // 从 tag 的语法文本中提取 `@<tag>` 后面的第一个符号：
    // 例：`---@alias std.readmode` -> `std.readmode`
    // 例：`---@class file` -> `file`
    // 例：`---@class foo:bar` -> `foo`
    let s = tag_text.trim();
    let at = s.find('@')?;
    let after_at = &s[at + 1..];
    let mut iter = after_at.split_whitespace();
    let _tag_name = iter.next()?; // alias/class 等
    let name = iter.next()?;
    let name = name.trim_end_matches(['\r', '\n']).trim_end_matches(',');
    let stop_at = name.find([':', '<']).unwrap_or(name.len());
    Some(name[..stop_at].to_string())
}

fn extract_tag_name_from_raw(raw_comment: &str, tag: &str) -> Option<String> {
    let needle = format!("@{tag}");
    for line in raw_comment.lines() {
        let t = line.trim_start();
        let Some(after) = doc_tag_payload(t, &needle) else {
            continue;
        };
        let after = after.trim();
        if after.is_empty() {
            continue;
        }
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, ':' | '<'))
            .unwrap_or(after.len());
        return Some(after[..end].to_string());
    }
    None
}

fn is_whitespace_between(content: &str, from: usize, to: usize) -> bool {
    if from >= to {
        return true;
    }
    let Some(slice) = content.get(from..to) else {
        return false;
    };
    slice.chars().all(|c| c.is_whitespace())
}

fn format_doc_field_key(key: emmylua_parser::LuaDocFieldKey) -> Option<String> {
    match key {
        emmylua_parser::LuaDocFieldKey::Name(name) => Some(name.get_name_text().to_string()),
        emmylua_parser::LuaDocFieldKey::String(s) => Some(s.get_value()),
        emmylua_parser::LuaDocFieldKey::Integer(i) => Some(i.get_number_value().to_string()),
        emmylua_parser::LuaDocFieldKey::Type(t) => Some(t.syntax().text().to_string()),
    }
}

fn literal_value_from_doc_type(typ: &LuaDocType) -> Option<String> {
    match typ {
        LuaDocType::Literal(lit) => match lit.get_literal()? {
            LuaLiteralToken::String(s) => Some(s.get_value()),
            LuaLiteralToken::Number(n) => Some(n.get_number_value().to_string()),
            LuaLiteralToken::Bool(b) => Some(b.syntax().text().to_string()),
            LuaLiteralToken::Nil(n) => Some(n.syntax().text().to_string()),
            other => Some(other.syntax().text().to_string()),
        },
        _ => None,
    }
}

fn extract_owner_description_fallback(raw_comment: &str) -> Option<String> {
    // 从源码行做一次兜底提取（尽力而为）：
    // - 跳过开头的 `---@...` tag 行
    // - 收集连续的 doc 描述行（但不包含 tag、也不包含 union 行）
    // - 遇到 tag/union/非 doc 行则停止
    let mut lines = raw_comment.lines().peekable();
    while let Some(line) = lines.peek() {
        let t = line.trim_start();
        if is_doc_tag_line(t) {
            let _ = lines.next();
            continue;
        }
        break;
    }

    let mut buf: Vec<String> = Vec::new();
    while let Some(line) = lines.peek() {
        let t = line.trim_start();
        if is_doc_tag_line(t) || is_doc_continue_or_line(t) {
            break;
        }
        if let Some(payload) = comment_payload(t) {
            let mut s = payload;
            if let Some(rest) = s.strip_prefix(' ') {
                s = rest;
            }
            buf.push(s.to_string());
            let _ = lines.next();
            continue;
        }
        break;
    }

    if buf.is_empty() {
        None
    } else {
        Some(buf.join("\n"))
    }
}

pub(crate) fn owner_symbol_from_ast(owner: LuaAst) -> Option<String> {
    match owner {
        LuaAst::LuaFuncStat(func) => {
            let var = func.get_func_name()?;
            format_var_expr_path_var(&var)
        }
        LuaAst::LuaLocalFuncStat(local_func) => {
            let local_name = local_func.get_local_name()?;
            Some(local_name.get_name_token()?.get_name_text().to_string())
        }
        LuaAst::LuaAssignStat(assign) => {
            let (vars, _) = assign.get_var_and_expr_list();
            let v = vars.first()?;
            format_var_expr_path_var(v)
        }
        LuaAst::LuaLocalStat(local_stat) => {
            let name = local_stat.get_local_name_list().next()?;
            Some(name.get_name_token()?.get_name_text().to_string())
        }
        _ => None,
    }
}

fn format_var_expr_path_var(var: &LuaVarExpr) -> Option<String> {
    match var {
        LuaVarExpr::NameExpr(name) => Some(name.get_name_token()?.get_name_text().to_string()),
        LuaVarExpr::IndexExpr(index) => format_index_expr_path(index),
    }
}

fn format_expr_path(expr: &LuaExpr) -> Option<String> {
    match expr {
        LuaExpr::NameExpr(name) => Some(name.get_name_token()?.get_name_text().to_string()),
        LuaExpr::IndexExpr(index) => format_index_expr_path(index),
        _ => None,
    }
}

fn format_index_expr_path(index: &LuaIndexExpr) -> Option<String> {
    let prefix = format_expr_path(&index.get_prefix_expr()?)?;
    let key = index.get_index_key()?;
    match key {
        emmylua_parser::LuaIndexKey::Name(name) => {
            Some(format!("{prefix}.{}", name.get_name_text()))
        }
        emmylua_parser::LuaIndexKey::String(s) => Some(format!("{prefix}.{}", s.get_value())),
        emmylua_parser::LuaIndexKey::Integer(i) => {
            Some(format!("{prefix}.{}", i.get_number_value()))
        }
        _ => None,
    }
}
