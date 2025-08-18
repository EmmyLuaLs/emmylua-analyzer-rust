use emmylua_code_analysis::update_code_style;
use std::path::PathBuf;
use walkdir::{DirEntry, WalkDir};

const VCS_DIRS: [&str; 3] = [".git", ".hg", ".svn"];

pub fn load_editorconfig(workspace_folders: Vec<PathBuf>) -> Option<()> {
    let mut editorconfig_files = Vec::new();

    for workspace in workspace_folders {
        // 构建 WalkDir 迭代器，递归遍历工作区目录
        let walker = WalkDir::new(&workspace)
            .into_iter()
            .filter_entry(|e| !is_vcs_dir(e, &VCS_DIRS))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file());
        for entry in walker {
            let path = entry.path();
            if path.ends_with(".editorconfig") {
                editorconfig_files.push(path.to_path_buf());
            }
        }
    }

    if editorconfig_files.is_empty() {
        return None;
    }

    log::info!("found editorconfig files: {:?}", editorconfig_files);
    for file in editorconfig_files {
        let parent_dir = file
            .parent()
            .unwrap()
            .to_path_buf()
            .to_string_lossy()
            .to_string()
            .replace("\\", "/");
        let file_normalized = file.to_string_lossy().to_string().replace("\\", "/");
        update_code_style(&parent_dir, &file_normalized);
    }
    log::info!("loaded editorconfig complete");

    Some(())
}

/// 判断目录条目是否应该被包含在遍历中（不被过滤）
fn is_vcs_dir(entry: &DirEntry, vcs_dirs: &[&str]) -> bool {
    if entry.file_type().is_dir() {
        let name = entry.file_name().to_string_lossy();
        // 如果是 VCS 目录，则不包含（返回 false）
        vcs_dirs.iter().any(|&vcs| vcs == name)
    } else {
        // 如果是文件，则包含（返回 true）
        false
    }
}
