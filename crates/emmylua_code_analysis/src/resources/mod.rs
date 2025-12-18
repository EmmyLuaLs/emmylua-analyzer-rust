mod best_resource_path;

use std::path::{Path, PathBuf};

pub use best_resource_path::get_best_resources_dir;
use include_dir::{Dir, DirEntry, include_dir};

use crate::{LuaFileInfo, get_locale_code, load_workspace_files};

static RESOURCE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/resources");
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn load_resource_std(
    create_resources_dir: Option<String>,
    is_jit: bool,
) -> (PathBuf, Vec<LuaFileInfo>) {
    // 指定了输出的资源目录, 目前只有 lsp 会指定
    if let Some(create_resources_dir) = create_resources_dir {
        let resource_path = if create_resources_dir.is_empty() {
            get_best_resources_dir()
        } else {
            PathBuf::from(&create_resources_dir)
        };
        // 此时会存在 i18n, 我们需要根据当前语言环境切换到对应语言的 std 目录
        let std_dir = get_std_dir(&resource_path);
        let result = load_resource_from_file_system(&resource_path);
        if let Some(mut files) = result {
            if !is_jit {
                remove_jit_resource(&mut files);
            }
            return (std_dir, files);
        }
    }
    // 没有指定资源目录, 那么直接使用默认的资源目录, 此时不会存在 i18n
    let resoucres_dir = get_best_resources_dir();
    let std_dir = resoucres_dir.join("std");
    let files = load_resource_from_include_dir();
    let mut files = files
        .into_iter()
        .filter_map(|file| {
            if file.path.ends_with(".lua") {
                let path = resoucres_dir
                    .join(&file.path)
                    .to_str()
                    .expect("UTF-8 paths")
                    .to_string();
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
    const JIT_FILES_TO_REMOVE: &[&str] = &[
        "jit.lua",
        "jit/profile.lua",
        "jit/util.lua",
        "string/buffer.lua",
        "table/clear.lua",
        "table/new.lua",
        "ffi.lua",
    ];
    files.retain(|file| {
        let path = Path::new(&file.path);
        !JIT_FILES_TO_REMOVE
            .iter()
            .any(|suffix| path.ends_with(suffix))
    });
}

fn load_resource_from_file_system(resources_dir: &Path) -> Option<Vec<LuaFileInfo>> {
    // lsp i18n 的资源在更早之前的 crates\emmylua_ls\src\handlers\initialized\std_i18n.rs 中写入到文件系统
    if check_need_dump_to_file_system() {
        log::info!("Creating resources dir: {:?}", resources_dir);
        let files = load_resource_from_include_dir();
        for file in &files {
            let path = resources_dir.join(&file.path);
            let parent = path.parent()?;
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
        let content = VERSION.to_string();
        match std::fs::write(&version_path, content) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to write file: {:?}, {:?}", version_path, e);
                return None;
            }
        }
    }

    let std_dir = get_std_dir(&resources_dir);
    let match_pattern = vec!["**/*.lua".to_string()];
    let files = match load_workspace_files(&std_dir, &match_pattern, &Vec::new(), &Vec::new(), None)
    {
        Ok(files) => files,
        Err(e) => {
            log::error!("Failed to load std lib: {:?}", e);
            vec![]
        }
    };

    Some(files)
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

    let Ok(content) = std::fs::read_to_string(&version_path) else {
        return true;
    };
    let version = content.trim();
    if version != VERSION {
        return true;
    }

    false
}

pub fn load_resource_from_include_dir() -> Vec<LuaFileInfo> {
    let mut files = Vec::new();
    walk_resource_dir(&RESOURCE_DIR, &mut files);
    files
}

fn walk_resource_dir(dir: &Dir, files: &mut Vec<LuaFileInfo>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(file) => {
                let path = file.path();
                let content = file.contents_utf8().expect("UTF-8 paths");

                files.push(LuaFileInfo {
                    path: path.to_str().expect("UTF-8 paths").to_string(),
                    content: content.to_string(),
                });
            }
            DirEntry::Dir(subdir) => {
                walk_resource_dir(subdir, files);
            }
        }
    }
}

// 优先使用当前语言环境的 std-{locale} 目录, 否则回退到默认的 std 目录
fn get_std_dir(resources_dir: &Path) -> PathBuf {
    let locale = get_locale_code(&rust_i18n::locale());
    if locale != "en" {
        let locale_dir = resources_dir.join(format!("std-{locale}"));
        if locale_dir.exists() {
            return locale_dir;
        }
    }
    resources_dir.join("std")
}
