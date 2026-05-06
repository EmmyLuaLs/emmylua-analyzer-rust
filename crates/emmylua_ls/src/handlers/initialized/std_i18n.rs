use std::collections::HashMap;
use std::path::{Path, PathBuf};

use emmylua_code_analysis::{LuaFileInfo, get_best_resources_dir, get_locale_code};
use include_dir::{Dir, include_dir};

static STD_I18N_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/std_i18n");

#[derive(Debug, Clone, serde::Deserialize)]
struct MetaFile {
    v: u32,
    #[allow(dead_code)]
    f: String,
    b: Vec<MetaBlock>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MetaBlock {
    a: Option<String>,
    g: Option<String>,
    r: [u32; 4],
    e: Vec<MetaEntry>,
}

#[derive(Debug, Clone)]
struct MetaEntry {
    k: String,
    s: String,
    t: MetaKind,
    r: [u32; 4],
    h: String,
    i: Option<String>,
    p: Option<String>,
}

impl<'de> serde::Deserialize<'de> for MetaEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MetaEntryVisitor;

        impl<'de> serde::de::Visitor<'de> for MetaEntryVisitor {
            type Value = MetaEntry;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a std i18n meta entry sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let k = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let s = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let t: MetaKind = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                let r = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
                let h = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;
                let affix: Option<String> = seq.next_element()?;
                let (i, p) = match t {
                    MetaKind::Doc => (affix, None),
                    MetaKind::Tail => (None, affix),
                };

                Ok(MetaEntry {
                    k,
                    s,
                    t,
                    r,
                    h,
                    i,
                    p,
                })
            }
        }

        deserializer.deserialize_seq(MetaEntryVisitor)
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum MetaKind {
    Doc,
    Tail,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 尝试生成翻译后的 std
pub fn try_generate_translated_std() -> Option<()> {
    let locale = get_locale_code(&rust_i18n::locale());
    if locale == "en" {
        return Some(());
    }

    // 确定是否存在对应语言的翻译文件
    let first_sub_dir = STD_I18N_DIR
        .entries()
        .iter()
        .filter_map(|e| e.as_dir())
        .next()?;

    let locale_yaml = format!("{}.yaml", locale);
    let has_locale_file = first_sub_dir
        .entries()
        .iter()
        .filter_map(|e| e.as_file())
        .any(|f| {
            f.path()
                .file_name()
                .is_some_and(|n| n == locale_yaml.as_str())
        });
    if !has_locale_file {
        return Some(());
    }

    let resources_dir = get_best_resources_dir();
    if !check_need_dump_std(&resources_dir, &locale) {
        return None;
    }
    // 获取最佳资源目录作为输出目录的父目录
    generate(&locale, &resources_dir);
    Some(())
}

/// 检查是否需要重新生成翻译后的 std 文件
fn check_need_dump_std(resources_dir: &Path, locale: &str) -> bool {
    // debug 模式下总是重新生成
    if cfg!(debug_assertions) {
        return true;
    }
    // 不存在对应语言的翻译文件, 需要生成
    let translated_std_dir = resources_dir.join(format!("std-{locale}"));
    if !translated_std_dir.exists() {
        return true;
    }

    let version_path = resources_dir.join("version");

    // 版本文件不存在, 需要重新生成
    if !version_path.exists() {
        return true;
    }

    // 读取版本文件失败, 需要重新生成
    let Ok(content) = std::fs::read_to_string(&version_path) else {
        return true;
    };

    // 版本不匹配, 需要重新生成
    let version = content.trim();
    if version != VERSION {
        return true;
    }
    false
}

/// Params:
/// - `locale` - 语言
/// - `out_parent_dir` - 输出目录的父目录
fn generate(locale: &str, out_parent_dir: &Path) -> Vec<LuaFileInfo> {
    let origin_std_files = emmylua_code_analysis::load_resource_from_include_dir();
    let translate_std_root = out_parent_dir.join(format!("std-{locale}"));
    log::info!("Creating std-{locale} dir: {:?}", translate_std_root);

    let mut out_files: Vec<LuaFileInfo> = Vec::with_capacity(origin_std_files.len());

    for file in origin_std_files {
        let rel = match std_rel_path(&file.path) {
            Some(r) => r,
            None => continue,
        };

        let translated =
            translate_one_std_file(locale, &rel, &file.content).unwrap_or(file.content);
        let out_path = translate_std_root.join(&rel);
        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&out_path, &translated);

        out_files.push(LuaFileInfo {
            path: out_path.to_string_lossy().to_string(),
            content: translated,
        });
    }

    out_files
}

fn translate_one_std_file(locale: &str, rel_lua_path: &Path, content: &str) -> Option<String> {
    let stem = rel_lua_path.with_extension("");
    let stem_str = stem.to_string_lossy().replace('\\', "/");

    let meta_path = format!("{stem_str}/meta.yaml");
    let tr_path = format!("{stem_str}/{locale}.yaml");

    let meta = read_meta(&meta_path)?;
    let translations = read_translations(&tr_path)?;

    Some(apply_meta_translations(content, &meta, &translations))
}

fn read_meta(path_in_dir: &str) -> Option<MetaFile> {
    let file = STD_I18N_DIR.get_file(path_in_dir)?;
    let raw = file.contents_utf8()?;
    serde_yml::from_str(raw).ok()
}

fn read_translations(path_in_dir: &str) -> Option<HashMap<String, String>> {
    let file = STD_I18N_DIR.get_file(path_in_dir)?;
    let raw = file.contents_utf8()?;
    serde_yml::from_str(raw).ok()
}

fn apply_meta_translations(
    content: &str,
    meta: &MetaFile,
    translations: &HashMap<String, String>,
) -> String {
    if meta.v != 2 {
        return content.to_string();
    }

    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let line_starts = build_line_start_offsets(content);

    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for block in &meta.b {
        let block_span = locate_meta_block(content, &line_starts, block);
        for entry in &block.e {
            let Some(translated) = translations
                .get(&entry.k)
                .map(|s| s.to_string())
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };

            let Some((start, end)) =
                locate_meta_entry(content, &line_starts, block, block_span, entry)
            else {
                continue;
            };

            let rep = match entry.t {
                MetaKind::Doc => {
                    let indent = entry
                        .i
                        .as_deref()
                        .map(str::to_string)
                        .unwrap_or_else(|| line_indent_at(content, start));
                    let mut rep = build_doc_block_string(&indent, &translated, newline);
                    if line_break_len_at(content, end) > 0 && rep.ends_with(newline) {
                        rep.truncate(rep.len().saturating_sub(newline.len()));
                    }
                    rep
                }
                MetaKind::Tail => {
                    let one_line = to_one_line(&translated);
                    format!("{}{}", entry.p.as_deref().unwrap_or(""), one_line)
                }
            };

            replacements.push((start, end, rep));
        }
    }

    if replacements.is_empty() {
        return content.to_string();
    }

    replacements.sort_by_key(|(s, _, _)| *s);
    let mut out = String::with_capacity(content.len() + 256);
    let mut cursor = 0usize;
    for (start, end, rep) in replacements {
        if start < cursor || end < start || end > content.len() {
            continue;
        }
        out.push_str(&content[cursor..start]);
        out.push_str(&rep);
        cursor = end;
    }
    out.push_str(&content[cursor..]);
    out
}

fn locate_meta_entry(
    content: &str,
    line_starts: &[usize],
    block: &MetaBlock,
    block_span: Option<(usize, usize)>,
    entry: &MetaEntry,
) -> Option<(usize, usize)> {
    if let Some((block_start, block_end)) = block_span {
        if let Some(range) = range_to_offsets(line_starts, entry.r)
            && range.0 >= block_start
            && range.1 <= block_end
            && hash_matches(content, range, &entry.h)
        {
            return Some(range);
        }

        if let Some(range) = locate_entry_by_relative_range(content, block, block_start, entry)
            && range.0 >= block_start
            && range.1 <= block_end
            && hash_matches(content, range, &entry.h)
        {
            return Some(range);
        }

        if let Some(range) = locate_entry_by_selector(content, block_start, block_end, entry)
            && hash_matches(content, range, &entry.h)
        {
            return Some(range);
        }
    }

    None
}

fn locate_meta_block(
    content: &str,
    line_starts: &[usize],
    block: &MetaBlock,
) -> Option<(usize, usize)> {
    if let Some(range) = range_to_offsets(line_starts, block.r)
        && range.0 <= range.1
        && range.1 <= content.len()
        && block_guard_matches(content.get(range.0..range.1).unwrap_or(""), block)
        && block_anchor_after_range_matches(content, range.1, block)
    {
        return Some(range);
    }

    if let Some(anchor) = block.a.as_deref() {
        let mut offset = 0usize;
        while offset < content.len() {
            let line_end = find_line_end(content, offset);
            let line = content.get(offset..line_end).unwrap_or("").trim();
            if line == anchor
                && let Some(range) = doc_block_before_line(content, offset)
                && block_guard_matches(content.get(range.0..range.1).unwrap_or(""), block)
            {
                return Some(range);
            }
            offset = advance_past_line_break(content, line_end);
        }
    }

    if let Some(guard) = block.g.as_deref() {
        let mut offset = 0usize;
        while offset < content.len() {
            let line_end = find_line_end(content, offset);
            let line = content.get(offset..line_end).unwrap_or("");
            if normalize_comment_guard_line(line) == Some(guard)
                && let Some(range) = doc_block_around_line(content, offset)
            {
                return Some(range);
            }
            offset = advance_past_line_break(content, line_end);
        }
    }

    None
}

fn block_guard_matches(block_raw: &str, block: &MetaBlock) -> bool {
    block.g.as_deref().is_none_or(|guard| {
        block_raw
            .lines()
            .any(|line| normalize_comment_guard_line(line) == Some(guard))
    })
}

fn block_anchor_after_range_matches(content: &str, range_end: usize, block: &MetaBlock) -> bool {
    let Some(anchor) = block.a.as_deref() else {
        return true;
    };

    let mut offset = range_end;
    while offset < content.len() {
        let line_end = find_line_end(content, offset);
        let line = content.get(offset..line_end).unwrap_or("").trim();
        if !line.is_empty() {
            return line == anchor;
        }
        offset = advance_past_line_break(content, line_end);
    }
    false
}

fn locate_entry_by_relative_range(
    content: &str,
    block: &MetaBlock,
    block_start: usize,
    entry: &MetaEntry,
) -> Option<(usize, usize)> {
    let block_line = block.r[0];
    let start_line_delta = entry.r[0].checked_sub(block_line)? as usize;
    let end_line_delta = entry.r[2].checked_sub(block_line)? as usize;
    let block_raw = content.get(block_start..)?;
    let block_line_starts = build_line_start_offsets(block_raw);
    let rel_start_line = *block_line_starts.get(start_line_delta)?;
    let rel_end_line = *block_line_starts.get(end_line_delta)?;
    let start = block_start + rel_start_line + entry.r[1] as usize;
    let end = block_start + rel_end_line + entry.r[3] as usize;
    if start <= end && end <= content.len() {
        Some((start, end))
    } else {
        None
    }
}

fn std_rel_path(path: &str) -> Option<PathBuf> {
    // `emmylua_code_analysis` 嵌入资源的路径形如 `std/builtin.lua`.
    let p = Path::new(path);
    let mut it = p.components();
    let first = it.next()?.as_os_str().to_string_lossy();
    if first != "std" {
        return None;
    }
    let rest = it.as_path();
    Some(rest.to_path_buf())
}

fn build_line_start_offsets(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    out.push(0);
    for (i, b) in s.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            out.push(i + 1);
        }
    }
    out
}

fn range_to_offsets(line_starts: &[usize], range: [u32; 4]) -> Option<(usize, usize)> {
    let start = line_col_to_offset(line_starts, range[0] as usize, range[1] as usize)?;
    let end = line_col_to_offset(line_starts, range[2] as usize, range[3] as usize)?;
    Some((start, end))
}

fn line_col_to_offset(line_starts: &[usize], line: usize, col: usize) -> Option<usize> {
    let line_start = *line_starts.get(line)?;
    Some(line_start.saturating_add(col))
}

fn hash_matches(content: &str, range: (usize, usize), expected: &str) -> bool {
    let (start, end) = range;
    if start > end || end > content.len() {
        return false;
    }
    content
        .get(start..end)
        .is_some_and(|slice| fnv1a64_hex(slice) == expected)
}

fn line_indent_at(content: &str, offset: usize) -> String {
    let line_start = content[..offset.min(content.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    content[line_start..offset.min(content.len())]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect()
}

fn find_line_end(content: &str, offset: usize) -> usize {
    content[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(content.len())
}

fn advance_past_line_break(content: &str, offset: usize) -> usize {
    let bytes = content.as_bytes();
    if offset < bytes.len() && bytes[offset] == b'\r' {
        if offset + 1 < bytes.len() && bytes[offset + 1] == b'\n' {
            return offset + 2;
        }
        return offset + 1;
    }
    if offset < bytes.len() && bytes[offset] == b'\n' {
        return offset + 1;
    }
    offset
}

fn doc_block_before_line(content: &str, line_start: usize) -> Option<(usize, usize)> {
    let mut lines = split_lines_with_offsets(content);
    lines.retain(|line| line.start < line_start);
    let mut end_idx = lines.len();
    while end_idx > 0 && lines[end_idx - 1].text(content).trim().is_empty() {
        end_idx -= 1;
    }
    if end_idx == 0 {
        return None;
    }

    let mut start_idx = end_idx;
    while start_idx > 0 && is_doc_comment_line(lines[start_idx - 1].trim_start_text(content)) {
        start_idx -= 1;
    }
    if start_idx == end_idx {
        return None;
    }

    Some((
        lines[start_idx].start,
        lines[end_idx - 1].line_end_with_break(content),
    ))
}

fn doc_block_around_line(content: &str, line_start: usize) -> Option<(usize, usize)> {
    let lines = split_lines_with_offsets(content);
    let idx = lines.iter().position(|line| {
        line.start == line_start && is_doc_comment_line(line.trim_start_text(content))
    })?;

    let mut start_idx = idx;
    while start_idx > 0 && is_doc_comment_line(lines[start_idx - 1].trim_start_text(content)) {
        start_idx -= 1;
    }

    let mut end_idx = idx + 1;
    while end_idx < lines.len() && is_doc_comment_line(lines[end_idx].trim_start_text(content)) {
        end_idx += 1;
    }

    Some((
        lines[start_idx].start,
        lines[end_idx - 1].line_end_with_break(content),
    ))
}

fn locate_entry_by_selector(
    content: &str,
    block_start: usize,
    block_end: usize,
    entry: &MetaEntry,
) -> Option<(usize, usize)> {
    let raw = content.get(block_start..block_end)?;
    let ctx = RuntimeCommentContext::new(raw);
    match parse_selector(&entry.s)? {
        RuntimeSelector::Desc => desc_replace_target(&ctx, block_start),
        RuntimeSelector::Param(name) => {
            let tag_idx = ctx.param_line(&name)?;
            tag_attached_replace_target_after(&ctx, block_start, tag_idx, entry)
        }
        RuntimeSelector::Return(index) => {
            let tag_idx = ctx.return_line(index)?;
            tag_attached_replace_target_after(&ctx, block_start, tag_idx, entry)
        }
        RuntimeSelector::Field(name) => {
            let tag_idx = ctx.field_line(&name)?;
            tag_attached_replace_target_after(&ctx, block_start, tag_idx, entry)
        }
        RuntimeSelector::Item(value) => union_item_replace_target(&ctx, block_start, &value),
        RuntimeSelector::ReturnItem { index, value } => {
            return_union_item_replace_target(&ctx, block_start, index, &value)
                .or_else(|| union_item_replace_target(&ctx, block_start, &value))
        }
    }
}

fn desc_replace_target(
    ctx: &RuntimeCommentContext<'_>,
    block_start: usize,
) -> Option<(usize, usize)> {
    let (rel_start, rel_end) = if let Some((start, end)) = ctx.desc_block_line_range() {
        let start_off = ctx.lines.get(start)?.start;
        let end_off = if end < ctx.lines.len() {
            ctx.lines.get(end)?.start
        } else {
            ctx.raw.len()
        };
        (start_off, end_off)
    } else {
        (0, 0)
    };

    Some((block_start + rel_start, block_start + rel_end))
}

fn tag_attached_replace_target_after(
    ctx: &RuntimeCommentContext<'_>,
    block_start: usize,
    tag_idx: usize,
    entry: &MetaEntry,
) -> Option<(usize, usize)> {
    if matches!(entry.t, MetaKind::Tail) {
        return inline_or_insert_target(ctx, block_start, tag_idx);
    }
    attached_doc_block_target_after(ctx, block_start, tag_idx)
}

fn inline_or_insert_target(
    ctx: &RuntimeCommentContext<'_>,
    block_start: usize,
    tag_idx: usize,
) -> Option<(usize, usize)> {
    let li = *ctx.lines.get(tag_idx)?;
    let line_text = li.text(ctx.raw);
    if let Some(hash_pos) = line_text.find('#') {
        return Some((block_start + li.start + hash_pos + 1, block_start + li.end));
    }

    let tail_start = tag_description_tail_start(line_text).unwrap_or(li.end - li.start);
    let start = block_start + li.start + tail_start;
    if start < block_start + li.end {
        Some((start, block_start + li.end))
    } else {
        Some((block_start + li.end, block_start + li.end))
    }
}

fn attached_doc_block_target_after(
    ctx: &RuntimeCommentContext<'_>,
    block_start: usize,
    tag_idx: usize,
) -> Option<(usize, usize)> {
    let mut start = tag_idx + 1;
    while start < ctx.lines.len() && ctx.lines[start].text(ctx.raw).trim().is_empty() {
        start += 1;
    }

    let mut end = start;
    while end < ctx.lines.len() {
        let t = ctx.lines[end].trim_start_text(ctx.raw);
        if is_doc_tag_line(t) || is_doc_continue_or_line(t) {
            break;
        }
        if is_doc_comment_line(t) {
            end += 1;
            continue;
        }
        break;
    }

    if start < end {
        let rel_start = ctx.lines.get(start)?.start;
        let rel_end = if end < ctx.lines.len() {
            ctx.lines.get(end)?.start
        } else {
            ctx.raw.len()
        };
        Some((block_start + rel_start, block_start + rel_end))
    } else {
        let rel_insert = if start < ctx.lines.len() {
            ctx.lines.get(start)?.start
        } else {
            ctx.raw.len()
        };
        Some((block_start + rel_insert, block_start + rel_insert))
    }
}

fn union_item_replace_target(
    ctx: &RuntimeCommentContext<'_>,
    block_start: usize,
    value: &str,
) -> Option<(usize, usize)> {
    let line_idx = ctx.union_line(value)?;
    union_item_replace_target_at_line(ctx, block_start, line_idx)
}

fn return_union_item_replace_target(
    ctx: &RuntimeCommentContext<'_>,
    block_start: usize,
    index: usize,
    value: &str,
) -> Option<(usize, usize)> {
    let return_idx = ctx.return_line(index)?;
    let mut line_idx = return_idx + 1;
    while line_idx < ctx.lines.len() {
        let t = ctx.lines[line_idx].trim_start_text(ctx.raw);
        if is_doc_tag_line(t) {
            break;
        }
        if is_doc_continue_or_line(t) {
            if parse_union_item_value_from_line_trim(t).as_deref() == Some(value) {
                return union_item_replace_target_at_line(ctx, block_start, line_idx);
            }
            line_idx += 1;
            continue;
        }
        if t.trim().is_empty() {
            line_idx += 1;
            continue;
        }
        break;
    }
    None
}

fn union_item_replace_target_at_line(
    ctx: &RuntimeCommentContext<'_>,
    block_start: usize,
    line_idx: usize,
) -> Option<(usize, usize)> {
    let li = *ctx.lines.get(line_idx)?;
    let line_text = li.text(ctx.raw);
    if let Some(hash_pos) = line_text.find('#') {
        Some((block_start + li.start + hash_pos + 1, block_start + li.end))
    } else {
        Some((block_start + li.end, block_start + li.end))
    }
}

#[derive(Debug, Clone, Copy)]
struct LineInfo {
    start: usize,
    end: usize,
}

impl LineInfo {
    fn text<'a>(&self, raw: &'a str) -> &'a str {
        raw.get(self.start..self.end).unwrap_or("")
    }

    fn trim_start_text<'a>(&self, raw: &'a str) -> &'a str {
        self.text(raw).trim_start()
    }

    fn line_end_with_break(&self, raw: &str) -> usize {
        let bytes = raw.as_bytes();
        if self.end < bytes.len() && bytes[self.end] == b'\r' {
            if self.end + 1 < bytes.len() && bytes[self.end + 1] == b'\n' {
                return self.end + 2;
            }
            return self.end + 1;
        }
        if self.end < bytes.len() && bytes[self.end] == b'\n' {
            return self.end + 1;
        }
        self.end
    }
}

struct RuntimeCommentContext<'a> {
    raw: &'a str,
    lines: Vec<LineInfo>,
}

impl<'a> RuntimeCommentContext<'a> {
    fn new(raw: &'a str) -> Self {
        Self {
            raw,
            lines: split_lines_with_offsets(raw),
        }
    }

    fn desc_block_line_range(&self) -> Option<(usize, usize)> {
        let start = self.lines.iter().position(|li| {
            let t = li.trim_start_text(self.raw);
            !is_doc_tag_line(t) && !is_doc_continue_or_line(t) && is_doc_comment_line(t)
        })?;

        let mut end = start;
        while end < self.lines.len() {
            let t = self.lines[end].trim_start_text(self.raw);
            if is_doc_tag_line(t) || is_doc_continue_or_line(t) {
                break;
            }
            if is_doc_comment_line(t) {
                end += 1;
                continue;
            }
            break;
        }

        Some((start, end))
    }

    fn param_line(&self, name: &str) -> Option<usize> {
        self.lines.iter().position(|li| {
            parse_param_name_from_line(li.trim_start_text(self.raw)).as_deref() == Some(name)
        })
    }

    fn field_line(&self, name: &str) -> Option<usize> {
        self.lines.iter().position(|li| {
            parse_field_name_from_line(li.trim_start_text(self.raw)).as_deref() == Some(name)
        })
    }

    fn return_line(&self, index: usize) -> Option<usize> {
        let mut seen = 0usize;
        for (i, li) in self.lines.iter().enumerate() {
            if doc_tag_payload(li.trim_start_text(self.raw), "@return").is_some() {
                seen += 1;
                if seen == index {
                    return Some(i);
                }
            }
        }
        None
    }

    fn union_line(&self, value: &str) -> Option<usize> {
        self.lines.iter().position(|li| {
            parse_union_item_value_from_line_trim(li.trim_start_text(self.raw)).as_deref()
                == Some(value)
        })
    }
}

#[derive(Debug)]
enum RuntimeSelector {
    Desc,
    Param(String),
    Return(usize),
    ReturnItem { index: usize, value: String },
    Field(String),
    Item(String),
}

fn parse_selector(selector: &str) -> Option<RuntimeSelector> {
    if selector == "d" {
        return Some(RuntimeSelector::Desc);
    }
    if let Some(name) = selector.strip_prefix("p:") {
        return Some(RuntimeSelector::Param(name.to_string()));
    }
    if let Some(name) = selector.strip_prefix("f:") {
        return Some(RuntimeSelector::Field(name.to_string()));
    }
    if let Some(value) = selector.strip_prefix("i:") {
        return Some(RuntimeSelector::Item(value.to_string()));
    }
    if let Some(index) = selector.strip_prefix("r:") {
        return index.parse().ok().map(RuntimeSelector::Return);
    }
    if let Some(rest) = selector.strip_prefix("ri:") {
        let (index, value) = rest.split_once(':')?;
        return Some(RuntimeSelector::ReturnItem {
            index: index.parse().ok()?,
            value: value.to_string(),
        });
    }
    None
}

fn split_lines_with_offsets(s: &str) -> Vec<LineInfo> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();

    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut line_end = i;
            if line_end > line_start && bytes[line_end - 1] == b'\r' {
                line_end -= 1;
            }
            out.push(LineInfo {
                start: line_start,
                end: line_end,
            });
            line_start = i + 1;
        }
        i += 1;
    }

    if line_start <= bytes.len() {
        out.push(LineInfo {
            start: line_start,
            end: bytes.len(),
        });
    }

    out
}

fn is_doc_comment_line(line_trim_start: &str) -> bool {
    comment_payload(line_trim_start).is_some()
}

fn normalize_comment_guard_line(line: &str) -> Option<&str> {
    comment_payload(line).map(|text| text.trim_start().trim_end())
}

fn is_doc_tag_line(line_trim_start: &str) -> bool {
    let Some(after) = comment_payload(line_trim_start) else {
        return false;
    };
    after.trim_start().starts_with('@')
}

fn doc_continue_or_payload(line_trim_start: &str) -> Option<&str> {
    let after = comment_payload(line_trim_start)?.trim_start();
    after.strip_prefix('|').map(str::trim_start)
}

fn is_doc_continue_or_line(line_trim_start: &str) -> bool {
    doc_continue_or_payload(line_trim_start).is_some()
}

fn doc_tag_payload<'a>(line_trim_start: &'a str, tag: &str) -> Option<&'a str> {
    let after = comment_payload(line_trim_start)?.trim_start();
    let after = after.strip_prefix(tag)?;
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some(after.trim_start())
}

fn comment_payload(line_trim_start: &str) -> Option<&str> {
    let t = line_trim_start.trim_start();
    if let Some(rest) = t.strip_prefix("---") {
        return Some(rest);
    }
    let rest = t.strip_prefix("--")?;
    if rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with('|') {
        return Some(rest);
    }
    None
}

fn parse_param_name_from_line(trimmed: &str) -> Option<String> {
    let after = doc_tag_payload(trimmed, "@param")?;
    let name = after.split_whitespace().next()?;
    Some(normalize_optional_name(name))
}

fn parse_field_name_from_line(trimmed: &str) -> Option<String> {
    let after = doc_tag_payload(trimmed, "@field")?;
    let token = after.split_whitespace().next()?;
    Some(normalize_optional_name(&normalize_field_key_token(token)))
}

fn normalize_optional_name(s: &str) -> String {
    s.trim()
        .trim_end_matches('?')
        .trim_end_matches(',')
        .to_string()
}

fn normalize_field_key_token(token: &str) -> String {
    let t = token.trim();
    if let Some(inner) = t.strip_prefix("[\"").and_then(|s| s.strip_suffix("\"]")) {
        return inner.to_string();
    }
    if let Some(inner) = t.strip_prefix("['").and_then(|s| s.strip_suffix("']")) {
        return inner.to_string();
    }
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        return t[1..t.len() - 1].to_string();
    }
    t.to_string()
}

fn parse_union_item_value_from_line_trim(line_trim_start: &str) -> Option<String> {
    let after = doc_continue_or_payload(line_trim_start)?;
    let after = after.strip_prefix('>').unwrap_or(after).trim_start();
    let after = after.strip_prefix('+').unwrap_or(after).trim_start();

    if let Some(rest) = after.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }
    if let Some(rest) = after.strip_prefix('\'') {
        let end = rest.find('\'')?;
        return Some(rest[..end].to_string());
    }

    let end = after
        .find(|c: char| c.is_whitespace() || c == '#')
        .unwrap_or(after.len());
    if end == 0 {
        None
    } else {
        Some(after[..end].to_string())
    }
}

fn tag_description_tail_start(line_text: &str) -> Option<usize> {
    line_text.find('#').map(|i| i + 1)
}

fn line_break_len_at(content: &str, offset: usize) -> usize {
    let bytes = content.as_bytes();
    if offset >= bytes.len() {
        return 0;
    }
    match bytes[offset] {
        b'\r' => {
            if offset + 1 < bytes.len() && bytes[offset + 1] == b'\n' {
                2
            } else {
                1
            }
        }
        b'\n' => 1,
        _ => 0,
    }
}

fn build_doc_block_string(indent: &str, translated: &str, newline: &str) -> String {
    let translated_norm = translated.replace("\r\n", "\n");
    let translated_trim = translated_norm.trim_end_matches('\n');

    let mut out = String::new();
    if translated_trim.is_empty() {
        out.push_str(indent);
        out.push_str("---");
        out.push_str(newline);
        return out;
    }

    for line in translated_trim.split('\n') {
        out.push_str(indent);
        if line.is_empty() {
            out.push_str("---");
        } else {
            out.push_str("--- ");
            out.push_str(line);
        }
        out.push_str(newline);
    }

    out
}

fn to_one_line(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn fnv1a64_hex(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x00000100000001B3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use super::{
        MetaBlock, MetaEntry, MetaFile, MetaKind, apply_meta_translations,
        build_line_start_offsets, fnv1a64_hex, generate, range_to_offsets,
    };

    #[test]
    #[ignore]
    fn test_generate_translated() {
        let test_output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("emmylua_code_analysis")
            .join("resources");
        let files = generate("zh_CN", &test_output_dir);
        assert!(!files.is_empty());
    }

    #[test]
    fn test_apply_meta_translates_return_union_item() {
        let content = r#"---
--- Returns status.
--- @return
--- | "running" # Is running.
--- | "dead" # Has finished.
function coroutine.status(co) end
"#;

        let line_starts = build_line_start_offsets(content);
        let running = content.find(" Is running.").unwrap();
        let running_range = (running, running + " Is running.".len());
        let block_end = content.find("function coroutine.status").unwrap();
        let meta = MetaFile {
            v: 2,
            f: "coroutine.lua".to_string(),
            b: vec![MetaBlock {
                a: Some("function coroutine.status(co) end".to_string()),
                g: Some("Returns status.".to_string()),
                r: offsets_to_range(&line_starts, 0, block_end),
                e: vec![MetaEntry {
                    k: "coroutinelib.status.return.1.running".to_string(),
                    s: "ri:1:running".to_string(),
                    t: MetaKind::Tail,
                    r: offsets_to_range(&line_starts, running_range.0, running_range.1),
                    h: fnv1a64_hex(&content[running_range.0..running_range.1]),
                    i: None,
                    p: Some(" ".to_string()),
                }],
            }],
        };
        assert!(range_to_offsets(&line_starts, meta.b[0].e[0].r).is_some());

        let mut translations = HashMap::new();
        translations.insert(
            "coroutinelib.status.return.1.running".to_string(),
            "正在运行。".to_string(),
        );

        let translated = apply_meta_translations(content, &meta, &translations);
        assert!(translated.contains(r#"--- | "running" # 正在运行。"#));
    }

    #[test]
    fn test_apply_meta_uses_anchor_when_range_drifts() {
        let original = r#"---
--- Returns status.
--- @return
--- | "running" # Is running.
function coroutine.status(co) end
"#;
        let changed = format!("\n\n{original}");

        let line_starts = build_line_start_offsets(original);
        let running = original.find(" Is running.").unwrap();
        let running_range = (running, running + " Is running.".len());
        let block_end = original.find("function coroutine.status").unwrap();
        let meta = MetaFile {
            v: 2,
            f: "coroutine.lua".to_string(),
            b: vec![MetaBlock {
                a: Some("function coroutine.status(co) end".to_string()),
                g: Some("Returns status.".to_string()),
                r: offsets_to_range(&line_starts, 0, block_end),
                e: vec![MetaEntry {
                    k: "coroutinelib.status.return.1.running".to_string(),
                    s: "ri:1:running".to_string(),
                    t: MetaKind::Tail,
                    r: offsets_to_range(&line_starts, running_range.0, running_range.1),
                    h: fnv1a64_hex(&original[running_range.0..running_range.1]),
                    i: None,
                    p: Some(" ".to_string()),
                }],
            }],
        };

        let mut translations = HashMap::new();
        translations.insert(
            "coroutinelib.status.return.1.running".to_string(),
            "正在运行。".to_string(),
        );

        let translated = apply_meta_translations(&changed, &meta, &translations);
        assert!(translated.contains(r#"--- | "running" # 正在运行。"#));
    }

    #[test]
    fn test_apply_meta_relocates_dash_dash_doc_block() {
        let original = r#"-- Returns status.
-- @return
-- | "running" # Is running.
function coroutine.status(co) end
"#;
        let changed = format!("\n{original}");

        let line_starts = build_line_start_offsets(original);
        let running = original.find(" Is running.").unwrap();
        let running_range = (running, running + " Is running.".len());
        let block_end = original.find("function coroutine.status").unwrap();
        let meta = MetaFile {
            v: 2,
            f: "coroutine.lua".to_string(),
            b: vec![MetaBlock {
                a: Some("function coroutine.status(co) end".to_string()),
                g: Some("Returns status.".to_string()),
                r: offsets_to_range(&line_starts, 0, block_end),
                e: vec![MetaEntry {
                    k: "coroutinelib.status.return.1.running".to_string(),
                    s: "ri:1:running".to_string(),
                    t: MetaKind::Tail,
                    r: offsets_to_range(&line_starts, running_range.0, running_range.1),
                    h: fnv1a64_hex(&original[running_range.0..running_range.1]),
                    i: None,
                    p: Some(" ".to_string()),
                }],
            }],
        };

        let mut translations = HashMap::new();
        translations.insert(
            "coroutinelib.status.return.1.running".to_string(),
            "正在运行。".to_string(),
        );

        let translated = apply_meta_translations(&changed, &meta, &translations);
        assert!(translated.contains(r#"-- | "running" # 正在运行。"#));
    }

    #[test]
    fn test_parse_generated_v2_meta() {
        let file = super::STD_I18N_DIR
            .get_file("coroutine/meta.yaml")
            .expect("generated coroutine meta exists");
        let meta: MetaFile =
            serde_yml::from_str(file.contents_utf8().expect("meta is utf8")).unwrap();

        assert_eq!(meta.v, 2);
        assert!(meta.b.iter().flat_map(|block| &block.e).any(|entry| {
            entry.k == "coroutinelib.status.return.1.running"
                && entry.s == "ri:1:running"
                && matches!(entry.t, MetaKind::Tail)
        }));
    }

    #[test]
    fn test_apply_generated_meta_to_std_source() {
        let file = super::STD_I18N_DIR
            .get_file("coroutine/meta.yaml")
            .expect("generated coroutine meta exists");
        let meta: MetaFile =
            serde_yml::from_str(file.contents_utf8().expect("meta is utf8")).unwrap();
        let source = emmylua_code_analysis::load_resource_from_include_dir()
            .into_iter()
            .find(|file| file.path == "std/coroutine.lua")
            .expect("coroutine std source exists")
            .content;

        let mut translations = HashMap::new();
        translations.insert(
            "coroutinelib.status.return.1.running".to_string(),
            "正在运行。".to_string(),
        );

        let translated = apply_meta_translations(&source, &meta, &translations);
        assert!(translated.contains(r#"--- | "running" # 正在运行。"#));
    }

    fn offsets_to_range(line_starts: &[usize], start: usize, end: usize) -> [u32; 4] {
        let (start_line, start_col) = offset_to_line_col(line_starts, start);
        let (end_line, end_col) = offset_to_line_col(line_starts, end);
        [
            start_line as u32,
            start_col as u32,
            end_line as u32,
            end_col as u32,
        ]
    }

    fn offset_to_line_col(line_starts: &[usize], offset: usize) -> (usize, usize) {
        let idx = match line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line = idx.min(line_starts.len().saturating_sub(1));
        let col = offset.saturating_sub(line_starts[line]);
        (line, col)
    }
}
