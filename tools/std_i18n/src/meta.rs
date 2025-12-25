use crate::translator::{ReplaceStrategy, compute_replace_targets};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
pub struct MetaFile {
    pub version: u32,
    pub line_base: u32,
    pub col_base: u32,
    pub file: String,
    pub entries: Vec<MetaEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetaEntry {
    pub key: String,
    pub kind: MetaKind,
    pub range: MetaRange,
    pub hash: String,
    pub context_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MetaKind {
    DocBlock { indent: String },
    LineTail { prefix: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct MetaRange {
    pub start: MetaPos,
    pub end: MetaPos,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MetaPos {
    pub line: u32,
    pub col: u32,
}

/// 基于 std 源文件生成 `meta.yaml`（一次生成，供运行时做快速替换）。
///
/// 输出路径形如：
/// - `<out_root>/global/meta.yaml`
/// - `<out_root>/jit/profile/meta.yaml`
///
/// `out_root` 目录结构与 `write_std_locales_yaml` 一致：以去掉 `.lua` 扩展名后的相对路径作为目录。
pub fn write_std_meta_yaml(
    std_dir: &Path,
    out_root: &Path,
    full_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut files: Vec<PathBuf> = WalkDir::new(std_dir)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lua"))
        .collect();

    files.sort();

    for full_path in files {
        let rel_path = full_path.strip_prefix(std_dir)?.to_path_buf();
        let mut dir_rel = rel_path.clone();
        if dir_rel.extension().and_then(|e| e.to_str()) == Some("lua") {
            dir_rel.set_extension("");
        }

        let file_name = rel_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let content = fs::read_to_string(&full_path)?;

        let targets = compute_replace_targets(&content, &file_name, full_output);
        let line_starts = build_line_start_offsets(&content);

        let mut entries: Vec<MetaEntry> = Vec::with_capacity(targets.len());
        for t in targets {
            let kind = match &t.strategy {
                ReplaceStrategy::DocBlock { indent } => MetaKind::DocBlock {
                    indent: indent.clone(),
                },
                ReplaceStrategy::LineCommentTail { prefix } => MetaKind::LineTail {
                    prefix: prefix.clone(),
                },
            };

            let (start_line, start_col) = offset_to_line_col(&line_starts, t.start);
            let (end_line, end_col) = offset_to_line_col(&line_starts, t.end);

            let replaced_slice = content.get(t.start..t.end).unwrap_or("");
            let hash = fnv1a64_hex(replaced_slice);

            let context_line = line_slice_at_offset(&content, &line_starts, t.start);
            let context_hash = fnv1a64_hex(context_line);

            entries.push(MetaEntry {
                key: t.key,
                kind,
                range: MetaRange {
                    start: MetaPos {
                        line: start_line as u32,
                        col: start_col as u32,
                    },
                    end: MetaPos {
                        line: end_line as u32,
                        col: end_col as u32,
                    },
                },
                hash,
                context_hash,
            });
        }

        let out_dir = out_root.join(&dir_rel);
        fs::create_dir_all(&out_dir)?;
        let meta_path = out_dir.join("meta.yaml");

        let meta = MetaFile {
            version: 1,
            line_base: 0,
            col_base: 0,
            file: rel_path.to_string_lossy().replace('\\', "/"),
            entries,
        };

        let yaml = serde_yml::to_string(&meta)?;
        fs::write(meta_path, yaml)?;
    }

    Ok(())
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

fn offset_to_line_col(line_starts: &[usize], offset: usize) -> (usize, usize) {
    // upper_bound(line_starts, offset) - 1
    let idx = match line_starts.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let line = idx.min(line_starts.len().saturating_sub(1));
    let col = offset.saturating_sub(line_starts[line]);
    (line, col)
}

fn line_slice_at_offset<'a>(s: &'a str, line_starts: &[usize], offset: usize) -> &'a str {
    if s.is_empty() {
        return "";
    }
    let (line, _) = offset_to_line_col(line_starts, offset.min(s.len()));
    let line_start = *line_starts.get(line).unwrap_or(&0);
    let next_start = line_starts.get(line + 1).copied().unwrap_or(s.len());
    let mut line_end = next_start;
    if line_end > line_start && s.as_bytes().get(line_end - 1) == Some(&b'\n') {
        line_end -= 1;
        if line_end > line_start && s.as_bytes().get(line_end - 1) == Some(&b'\r') {
            line_end -= 1;
        }
    }
    s.get(line_start..line_end).unwrap_or("")
}

fn fnv1a64_hex(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x00000100000001B3);
    }
    format!("{hash:016x}")
}
