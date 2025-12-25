use crate::comment_syntax::{is_doc_tag_line, parse_union_item_value_from_line_trim};
use crate::keys::{
    build_module_table_to_class_map, locale_key_desc, locale_key_field, locale_key_item,
    locale_key_param, locale_key_return, locale_key_return_item, map_symbol_for_locale_key,
};
use crate::model::{
    AnalyzedLuaDocFile, ExtractedComment, ExtractedEntry, ExtractedFile, ExtractedKind, SourceSpan,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaComment, LuaDocDescriptionOwner, LuaDocMultiLineUnionType,
    LuaDocTag, LuaDocType, LuaExpr, LuaIndexExpr, LuaLiteralToken, LuaParser, LuaVarExpr,
    ParserConfig,
};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct RootContext {
    span: SourceSpan,
    symbol: String,
    base: String,
    version_suffix: Option<String>,
}

type LocaleBaseMap = HashMap<(SourceSpan, String), String>;

/// 从 `std_dir/*.lua` 提取 i18n 条目，并尽量保持“分析顺序”：
/// - 文件顺序：按相对路径排序（稳定、可复现）
/// - 文件内：按注释在源码中的位置（text_range.start）排序
/// - 注释内：按 tag/item 在源码中的出现顺序输出
pub fn extract_std_dir(
    std_dir: &Path,
    include_empty: bool,
) -> Result<Vec<ExtractedFile>, Box<dyn std::error::Error>> {
    let mut files: Vec<(PathBuf, PathBuf)> = WalkDir::new(std_dir)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lua"))
        .filter_map(|full| {
            let rel = full.strip_prefix(std_dir).ok()?.to_path_buf();
            Some((rel, full))
        })
        .collect();

    files.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut out: Vec<ExtractedFile> = Vec::new();
    for (rel_path, full_path) in files {
        let file_name = rel_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let content = std::fs::read_to_string(&full_path)?;
        let analyzed = analyze_lua_doc_file(&file_name, &content, include_empty);
        out.push(ExtractedFile {
            path: rel_path,
            comments: analyzed.comments,
            entries: analyzed.entries,
        });
    }

    Ok(out)
}

pub fn analyze_lua_doc_file(
    file_name: &str,
    content: &str,
    include_empty: bool,
) -> AnalyzedLuaDocFile {
    let module_map = build_module_table_to_class_map(content);

    let tree = LuaParser::parse(content, ParserConfig::default());
    let chunk = tree.get_chunk_node();
    let mut comments: Vec<LuaComment> = chunk.descendants::<LuaComment>().collect();
    comments.sort_by_key(|c| c.syntax().text_range().start());

    #[derive(Debug, Clone)]
    struct CommentRecord {
        comment: LuaComment,
        span: SourceSpan,
        raw: String,
        effective_version: Option<String>,
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
            effective_version,
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
            file_name,
            &r.comment,
            &r.raw,
            include_empty,
            r.effective_version.as_deref(),
            r.span,
            &module_map,
            &locale_base_map,
            &mut comment_entries,
            &mut seen,
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

    AnalyzedLuaDocFile {
        module_map,
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

fn extract_from_comment(
    file_name: &str,
    comment: &LuaComment,
    raw_comment: &str,
    include_empty: bool,
    version_suffix: Option<&str>,
    comment_span: SourceSpan,
    module_map: &HashMap<String, String>,
    locale_base_map: &LocaleBaseMap,
    out: &mut Vec<ExtractedEntry>,
    seen: &mut HashSet<String>,
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
            out,
            seen,
            ExtractedEntry {
                key: make_key(file_name, symbol, version_suffix, "desc", None),
                locale_key: locale_key_desc(&locale_base),
                kind: ExtractedKind::Desc,
                symbol: symbol.to_string(),
                base: base.clone(),
                version_suffix: version_suffix.map(|s| s.to_string()),
                comment_span,
                raw: desc_raw,
                value: desc_text,
            },
            include_empty,
        );

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
                        out,
                        seen,
                        ExtractedEntry {
                            key: make_key(file_name, symbol, version_suffix, "param", Some(&name)),
                            locale_key: locale_key_param(&locale_base, &name),
                            kind: ExtractedKind::Param { name: name.clone() },
                            symbol: symbol.to_string(),
                            base: base.clone(),
                            version_suffix: version_suffix.map(|s| s.to_string()),
                            comment_span,
                            raw: raw_desc,
                            value: text,
                        },
                        include_empty,
                    );
                }
                LuaDocTag::Return(ret) => {
                    return_index += 1;
                    let ident = return_index.to_string();
                    let raw_desc = ret
                        .get_description()
                        .map(|d| d.get_description_text())
                        .unwrap_or_default();
                    let text = preprocess_description(&raw_desc);
                    push_entry(
                        out,
                        seen,
                        ExtractedEntry {
                            key: make_key(
                                file_name,
                                symbol,
                                version_suffix,
                                "return",
                                Some(&ident),
                            ),
                            locale_key: locale_key_return(&locale_base, &ident),
                            kind: ExtractedKind::Return {
                                index: return_index,
                            },
                            symbol: symbol.to_string(),
                            base: base.clone(),
                            version_suffix: version_suffix.map(|s| s.to_string()),
                            comment_span,
                            raw: raw_desc,
                            value: text,
                        },
                        include_empty,
                    );

                    for (value, raw_item_desc) in
                        return_union_items_for_index(raw_comment, return_index)
                    {
                        let item_key = format!("{ident}.{value}");
                        let text = preprocess_description(&raw_item_desc);
                        push_entry(
                            out,
                            seen,
                            ExtractedEntry {
                                key: make_key(
                                    file_name,
                                    symbol,
                                    version_suffix,
                                    "return_item",
                                    Some(&item_key),
                                ),
                                locale_key: locale_key_return_item(&locale_base, &ident, &value),
                                kind: ExtractedKind::ReturnItem {
                                    index: return_index,
                                    value: value.clone(),
                                },
                                symbol: symbol.to_string(),
                                base: base.clone(),
                                version_suffix: version_suffix.map(|s| s.to_string()),
                                comment_span,
                                raw: raw_item_desc,
                                value: text,
                            },
                            include_empty,
                        );
                    }
                }
                _ => {}
            }
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
            out,
            seen,
            ExtractedEntry {
                key: make_key(file_name, class_symbol, version_suffix, "desc", None),
                locale_key: locale_key_desc(&locale_base),
                kind: ExtractedKind::Desc,
                symbol: class_symbol.to_string(),
                base: base.clone(),
                version_suffix: version_suffix.map(|s| s.to_string()),
                comment_span,
                raw: desc_raw,
                value: text,
            },
            include_empty,
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
                    out,
                    seen,
                    ExtractedEntry {
                        key: make_key(
                            file_name,
                            class_symbol,
                            version_suffix,
                            "field",
                            Some(&field_name),
                        ),
                        locale_key: locale_key_field(&locale_base, &field_name),
                        kind: ExtractedKind::Field {
                            name: field_name.clone(),
                        },
                        symbol: class_symbol.to_string(),
                        base: base.clone(),
                        version_suffix: version_suffix.map(|s| s.to_string()),
                        comment_span,
                        raw: raw_desc,
                        value: text,
                    },
                    include_empty,
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
            out,
            seen,
            ExtractedEntry {
                key: make_key(file_name, alias_symbol, version_suffix, "desc", None),
                locale_key: locale_key_desc(&locale_base),
                kind: ExtractedKind::Desc,
                symbol: alias_symbol.to_string(),
                base: base.clone(),
                version_suffix: version_suffix.map(|s| s.to_string()),
                comment_span,
                raw: desc_raw,
                value: text,
            },
            include_empty,
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
                    out,
                    seen,
                    ExtractedEntry {
                        key: make_key(
                            file_name,
                            alias_symbol,
                            version_suffix,
                            "item",
                            Some(&value),
                        ),
                        locale_key: locale_key_item(&locale_base, &value),
                        kind: ExtractedKind::Item {
                            value: value.clone(),
                        },
                        symbol: alias_symbol.to_string(),
                        base: base.clone(),
                        version_suffix: version_suffix.map(|s| s.to_string()),
                        comment_span,
                        raw: raw_desc,
                        value: text,
                    },
                    include_empty,
                );
            }
        }
    }
}

fn push_entry(
    out: &mut Vec<ExtractedEntry>,
    seen: &mut HashSet<String>,
    entry: ExtractedEntry,
    include_empty: bool,
) {
    if entry.value.trim().is_empty() && !include_empty {
        return;
    }
    if !seen.insert(entry.locale_key.clone()) {
        let _ = writeln!(
            io::stderr(),
            "warning: duplicate locale_key {} (kept first, ignored new value)",
            entry.locale_key
        );
        return;
    }
    out.push(entry);
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

fn return_union_items_for_index(raw_comment: &str, index: usize) -> Vec<(String, String)> {
    let mut return_idx = 0usize;
    let mut lines: Vec<&str> = raw_comment.lines().collect();
    if raw_comment.contains("\r\n") {
        lines = raw_comment.split("\r\n").collect();
    }

    for i in 0..lines.len() {
        let t = lines[i].trim_start();
        let Some(after_triple) = t.strip_prefix("---") else {
            continue;
        };
        let after_triple = after_triple.trim_start();
        let Some(after_return) = after_triple.strip_prefix("@return") else {
            continue;
        };
        return_idx += 1;
        if return_idx != index {
            continue;
        }

        let after = after_return.trim();
        // 仅处理 `@return`（无类型列表）+ 后续 `---| ... # ...` 的写法。
        if !after.is_empty() {
            return Vec::new();
        }

        let mut out: Vec<(String, String)> = Vec::new();
        let mut j = i + 1;
        while j < lines.len() && lines[j].trim().is_empty() {
            j += 1;
        }
        while j < lines.len() {
            let lt = lines[j].trim_start();
            if is_doc_tag_line(lt) {
                break;
            }
            if lt.starts_with("---|") {
                if let Some(value) = parse_union_item_value_from_line_trim(lt) {
                    let desc = lt
                        .split_once('#')
                        .map(|(_, after)| after.trim().to_string())
                        .unwrap_or_default();
                    out.push((value, desc));
                }
                j += 1;
                continue;
            }
            break;
        }
        return out;
    }

    Vec::new()
}

fn make_key(
    file_name: &str,
    symbol: &str,
    version_suffix: Option<&str>,
    section: &str,
    ident: Option<&str>,
) -> String {
    let mut key = String::new();
    key.push_str(file_name);
    key.push_str("::");
    key.push_str(symbol);
    if let Some(v) = version_suffix {
        key.push_str(v);
    }
    key.push_str("::");
    key.push_str(section);
    if let Some(ident) = ident {
        key.push('.');
        key.push_str(ident);
    }
    key
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

    // 兜底：逐行解析 `---@version ...` / `--- @version ...`（避免把整段 comment 都当成版本后缀）。
    for line in raw_comment.lines() {
        let t = line.trim_start();
        let remainder = if let Some(rest) = t.strip_prefix("@version") {
            rest
        } else if let Some(rest) = t.strip_prefix("---@version") {
            rest
        } else if let Some(rest) = t.strip_prefix("--- @version") {
            rest
        } else {
            // 通用形式：`---` 后允许若干空格，再跟 `@version`
            let Some(after) = t.strip_prefix("---") else {
                continue;
            };
            let after = after.trim_start();
            let Some(after) = after.strip_prefix("@version") else {
                continue;
            };
            after
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
        let Some(at) = line.find(&needle) else {
            continue;
        };
        let after = &line[(at + needle.len())..];
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
    // - 收集连续的 `--- ...` 描述行（但不包含 `---@`、也不包含 `---|` union 行）
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
        if is_doc_tag_line(t) || t.starts_with("---|") {
            break;
        }
        if t.starts_with("---") {
            let mut s = t.trim_start_matches("---");
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
