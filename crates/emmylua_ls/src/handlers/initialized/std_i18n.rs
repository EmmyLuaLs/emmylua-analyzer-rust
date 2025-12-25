use std::collections::HashMap;
use std::path::{Path, PathBuf};

use emmylua_code_analysis::{LuaFileInfo, get_best_resources_dir, get_locale_code};
use include_dir::{Dir, include_dir};

static STD_I18N_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/std_i18n");

#[derive(Debug, Clone, serde::Deserialize)]
struct MetaFile {
    version: u32,
    line_base: u32,
    col_base: u32,
    #[allow(dead_code)]
    file: String,
    entries: Vec<MetaEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MetaEntry {
    key: String,
    kind: MetaKind,
    range: MetaRange,
    hash: String,
    context_hash: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MetaKind {
    DocBlock { indent: String },
    LineTail { prefix: String },
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MetaRange {
    start: MetaPos,
    end: MetaPos,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
struct MetaPos {
    line: u32,
    col: u32,
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
    if meta.version != 1 || meta.line_base != 0 || meta.col_base != 0 {
        return content.to_string();
    }

    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let line_starts = build_line_start_offsets(content);

    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for entry in &meta.entries {
        let Some(translated) = translations
            .get(&entry.key)
            .map(|s| s.to_string())
            .filter(|t| !t.trim().is_empty())
        else {
            continue;
        };

        let start = match pos_to_offset(&line_starts, entry.range.start) {
            Some(o) => o,
            None => continue,
        };
        let end = match pos_to_offset(&line_starts, entry.range.end) {
            Some(o) => o,
            None => continue,
        };
        if start > end || end > content.len() {
            continue;
        }

        let slice = content.get(start..end).unwrap_or("");
        if fnv1a64_hex(slice) != entry.hash {
            continue;
        }

        let context_line = line_slice_at_offset(content, &line_starts, start);
        if fnv1a64_hex(context_line) != entry.context_hash {
            continue;
        }

        let rep = match &entry.kind {
            MetaKind::DocBlock { indent } => {
                let mut rep = build_doc_block_string(indent, &translated, newline);
                if line_break_len_at(content, end) > 0 && rep.ends_with(newline) {
                    rep.truncate(rep.len().saturating_sub(newline.len()));
                }
                rep
            }
            MetaKind::LineTail { prefix } => {
                let one_line = to_one_line(&translated);
                format!("{prefix}{one_line}")
            }
        };

        replacements.push((start, end, rep));
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

fn pos_to_offset(line_starts: &[usize], pos: MetaPos) -> Option<usize> {
    let line = pos.line as usize;
    let col = pos.col as usize;
    let line_start = *line_starts.get(line)?;
    Some(line_start.saturating_add(col))
}

fn line_slice_at_offset<'a>(s: &'a str, line_starts: &[usize], offset: usize) -> &'a str {
    if s.is_empty() {
        return "";
    }
    let offset = offset.min(s.len());

    // upper_bound(line_starts, offset) - 1
    let idx = match line_starts.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let line = idx.min(line_starts.len().saturating_sub(1));

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
    use std::path::Path;

    use super::generate;

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
}
