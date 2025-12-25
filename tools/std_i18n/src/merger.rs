use crate::model::ExtractedFile;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// 将提取到的 key 输出为 YAML（翻译文件），写入到当前工具根目录的 `locales/std` 下。
///
/// 输出路径形如：
/// - `locales/std/global/zh_CN.yaml`
/// - `locales/std/io/zh_CN.yaml`
/// - `locales/std/jit/profile/zh_CN.yaml`
///
/// 行为（尽量贴近“同步”）：
/// - 按分析顺序生成 key 列表
/// - 若目标 YAML 已存在，则保留已有翻译；新增 key 的值为空串
/// - 若目标 YAML 不存在，则生成仅含 key 的空值模板
pub fn write_std_locales_yaml(
    std_dir: &Path,
    locale: &str,
    out_root: &Path,
    full_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // `full_output=false` 时，extractor 会过滤掉不包含原文（value 为空）的条目。
    let files = crate::extractor::extract_std_dir(std_dir, full_output)?;
    for file in files {
        write_one_file(out_root, &file, locale)?;
    }

    Ok(())
}

fn write_one_file(
    out_root: &Path,
    file: &ExtractedFile,
    locale: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 输出目录：去掉 `.lua` 扩展名作为目录名（支持子目录）
    let mut dir_rel = file.path.clone();
    if dir_rel.extension().and_then(|e| e.to_str()) == Some("lua") {
        dir_rel.set_extension("");
    }
    let out_dir = out_root.join(&dir_rel);
    fs::create_dir_all(&out_dir)?;
    let out_file = out_dir.join(format!("{locale}.yaml"));

    let existing = read_yaml_string_map(&out_file).unwrap_or_default();

    let mut ordered = Vec::<YamlOutEntry>::new();
    let mut seen = HashSet::<String>::new();

    for entry in &file.entries {
        let yaml_key = entry.locale_key.clone();
        if !seen.insert(yaml_key.clone()) {
            continue;
        }
        let translated = existing.get(&yaml_key).cloned().unwrap_or_default();
        ordered.push(YamlOutEntry {
            key: yaml_key,
            translated,
            origin: entry.value.clone(),
        });
    }

    write_yaml_string_map_in_order(&out_file, &ordered)?;
    Ok(())
}

#[allow(clippy::type_complexity)]
fn read_yaml_string_map(
    path: &Path,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(path)?;
    let map: HashMap<String, String> = serde_yml::from_str(&raw)?;
    Ok(map)
}

#[derive(Debug, Clone)]
struct YamlOutEntry {
    key: String,
    translated: String,
    origin: String,
}

fn write_yaml_string_map_in_order(path: &Path, entries: &[YamlOutEntry]) -> std::io::Result<()> {
    let mut out = String::new();
    for entry in entries {
        let key = yaml_escape_key(&entry.key);

        // 同时输出原始英文文本，便于翻译时对照。
        // 仅在尚未翻译时输出（避免污染已维护的翻译文件）。
        if entry.translated.is_empty() && !entry.origin.trim().is_empty() {
            let normalized = entry.origin.replace("\r\n", "\n");
            for line in normalized.lines() {
                out.push_str("# ");
                out.push_str(line);
                out.push('\n');
            }
        }

        if entry.translated.is_empty() {
            out.push_str(&format!("{key}: \"\"\n\n"));
            continue;
        }

        out.push_str(&format!("{key}: |\n"));
        let normalized = entry.translated.replace("\r\n", "\n");
        for line in normalized.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    fs::write(path, out)
}

fn yaml_escape_key(key: &str) -> String {
    // 绝大多数 key（如 `iolib.open` / `std.readmode.item.n`）可以直接写为 plain scalar。
    // 为稳妥起见，碰到空白、冒号等特殊字符时用单引号包裹。
    let needs_quote = key.is_empty()
        || key.chars().any(|c| c.is_whitespace())
        || key.contains(':')
        || key.starts_with(['*', '?', '-', '!', '&'])
        || key.contains('#');
    if !needs_quote {
        return key.to_string();
    }
    let escaped = key.replace('\'', "''");
    format!("'{escaped}'")
}
