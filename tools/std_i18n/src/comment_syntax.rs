use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct LineInfo {
    pub start: usize,
    pub end: usize, // 不含换行（也不含 CR）
}

impl LineInfo {
    pub fn text<'a>(&self, raw: &'a str) -> &'a str {
        raw.get(self.start..self.end).unwrap_or("")
    }

    pub fn indent(&self, raw: &str) -> String {
        self.text(raw)
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect()
    }

    pub fn trim_start_text<'a>(&self, raw: &'a str) -> &'a str {
        self.text(raw).trim_start()
    }
}

pub fn split_lines_with_offsets(s: &str) -> Vec<LineInfo> {
    let bytes = s.as_bytes();
    let mut out: Vec<LineInfo> = Vec::new();

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

pub fn normalize_optional_name(s: &str) -> String {
    s.trim()
        .trim_end_matches('?')
        .trim_end_matches(',')
        .to_string()
}

pub fn normalize_field_key_token(token: &str) -> String {
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

pub fn parse_param_name_from_line(trimmed: &str) -> Option<String> {
    let after = doc_tag_payload(trimmed, "@param")?;
    let name = after.split_whitespace().next()?;
    Some(normalize_optional_name(name))
}

pub fn parse_field_name_from_line(trimmed: &str) -> Option<String> {
    let after = doc_tag_payload(trimmed, "@field")?;
    let token = after.split_whitespace().next()?;
    Some(normalize_optional_name(&normalize_field_key_token(token)))
}

pub fn is_doc_tag_line(line_trim_start: &str) -> bool {
    let t = line_trim_start.trim_start();
    let Some(after) = t.strip_prefix("---") else {
        return false;
    };
    let after = after.trim_start();
    after.starts_with('@')
}

pub fn find_desc_block_line_range(raw: &str, lines: &[LineInfo]) -> Option<(usize, usize)> {
    let mut start_idx: Option<usize> = None;
    for (i, li) in lines.iter().enumerate() {
        let t = li.trim_start_text(raw);
        if is_doc_tag_line(t) || t.starts_with("---|") {
            continue;
        }
        if t.starts_with("---") {
            start_idx = Some(i);
            break;
        }
    }
    let start = start_idx?;

    let mut end = start;
    while end < lines.len() {
        let t = lines[end].trim_start_text(raw);
        if is_doc_tag_line(t) || t.starts_with("---|") {
            break;
        }
        if t.starts_with("---") {
            end += 1;
            continue;
        }
        break;
    }
    Some((start, end))
}

/// 解析 union item 行的 value 部分。
///
/// 支持：
/// - `---| "n"   # ...`
/// - `---|>"collect" # ...`
/// - `---|+"n" # ...`
/// - `---|>+"n" # ...`
pub fn parse_union_item_value_from_line_trim(line_trim_start: &str) -> Option<String> {
    let after = line_trim_start.strip_prefix("---|")?.trim_start();
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

pub fn build_tag_line_indexes(raw: &str, lines: &[LineInfo]) -> TagLineIndexes {
    let default_indent = lines.first().map(|l| l.indent(raw)).unwrap_or_default();

    let desc_block = find_desc_block_line_range(raw, lines);

    let mut param_line: HashMap<String, usize> = HashMap::new();
    let mut field_line: HashMap<String, usize> = HashMap::new();
    let mut return_lines: Vec<usize> = Vec::new();
    let mut union_line: HashMap<String, usize> = HashMap::new();

    for (i, li) in lines.iter().enumerate() {
        let t = li.trim_start_text(raw);

        if let Some(name) = parse_param_name_from_line(t) {
            param_line.entry(name).or_insert(i);
            continue;
        }
        if let Some(name) = parse_field_name_from_line(t) {
            field_line.entry(name).or_insert(i);
            continue;
        }
        if doc_tag_payload(t, "@return").is_some() {
            return_lines.push(i);
            continue;
        }
        if t.starts_with("---|")
            && let Some(value) = parse_union_item_value_from_line_trim(t)
        {
            union_line.entry(value).or_insert(i);
        }
    }

    TagLineIndexes {
        default_indent,
        desc_block,
        param_line,
        field_line,
        return_lines,
        union_line,
    }
}

pub struct TagLineIndexes {
    pub default_indent: String,
    pub desc_block: Option<(usize, usize)>,
    pub param_line: HashMap<String, usize>,
    pub field_line: HashMap<String, usize>,
    pub return_lines: Vec<usize>,
    pub union_line: HashMap<String, usize>,
}

fn doc_tag_payload<'a>(line_trim_start: &'a str, tag: &str) -> Option<&'a str> {
    // 支持 `---@param ...` 以及 `--- @param ...`（中间允许空格）。
    let t = line_trim_start.trim_start();
    let after = t.strip_prefix("---")?.trim_start();
    let after = after.strip_prefix(tag)?;
    Some(after.trim_start())
}
