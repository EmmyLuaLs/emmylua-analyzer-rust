use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub type StdLuaFilePath = (PathBuf, PathBuf);

pub fn collect_std_lua_files(
    std_dir: &Path,
) -> Result<Vec<StdLuaFilePath>, Box<dyn std::error::Error>> {
    let mut files: Vec<StdLuaFilePath> = WalkDir::new(std_dir)
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
    Ok(files)
}

pub fn dir_for_lua_path(rel_path: &Path) -> PathBuf {
    let mut dir_rel = rel_path.to_path_buf();
    if dir_rel.extension().and_then(|e| e.to_str()) == Some("lua") {
        dir_rel.set_extension("");
    }
    dir_rel
}

pub fn path_label(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
