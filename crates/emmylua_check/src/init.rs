use std::{path::PathBuf, sync::Arc, str::FromStr};

use emmylua_code_analysis::{
    load_configs, load_workspace_files, update_code_style, EmmyLuaAnalysis, Emmyrc, LuaFileInfo,
};

#[allow(unused)]
pub fn load_workspace(
    workspace_folder: PathBuf,
    config_path: Option<PathBuf>,
    ignore: Option<Vec<String>>,
) -> Option<EmmyLuaAnalysis> {
    let mut analysis = EmmyLuaAnalysis::new();
    analysis.init_std_lib(false);

    let mut workspace_folders = vec![workspace_folder];
    for path in &workspace_folders {
        analysis.add_main_workspace(path.clone());
    }

    let main_path = workspace_folders.get(0)?.clone();
    let config_files = if let Some(config_path) = config_path {
        vec![config_path]
    } else {
        vec![
            main_path.join(".luarc.json"),
            main_path.join(".emmyrc.json"),
        ]
    };

    let mut emmyrc = load_configs(config_files, None);
    emmyrc.pre_process_emmyrc(&main_path);

    for root in &emmyrc.workspace.workspace_roots {
        analysis.add_main_workspace(PathBuf::from_str(root).unwrap());
    }

    for lib in &emmyrc.workspace.library {
        analysis.add_main_workspace(PathBuf::from_str(lib).unwrap());
        workspace_folders.push(PathBuf::from_str(lib).unwrap());
    }

    analysis.update_config(Arc::new(emmyrc));

    let file_infos = collect_files(&workspace_folders, &analysis.emmyrc, ignore);
    let files = file_infos
        .into_iter()
        .filter_map(|file| {
            if file.path.ends_with(".editorconfig") {
                let file_path = PathBuf::from(file.path);
                let parent_dir = file_path
                    .parent()
                    .unwrap()
                    .to_path_buf()
                    .to_string_lossy()
                    .to_string()
                    .replace("\\", "/");
                let file_normalized = file_path.to_string_lossy().to_string().replace("\\", "/");
                update_code_style(&parent_dir, &file_normalized);
                None
            } else {
                Some(file.into_tuple())
            }
        })
        .collect();
    analysis.update_files_by_path(files);

    Some(analysis)
}

pub fn collect_files(
    workspaces: &Vec<PathBuf>,
    emmyrc: &Emmyrc,
    ignore: Option<Vec<String>>,
) -> Vec<LuaFileInfo> {
    let mut files = Vec::new();
    let (match_pattern, exclude, exclude_dir) = calculate_include_and_exclude(emmyrc, ignore);

    let encoding = &emmyrc.workspace.encoding;

    for workspace in workspaces {
        let loaded = load_workspace_files(
            &workspace,
            &match_pattern,
            &exclude,
            &exclude_dir,
            Some(encoding),
        )
        .ok();
        if let Some(loaded) = loaded {
            files.extend(loaded);
        }
    }

    files
}

pub fn calculate_include_and_exclude(
    emmyrc: &Emmyrc,
    ignore: Option<Vec<String>>,
) -> (Vec<String>, Vec<String>, Vec<PathBuf>) {
    let mut include = vec!["**/*.lua".to_string(), "**/.editorconfig".to_string()];
    let mut exclude = Vec::new();
    let mut exclude_dirs = Vec::new();

    for extension in &emmyrc.runtime.extensions {
        if extension.starts_with(".") {
            include.push(format!("**/*{}", extension));
        } else if extension.starts_with("*.") {
            include.push(format!("**/{}", extension));
        } else {
            include.push(extension.clone());
        }
    }

    for ignore_glob in &emmyrc.workspace.ignore_globs {
        exclude.push(ignore_glob.clone());
    }

    if let Some(ignore) = ignore {
        exclude.extend(ignore);
    }

    for dir in &emmyrc.workspace.ignore_dir {
        exclude_dirs.push(PathBuf::from(dir));
    }

    // remove duplicate
    include.sort();
    include.dedup();

    // remove duplicate
    exclude.sort();
    exclude.dedup();

    (include, exclude, exclude_dirs)
}
