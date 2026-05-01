use crate::doc_text::DocBlockView;
use crate::fs::{dir_for_lua_path, path_label};
use crate::model::{ReplaceStrategy, SourceSpan};
use crate::pipeline::StdI18nProject;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MetaFile {
    pub v: u32,
    pub f: String,
    pub b: Vec<MetaBlock>,
}

#[derive(Debug, Clone)]
pub struct MetaBlock {
    /// Best-effort stable anchor line, usually the owner statement after a doc block.
    pub a: Option<String>,
    /// First meaningful line inside the doc block, used when the owner anchor is absent or ambiguous.
    pub g: Option<String>,
    /// Comment block range hint: [start_line, start_col, end_line, end_col].
    pub r: [u32; 4],
    pub e: Vec<MetaEntry>,
}

#[derive(Debug, Clone)]
pub struct MetaEntry {
    pub k: String,
    pub s: String,
    pub t: MetaKind,
    /// Replacement range hint: [start_line, start_col, end_line, end_col].
    pub r: [u32; 4],
    /// Single guard hash for the original replacement slice.
    pub h: String,
    pub i: Option<String>,
    pub p: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum MetaKind {
    Doc,
    Tail,
}

/// 基于 std 源文件生成 `meta.yaml`（一次生成，供运行时做快速替换）。
///
/// 输出路径形如：
/// - `<out_root>/global/meta.yaml`
/// - `<out_root>/jit/profile/meta.yaml`
///
/// `out_root` 目录结构与 locale YAML 一致：以去掉 `.lua` 扩展名后的相对路径作为目录。
pub fn write_std_meta_yaml(
    project: &StdI18nProject,
    out_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    for file in &project.files {
        let mut grouped: BTreeMap<(usize, usize), MetaBlockBuild> = BTreeMap::new();
        for t in &file.targets {
            let block_key = (t.comment_span.start, t.comment_span.end);
            let block_range =
                range_to_array(&file.line_starts, t.comment_span.start, t.comment_span.end);
            let anchor = owner_anchor_after_comment(&file.content, t.comment_span);
            let guard = block_anchor_line(&file.content, t.comment_span);

            let block = grouped.entry(block_key).or_insert_with(|| MetaBlockBuild {
                anchor,
                guard,
                range: block_range,
                entries: Vec::new(),
            });

            let replaced_slice = file.content.get(t.start..t.end).unwrap_or("");
            let (kind, indent, prefix) = match &t.strategy {
                ReplaceStrategy::DocBlock { indent } => (MetaKind::Doc, Some(indent), None),
                ReplaceStrategy::LineCommentTail { prefix } => (MetaKind::Tail, None, Some(prefix)),
            };

            block.entries.push(MetaEntry {
                k: t.locale_key.clone(),
                s: t.selector.encode(),
                t: kind,
                r: range_to_array(&file.line_starts, t.start, t.end),
                h: fnv1a64_hex(replaced_slice),
                i: indent.filter(|s| !s.is_empty()).cloned(),
                p: prefix.filter(|s| !s.is_empty()).cloned(),
            });
        }

        let blocks = grouped
            .into_values()
            .map(|b| MetaBlock {
                a: b.anchor,
                g: b.guard,
                r: b.range,
                e: b.entries,
            })
            .collect();

        let out_dir = out_root.join(dir_for_lua_path(&file.path));
        fs::create_dir_all(&out_dir)?;
        let meta_path = out_dir.join("meta.yaml");

        let meta = MetaFile {
            v: 2,
            f: path_label(&file.path),
            b: blocks,
        };

        let yaml = write_meta_yaml(&meta);
        fs::write(meta_path, yaml)?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct MetaBlockBuild {
    anchor: Option<String>,
    guard: Option<String>,
    range: [u32; 4],
    entries: Vec<MetaEntry>,
}

fn range_to_array(line_starts: &[usize], start: usize, end: usize) -> [u32; 4] {
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

fn owner_anchor_after_comment(content: &str, comment_span: SourceSpan) -> Option<String> {
    let mut offset = comment_span.end;
    while offset < content.len() {
        let line_end = find_line_end(content, offset);
        let line = content.get(offset..line_end)?.trim();
        if !line.is_empty() {
            return Some(line.to_string());
        }
        offset = advance_past_line_break(content, line_end);
    }
    None
}

fn block_anchor_line(content: &str, comment_span: SourceSpan) -> Option<String> {
    let raw = content.get(comment_span.start..comment_span.end)?;
    DocBlockView::new(raw).first_guard_text()
}

fn write_meta_yaml(meta: &MetaFile) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "v: {}", meta.v);
    let _ = writeln!(out, "f: {}", yaml_string(&meta.f));
    out.push_str("b:\n");
    for block in &meta.b {
        out.push('-');
        if let Some(anchor) = &block.a {
            let _ = writeln!(out, " a: {}", yaml_string(anchor));
        } else {
            out.push('\n');
        }
        if let Some(guard) = &block.g {
            let _ = writeln!(out, "  g: {}", yaml_string(guard));
        }
        let _ = writeln!(out, "  r: {}", yaml_range(block.r));
        out.push_str("  e:\n");
        for entry in &block.e {
            let _ = write!(
                out,
                "  - [{}, {}, {}, {}, {}",
                yaml_string(&entry.k),
                yaml_string(&entry.s),
                match entry.t {
                    MetaKind::Doc => "doc",
                    MetaKind::Tail => "tail",
                },
                yaml_range(entry.r),
                yaml_string(&entry.h)
            );
            let affix = match entry.t {
                MetaKind::Doc => entry.i.as_ref(),
                MetaKind::Tail => entry.p.as_ref(),
            };
            if let Some(affix) = affix {
                let _ = write!(out, ", {}", yaml_string(affix));
            }
            out.push_str("]\n");
        }
    }
    out
}

fn yaml_range(range: [u32; 4]) -> String {
    format!("[{}, {}, {}, {}]", range[0], range[1], range[2], range[3])
}

fn yaml_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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

fn fnv1a64_hex(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x00000100000001B3);
    }
    format!("{hash:016x}")
}
