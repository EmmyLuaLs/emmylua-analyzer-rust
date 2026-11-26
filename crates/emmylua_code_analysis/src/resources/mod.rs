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
    // Resource output dir specified; currently only the LSP sets it.
    if let Some(create_resources_dir) = create_resources_dir {
        let resource_path = if create_resources_dir.is_empty() {
            get_best_resources_dir()
        } else {
            PathBuf::from(&create_resources_dir)
        };
        let result = load_resource_from_file_system(&resource_path);
        if let Some(mut files) = result {
            // i18n resources exist; switch to the std dir for the current locale.
            let std_dir = get_std_dir(&resource_path);
            if !is_jit {
                remove_jit_resource(&mut files);
            }
            return (std_dir, files);
        }
    }
    // No resource dir specified; use the default dir, which has no i18n.
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
        "bit.lua",
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
    if cfg!(debug_assertions) || !stamp_is_current(resources_dir) {
        dump_resources(resources_dir)?;
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

/// What the `version` file of an unpacked resources dir holds: the crate version plus a hash of the embedded
/// files. The version alone stays the same across builds (every build between two releases), so a std fix
/// would never reach an unpacked copy.
fn resources_stamp() -> String {
    // FNV-1a over each embedded file's path and contents, in include_dir's (compile-time) order.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for file in load_resource_from_include_dir() {
        for byte in file
            .path
            .bytes()
            .chain([0])
            .chain(file.content.bytes())
            .chain([0])
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{VERSION}+{hash:016x}")
}

/// Whether `resources_dir` holds exactly the embedded resources: its own `version` file has `resources_stamp`.
fn stamp_is_current(resources_dir: &Path) -> bool {
    std::fs::read_to_string(resources_dir.join("version"))
        .is_ok_and(|content| content.trim() == resources_stamp())
}

/// Unpacks the embedded resources into `resources_dir`, first removing the top-level trees it unpacks
/// (`std`, `std-zh_CN`, …) so a file the build no longer has does not linger, then writes the stamp.
fn dump_resources(resources_dir: &Path) -> Option<()> {
    log::info!("Creating resources dir: {:?}", resources_dir);
    for entry in RESOURCE_DIR.entries() {
        if let DirEntry::Dir(dir) = entry {
            let old = resources_dir.join(dir.path());
            if let Err(e) = std::fs::remove_dir_all(&old)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                log::error!("Failed to remove dir: {:?}, {:?}", old, e);
                return None;
            }
        }
    }
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
    match std::fs::write(&version_path, resources_stamp()) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to write file: {:?}, {:?}", version_path, e);
            return None;
        }
    }
    Some(())
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

// Prefer the std-{locale} directory for the current locale; fall back to the default std dir.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch dir under the system temp dir, removed first if a previous run left it.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("emmylua-resources-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // A dir unpacked by another build of the same crate version is stale: every build between two releases
    // says the same version, so a std fix never reached the copy emmylua_ls reads (fork #93).
    #[test]
    fn test_a_copy_stamped_with_the_bare_version_is_not_current() {
        let dir = scratch("bare");
        std::fs::write(dir.join("version"), VERSION).unwrap();
        assert!(!stamp_is_current(&dir));
        assert!(resources_stamp().starts_with(&format!("{VERSION}+")));
    }

    #[test]
    fn test_unpacking_replaces_stale_files_removes_gone_ones_and_stamps() {
        let dir = scratch("dump");
        std::fs::create_dir_all(dir.join("std")).unwrap();
        std::fs::write(dir.join("std/io.lua"), "-- stale").unwrap();
        std::fs::write(dir.join("std/gone.lua"), "-- not in this build").unwrap();
        std::fs::write(dir.join("version"), VERSION).unwrap();

        dump_resources(&dir).unwrap();

        let io = load_resource_from_include_dir()
            .into_iter()
            .find(|f| f.path == "std/io.lua")
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("std/io.lua")).unwrap(),
            io.content
        );
        assert!(!dir.join("std/gone.lua").exists());
        assert!(stamp_is_current(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
