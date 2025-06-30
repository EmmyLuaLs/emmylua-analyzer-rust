use std::{path::PathBuf, str::FromStr, sync::Arc};

use emmylua_code_analysis::{
    load_configs, load_workspace_files, EmmyLuaAnalysis, Emmyrc, LuaFileInfo,
};

#[allow(unused)]
pub fn load_workspace(workspace_folders: Vec<String>) -> Option<EmmyLuaAnalysis> {
    let mut analysis = EmmyLuaAnalysis::new();
    analysis.init_std_lib(None);

    let mut workspace_folders = workspace_folders
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    for path in &workspace_folders {
        analysis.add_main_workspace(path.clone());
    }

    let main_path = workspace_folders.first()?.clone();
    let config_files = vec![
        main_path.join(".luarc.json"),
        main_path.join(".emmyrc.json"),
    ];
    let mut emmyrc = load_configs(config_files, None);
    emmyrc.pre_process_emmyrc(&main_path);
    let emmyrc = Arc::new(emmyrc);

    for root in &emmyrc.workspace.workspace_roots {
        analysis.add_main_workspace(PathBuf::from_str(root).unwrap());
    }

    for lib in &emmyrc.workspace.library {
        analysis.add_main_workspace(PathBuf::from_str(lib).unwrap());
        workspace_folders.push(PathBuf::from_str(lib).unwrap());
    }

    analysis.update_config(emmyrc);

    let file_infos = collect_files(&workspace_folders, &analysis.emmyrc);
    let files = file_infos
        .into_iter()
        .map(|file| file.into_tuple())
        .collect();
    analysis.update_files_by_path(files);

    Some(analysis)
}

pub fn collect_files(workspaces: &Vec<PathBuf>, emmyrc: &Emmyrc) -> Vec<LuaFileInfo> {
    let mut files = Vec::new();
    let (match_pattern, exclude, exclude_dir) = calculate_include_and_exclude(emmyrc);

    let encoding = &emmyrc.workspace.encoding;

    eprintln!(
        "collect_files from: {:?} match_pattern: {:?} exclude: {:?}",
        workspaces, match_pattern, exclude
    );
    for workspace in workspaces {
        let loaded = load_workspace_files(
            workspace,
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

    eprintln!("load files from workspace count: {:?}", files.len());

    for file in &files {
        eprintln!("loaded file: {:?}", file.path);
    }
    files
}

pub fn calculate_include_and_exclude(emmyrc: &Emmyrc) -> (Vec<String>, Vec<String>, Vec<PathBuf>) {
    let mut include = vec!["**/*.lua".to_string()];
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
