mod best_resource_path;

use std::path::{Path, PathBuf};

use best_resource_path::get_best_resources_dir;
use include_dir::{include_dir, Dir, DirEntry};

use crate::{load_workspace_files, LuaFileInfo};

static RESOURCE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/resources");
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn load_resource_std(
    create_resources_dir: Option<String>,
    is_jit: bool,
) -> (PathBuf, Vec<LuaFileInfo>) {
    if let Some(create_resources_dir) = create_resources_dir {
        let resource_path = if create_resources_dir.is_empty() {
            get_best_resources_dir()
        } else {
            PathBuf::from(&create_resources_dir)
        };
        let std_dir = PathBuf::from(&resource_path).join("std");
        let result = load_resource_from_file_system(&resource_path);
        match result {
            Some(mut files) => {
                if !is_jit {
                    remove_jit_resource(&mut files);
                }
                return (std_dir, files);
            }
            None => {}
        }
    }

    let resoucres_dir = get_best_resources_dir();
    let std_dir = resoucres_dir.join("std");
    let files = load_resource_from_include_dir();
    let mut files = files
        .into_iter()
        .filter_map(|file| {
            if file.path.ends_with(".lua") {
                let path = std_dir.join(&file.path).to_str().unwrap().to_string();
                Some(LuaFileInfo {
                    path,
                    content: file.content,
                })
            } else {
                None
            }
        })
        .collect::<_>();
    if !is_jit {
        remove_jit_resource(&mut files);
    }
    (std_dir, files)
}

fn remove_jit_resource(files: &mut Vec<LuaFileInfo>) {
    files.retain(|file| {
        let path = Path::new(&file.path);
        let should_remove = path.ends_with("std/jit.lua")
            || path.ends_with("std/jit/profile.lua")
            || path.ends_with("std/jit/util.lua")
            || path.ends_with("std/string/buffer.lua")
            || path.ends_with("std/table/clear.lua")
            || path.ends_with("std/table/new.lua");

        !should_remove
    });
}

fn load_resource_from_file_system(resources_dir: &Path) -> Option<Vec<LuaFileInfo>> {
    if check_need_dump_to_file_system() {
        log::info!("Creating resources dir: {:?}", resources_dir);
        let files = load_resource_from_include_dir();
        for file in &files {
            let path = resources_dir.join(&file.path);
            let parent = path.parent().unwrap();
            if !parent.exists() {
                match std::fs::create_dir_all(parent) {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Failed to create dir: {:?}, {:?}", parent, e);
                        return None;
                    }
                }
            }

            match std::fs::write(&path, &file.content) {
                Ok(_) => {}
                Err(e) => {
                    log::error!("Failed to write file: {:?}, {:?}", path, e);
                    return None;
                }
            }
        }

        let version_path = resources_dir.join("version");
        let content = format!("{}", VERSION);
        match std::fs::write(&version_path, content) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to write file: {:?}, {:?}", version_path, e);
                return None;
            }
        }
    }

    let std_dir = resources_dir.join("std");
    let match_pattern = vec!["**/*.lua".to_string()];
    let files = match load_workspace_files(&std_dir, &match_pattern, &Vec::new(), &Vec::new(), None)
    {
        Ok(files) => files,
        Err(e) => {
            log::error!("Failed to load std lib: {:?}", e);
            vec![]
        }
    };

    return Some(files);
}

fn check_need_dump_to_file_system() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }

    let resoucres_dir = get_best_resources_dir();
    let version_path = resoucres_dir.join("version");

    if !version_path.exists() {
        return true;
    }

    let content = std::fs::read_to_string(&version_path).unwrap();
    let version = content.trim();
    if version != VERSION {
        return true;
    }

    false
}

fn load_resource_from_include_dir() -> Vec<LuaFileInfo> {
    let mut files = Vec::new();
    walk_resource_dir(&RESOURCE_DIR, &mut files);
    files
}

fn walk_resource_dir(dir: &Dir, files: &mut Vec<LuaFileInfo>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(file) => {
                let path = file.path();
                let content = file.contents_utf8().unwrap();

                files.push(LuaFileInfo {
                    path: path.to_str().unwrap().to_string(),
                    content: content.to_string(),
                });
            }
            DirEntry::Dir(subdir) => {
                walk_resource_dir(subdir, files);
            }
        }
    }
}
