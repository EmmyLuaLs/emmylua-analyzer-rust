use std::error::Error;
use std::fs;
use std::path::Path;

use crate::apply::translate_std_file;
use crate::fs::collect_std_lua_files;

/// 根据 `data_dir` 中的 meta 与翻译, 生成 `out_dir` 下的目标语言 std 资源.
///
/// 输出目录会被整体重建, 以保证与 `std_dir` 的文件集合一致.
pub fn generate_translated_std(
    std_dir: &Path,
    data_dir: &Path,
    out_dir: &Path,
    locale: &str,
) -> Result<(), Box<dyn Error>> {
    if out_dir.exists() {
        fs::remove_dir_all(out_dir)?;
    }

    for (rel_path, full_path) in collect_std_lua_files(std_dir)? {
        // 统一换行为 LF, 保证产物与检出环境无关.
        let content = fs::read_to_string(&full_path)?.replace("\r\n", "\n");
        let translated =
            translate_std_file(data_dir, locale, &rel_path, &content).unwrap_or(content);

        let out_path = out_dir.join(&rel_path);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out_path, translated)?;
    }

    Ok(())
}
