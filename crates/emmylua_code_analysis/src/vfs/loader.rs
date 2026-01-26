use encoding_rs::{Encoding, UTF_8};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};
use wax::Pattern;

use log::{error, info};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct LuaFileInfo {
    pub path: String,
    pub content: String,
}

impl LuaFileInfo {
    pub fn into_tuple(self) -> (PathBuf, Option<String>) {
        (PathBuf::from(self.path), Some(self.content))
    }
}

pub fn load_workspace_files(
    root: &Path,
    include_pattern: &[String],
    exclude_pattern: &[String],
    exclude_dir: &[PathBuf],
    encoding: Option<&str>,
) -> Result<Vec<LuaFileInfo>, Box<dyn Error>> {
    let encoding = encoding.unwrap_or("utf-8");
    let mut files = Vec::new();
    if root.is_file() {
        if let Some(content) = read_file_with_encoding(root, encoding) {
            files.push(LuaFileInfo {
                path: root.to_string_lossy().to_string(),
                content,
            });
        }

        return Ok(files);
    }

    let include_pattern = include_pattern
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<&str>>();

    let include_set = match wax::any(include_pattern) {
        Ok(glob) => glob,
        Err(e) => {
            error!("Invalid glob pattern: {:?}", e);
            return Ok(files);
        }
    };

    let exclude_pattern = exclude_pattern
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<&str>>();
    let exclude_set = match wax::any(exclude_pattern) {
        Ok(glob) => glob,
        Err(e) => {
            error!("Invalid ignore glob pattern: {:?}", e);
            return Ok(files);
        }
    };

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !exclude_dir.iter().any(|dir| e.path().starts_with(dir)))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let relative_path = path.strip_prefix(root)?;
        if exclude_set.is_match(relative_path) {
            continue;
        }

        if include_set.is_match(relative_path)
            && let Some(content) = read_file_with_encoding(path, encoding)
        {
            files.push(LuaFileInfo {
                path: path.to_string_lossy().to_string(),
                content,
            });
        }
    }

    Ok(files)
}

pub fn read_file_with_encoding(path: &Path, encoding: &str) -> Option<String> {
    let origin_content = fs::read(path).ok()?;
    let encoding = Encoding::for_label(encoding.as_bytes()).unwrap_or(UTF_8);
    let (content, has_error) = encoding.decode_with_bom_removal(&origin_content);
    if has_error {
        error!("Error decoding file: {:?}", path);
        if encoding == UTF_8 {
            return None;
        }

        info!("Try utf-8 encoding");
        let (content, _, hash_error) = UTF_8.decode(&origin_content);
        if hash_error {
            error!("Try utf8 fail, error decoding file: {:?}", path);
            return None;
        }

        return Some(content.to_string());
    }

    Some(content.to_string())
}
