use std::{collections::HashSet, path::PathBuf};

use crate::{EmmyLibraryItem, Emmyrc, LuaFileInfo, load_workspace_files};

#[derive(Clone, Debug)]
pub enum WorkspaceImport {
    All,
    SubPaths(Vec<PathBuf>),
}

#[derive(Clone, Debug)]
pub struct WorkspaceFolder {
    pub root: PathBuf,
    pub import: WorkspaceImport,
    pub is_library: bool,
}

impl WorkspaceFolder {
    pub fn new(root: PathBuf, is_library: bool) -> Self {
        Self {
            root,
            import: WorkspaceImport::All,
            is_library,
        }
    }

    pub fn with_sub_paths(root: PathBuf, sub_paths: Vec<PathBuf>, is_library: bool) -> Self {
        Self {
            root,
            import: WorkspaceImport::SubPaths(sub_paths),
            is_library,
        }
    }
}

pub fn collect_workspace_files(
    workspaces: &Vec<WorkspaceFolder>,
    emmyrc: &Emmyrc,
    extra_include: Option<Vec<String>>,
    extra_exclude: Option<Vec<String>>,
) -> Vec<LuaFileInfo> {
    let mut files = Vec::new();
    let mut loaded_paths = HashSet::new(); // Track loaded file paths to avoid duplicates
    let (mut match_pattern, mut exclude, exclude_dir) = calculate_include_and_exclude(emmyrc);
    if let Some(extra_include) = extra_include {
        match_pattern.extend_from_slice(&extra_include);
        match_pattern.sort();
        match_pattern.dedup();
    }
    if let Some(extra_exclude) = extra_exclude {
        exclude.extend_from_slice(&extra_exclude);
        exclude.sort();
        exclude.dedup();
    }

    let encoding = &emmyrc.workspace.encoding;

    log::info!(
        "collect_files from: {:?} match_pattern: {:?} exclude: {:?}, exclude_dir: {:?}",
        workspaces,
        match_pattern,
        exclude,
        exclude_dir
    );

    for (idx, workspace) in workspaces.iter().enumerate() {
        // Build exclude_dirs for this workspace by finding child workspaces
        let mut workspace_exclude_dir = exclude_dir.clone();

        // Find all other workspaces that are children of current workspace
        for (other_idx, other_workspace) in workspaces.iter().enumerate() {
            if idx != other_idx {
                // Check if other_workspace is a child of current workspace
                if let Ok(relative) = other_workspace.root.strip_prefix(&workspace.root) {
                    if relative.components().count() > 0 {
                        // other_workspace is a child, add it to exclude_dir
                        workspace_exclude_dir.push(other_workspace.root.clone());
                        log::debug!(
                            "Excluding child workspace {:?} from parent {:?}",
                            other_workspace.root,
                            workspace.root
                        );
                    }
                }
            }
        }

        match &workspace.import {
            WorkspaceImport::All => {
                let loaded = if workspace.is_library {
                    let (lib_exclude, lib_exclude_dir) = find_library_exclude(workspace, emmyrc);
                    // Merge library exclude with workspace exclude
                    let mut merged_exclude = exclude.clone();
                    merged_exclude.extend(lib_exclude);
                    merged_exclude.sort();
                    merged_exclude.dedup();

                    let mut merged_exclude_dir = workspace_exclude_dir.clone();
                    merged_exclude_dir.extend(lib_exclude_dir);

                    load_workspace_files(
                        &workspace.root,
                        &match_pattern,
                        &merged_exclude,
                        &merged_exclude_dir,
                        Some(encoding),
                    )
                    .ok()
                } else {
                    load_workspace_files(
                        &workspace.root,
                        &match_pattern,
                        &exclude,
                        &workspace_exclude_dir,
                        Some(encoding),
                    )
                    .ok()
                };
                if let Some(loaded) = loaded {
                    for file in loaded {
                        // Normalize path and check for duplicates
                        let normalized_path = PathBuf::from(&file.path)
                            .canonicalize()
                            .unwrap_or_else(|_| PathBuf::from(&file.path));

                        if loaded_paths.insert(normalized_path) {
                            files.push(file);
                        } else {
                            log::debug!("Skipping duplicate file: {:?}", file.path);
                        }
                    }
                }
            }
            WorkspaceImport::SubPaths(paths) => {
                for sub in paths {
                    let target = workspace.root.join(sub);
                    let loaded = if workspace.is_library {
                        let (lib_exclude, lib_exclude_dir) =
                            find_library_exclude(workspace, emmyrc);
                        // Merge library exclude with workspace exclude
                        let mut merged_exclude = exclude.clone();
                        merged_exclude.extend(lib_exclude);
                        merged_exclude.sort();
                        merged_exclude.dedup();

                        let mut merged_exclude_dir = workspace_exclude_dir.clone();
                        merged_exclude_dir.extend(lib_exclude_dir);

                        load_workspace_files(
                            &target,
                            &match_pattern,
                            &merged_exclude,
                            &merged_exclude_dir,
                            Some(encoding),
                        )
                        .ok()
                    } else {
                        load_workspace_files(
                            &target,
                            &match_pattern,
                            &exclude,
                            &workspace_exclude_dir,
                            Some(encoding),
                        )
                        .ok()
                    };
                    if let Some(loaded) = loaded {
                        for file in loaded {
                            // Normalize path and check for duplicates
                            let normalized_path = PathBuf::from(&file.path)
                                .canonicalize()
                                .unwrap_or_else(|_| PathBuf::from(&file.path));

                            if loaded_paths.insert(normalized_path) {
                                files.push(file);
                            } else {
                                log::debug!("Skipping duplicate file: {:?}", file.path);
                            }
                        }
                    }
                }
            }
        }
    }

    log::info!("load files from workspace count: {:?}", files.len());

    for file in &files {
        log::debug!("loaded file: {:?}", file.path);
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

fn find_library_exclude(library: &WorkspaceFolder, emmyrc: &Emmyrc) -> (Vec<String>, Vec<PathBuf>) {
    let mut exclude = Vec::new();
    let mut exclude_dirs = Vec::new();

    for lib in &emmyrc.workspace.library {
        if let EmmyLibraryItem::Config(detail_config) = &lib {
            let lib_path = PathBuf::from(&detail_config.path);
            if lib_path == library.root {
                exclude = detail_config.ignore_globs.clone();
                exclude_dirs = detail_config.ignore_dir.iter().map(PathBuf::from).collect();
                break;
            }
        }
    }

    (exclude, exclude_dirs)
}
