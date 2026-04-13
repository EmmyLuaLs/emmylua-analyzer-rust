use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{Emmyrc, EmmyrcWorkspacePathItem, LuaFileInfo, load_workspace_files};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WorkspaceImport {
    All,
    Package(PathBuf),
}

impl WorkspaceImport {
    pub fn includes_path(&self, relative_path: &Path) -> bool {
        match self {
            WorkspaceImport::All => true,
            WorkspaceImport::Package(path) => relative_path.starts_with(path),
        }
    }
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

    pub fn with_package(root: PathBuf, dir: PathBuf) -> Self {
        Self {
            root,
            import: WorkspaceImport::Package(dir),
            is_library: true,
        }
    }
}

pub fn build_workspace_folders(
    workspace_folders: &[WorkspaceFolder],
    emmyrc: &Emmyrc,
) -> Vec<WorkspaceFolder> {
    let mut resolved = workspace_folders.to_vec();

    resolved.extend(
        emmyrc
            .workspace
            .workspace_roots
            .iter()
            .map(|root| WorkspaceFolder::new(PathBuf::from(root), false)),
    );
    resolved.extend(
        emmyrc
            .workspace
            .library
            .iter()
            .map(|library| WorkspaceFolder::new(PathBuf::from(library.get_path()), true)),
    );
    resolved.extend(emmyrc.workspace.packages.iter().filter_map(|package| {
        let package_path = PathBuf::from(package.get_path());
        let Some(parent) = package_path.parent() else {
            log::warn!("package dir {:?} has no parent", package_path);
            return None;
        };
        let Some(name) = package_path.file_name() else {
            log::warn!("package dir {:?} has no file name", package_path);
            return None;
        };

        Some(WorkspaceFolder::with_package(
            parent.to_path_buf(),
            PathBuf::from(name),
        ))
    }));

    resolved
}

#[derive(Debug, Clone)]
pub struct WorkspaceFileMatcher {
    include: Vec<String>,
    entries: Vec<WorkspaceMatchEntry>,
    watch_roots: HashSet<PathBuf>,
}

#[derive(Debug, Clone)]
struct WorkspaceMatchEntry {
    root: PathBuf,
    is_library: bool,
    exclude: Vec<String>,
    exclude_dir: Vec<PathBuf>,
}

pub fn collect_workspace_files(
    workspaces: &[WorkspaceFolder],
    emmyrc: &Emmyrc,
    extra_include: Option<Vec<String>>,
    extra_exclude: Option<Vec<String>>,
) -> Vec<LuaFileInfo> {
    let matcher = build_workspace_file_matcher(workspaces, emmyrc, extra_include, extra_exclude);
    let encoding = &emmyrc.workspace.encoding;
    let mut files = Vec::new();
    let mut loaded_paths = HashSet::new(); // Track loaded file paths to avoid duplicates

    log::info!(
        "collect_files from: {:?} match_pattern: {:?}, entries: {:?}",
        workspaces,
        matcher.source_file_globs(),
        matcher.entries
    );

    for entry in &matcher.entries {
        extend_loaded_files(
            &mut files,
            &mut loaded_paths,
            load_workspace_files(
                &entry.root,
                matcher.source_file_globs(),
                &entry.exclude,
                &entry.exclude_dir,
                Some(encoding),
            )
            .ok(),
        );
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

pub fn find_workspace_path_exclude(
    root: &Path,
    configured_entries: &[EmmyrcWorkspacePathItem],
) -> (Vec<String>, Vec<PathBuf>) {
    let mut exclude = Vec::new();
    let mut exclude_dirs = Vec::new();

    for entry in configured_entries {
        if let EmmyrcWorkspacePathItem::Config(detail_config) = entry {
            let configured_path = PathBuf::from(&detail_config.path);
            if configured_path == root {
                exclude = detail_config.ignore_globs.clone();
                exclude_dirs = detail_config.ignore_dir.iter().map(PathBuf::from).collect();
                break;
            }
        }
    }

    (exclude, exclude_dirs)
}

fn build_workspace_file_matcher(
    workspace_folders: &[WorkspaceFolder],
    emmyrc: &Emmyrc,
    extra_include: Option<Vec<String>>,
    extra_exclude: Option<Vec<String>>,
) -> WorkspaceFileMatcher {
    let (include, mut entries) =
        build_workspace_matcher_parts(workspace_folders, emmyrc, extra_include, extra_exclude);
    let watch_roots = entries.iter().map(|entry| entry.root.clone()).collect();
    entries.sort_by_key(|entry| {
        std::cmp::Reverse((entry.root.components().count(), entry.is_library))
    });

    WorkspaceFileMatcher {
        include,
        entries,
        watch_roots,
    }
}

fn build_workspace_matcher_parts(
    workspace_folders: &[WorkspaceFolder],
    emmyrc: &Emmyrc,
    extra_include: Option<Vec<String>>,
    extra_exclude: Option<Vec<String>>,
) -> (Vec<String>, Vec<WorkspaceMatchEntry>) {
    let (mut include, mut exclude, exclude_dir) = calculate_include_and_exclude(emmyrc);
    if let Some(extra_include) = extra_include {
        include.extend(extra_include);
        include.sort();
        include.dedup();
    }
    if let Some(extra_exclude) = extra_exclude {
        exclude.extend(extra_exclude);
        exclude.sort();
        exclude.dedup();
    }

    let mut entries = workspace_folders
        .iter()
        .cloned()
        .flat_map(|workspace| {
            WorkspaceMatchEntry::from_workspace(workspace, &exclude, &exclude_dir, emmyrc)
        })
        .collect::<Vec<_>>();
    add_child_workspace_excludes(&mut entries);

    (include, entries)
}

impl WorkspaceFileMatcher {
    pub fn new(workspace_folders: &[WorkspaceFolder], emmyrc: &Emmyrc) -> Self {
        let workspace_folders = build_workspace_folders(workspace_folders, emmyrc);
        build_workspace_file_matcher(&workspace_folders, emmyrc, None, None)
    }

    pub fn is_match(&self, path: &std::path::Path) -> bool {
        let include_set = match wax::any(self.include.iter().map(|s| s.as_str())) {
            Ok(include_set) => include_set,
            Err(_) => {
                log::error!("Invalid include pattern");
                return true;
            }
        };

        for entry in &self.entries {
            let Ok(relative_path) = path.strip_prefix(&entry.root) else {
                continue;
            };

            if entry.exclude_dir.iter().any(|dir| path.starts_with(dir)) {
                continue;
            }

            if !entry.exclude.is_empty() {
                match wax::any(entry.exclude.iter().map(|s| s.as_str())) {
                    Ok(exclude_set) if wax::Pattern::is_match(&exclude_set, relative_path) => {
                        continue;
                    }
                    Ok(_) => {}
                    Err(_) => log::error!("Invalid exclude pattern"),
                }
            }

            if wax::Pattern::is_match(&include_set, relative_path) {
                return true;
            }
        }

        false
    }

    pub fn watch_roots(&self) -> HashSet<PathBuf> {
        self.watch_roots.clone()
    }

    pub fn source_file_globs(&self) -> &[String] {
        &self.include
    }
}

impl WorkspaceMatchEntry {
    fn from_workspace(
        workspace: WorkspaceFolder,
        exclude: &[String],
        exclude_dir: &[PathBuf],
        emmyrc: &Emmyrc,
    ) -> Vec<Self> {
        let is_library = workspace.is_library;
        let workspace_root = workspace.root;
        let (roots, configured_entries) = match workspace.import {
            WorkspaceImport::All => (
                vec![workspace_root],
                is_library.then_some(emmyrc.workspace.library.as_slice()),
            ),
            WorkspaceImport::Package(path) => {
                let roots = vec![workspace_root.join(path)];
                (
                    roots,
                    is_library.then_some(emmyrc.workspace.packages.as_slice()),
                )
            }
        };

        roots
            .into_iter()
            .map(|root| {
                let mut entry_exclude = exclude.to_vec();
                let mut entry_exclude_dir = exclude_dir.to_vec();
                if let Some(configured_entries) = configured_entries {
                    let (configured_exclude, configured_exclude_dir) =
                        find_workspace_path_exclude(&root, configured_entries);
                    entry_exclude.extend(configured_exclude);
                    entry_exclude.sort();
                    entry_exclude.dedup();

                    entry_exclude_dir.extend(configured_exclude_dir);
                    entry_exclude_dir.sort();
                    entry_exclude_dir.dedup();
                }

                Self::new(root, is_library, &entry_exclude, &entry_exclude_dir)
            })
            .collect()
    }

    fn new(root: PathBuf, is_library: bool, exclude: &[String], exclude_dir: &[PathBuf]) -> Self {
        let exclude_dir = exclude_dir
            .iter()
            .filter(|dir| !root.starts_with(dir))
            .cloned()
            .collect();

        Self {
            root,
            is_library,
            exclude: exclude.to_vec(),
            exclude_dir,
        }
    }
}

fn add_child_workspace_excludes(entries: &mut [WorkspaceMatchEntry]) {
    let roots = entries
        .iter()
        .map(|entry| entry.root.clone())
        .collect::<Vec<_>>();
    for (idx, entry) in entries.iter_mut().enumerate() {
        for (other_idx, root) in roots.iter().enumerate() {
            if idx == other_idx {
                continue;
            }

            if let Ok(relative) = root.strip_prefix(&entry.root)
                && relative.components().count() > 0
            {
                entry.exclude_dir.push(root.clone());
            }
        }

        entry.exclude_dir.sort();
        entry.exclude_dir.dedup();
    }
}

fn extend_loaded_files(
    files: &mut Vec<LuaFileInfo>,
    loaded_paths: &mut HashSet<PathBuf>,
    loaded: Option<Vec<LuaFileInfo>>,
) {
    let Some(loaded) = loaded else {
        return;
    };

    for file in loaded {
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

impl Default for WorkspaceFileMatcher {
    fn default() -> Self {
        Self {
            include: vec!["**/*.lua".to_string()],
            entries: Vec::new(),
            watch_roots: HashSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EmmyLuaAnalysis, Emmyrc, file_path_to_uri};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEST_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestWorkspace {
        root: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let counter = TEST_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "emmylua-collect-workspace-files-{}-{}-{}",
                std::process::id(),
                unique,
                counter,
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write_file(&self, relative_path: &str) -> PathBuf {
            let path = self.root.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "return true\n").unwrap();
            path
        }

        fn path(&self, relative_path: &str) -> PathBuf {
            self.root.join(relative_path)
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn loaded_paths(files: Vec<LuaFileInfo>) -> HashSet<PathBuf> {
        files
            .into_iter()
            .map(|file| PathBuf::from(file.path))
            .collect()
    }

    fn to_string(path: &Path) -> String {
        path.to_string_lossy().to_string()
    }

    fn json_string(value: &str) -> String {
        serde_json::to_string(value).unwrap()
    }

    fn emmyrc_from_json(json: &str) -> Emmyrc {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn library_is_indexed_even_when_root_is_globally_ignored() {
        let workspace = TestWorkspace::new();
        let main_file = workspace.write_file("lua/main.lua");
        let library_root = workspace.path(".test-deps/runtime/lua/vim");
        let library_file = workspace.write_file(".test-deps/runtime/lua/vim/shared.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "library": [{}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path(".test-deps"))),
            json_string(&to_string(&library_root)),
        ));

        let files = collect_workspace_files(
            &[
                WorkspaceFolder::new(workspace.root.clone(), false),
                WorkspaceFolder::new(library_root.clone(), true),
            ],
            &emmyrc,
            None,
            None,
        );

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&main_file));
        assert!(loaded.contains(&library_file));
    }

    #[test]
    fn library_specific_ignores_still_apply() {
        let workspace = TestWorkspace::new();
        let library_root = workspace.path(".test-deps/runtime/lua/vim");
        let kept_file = workspace.write_file(".test-deps/runtime/lua/vim/keep.lua");
        let ignored_dir_file = workspace.write_file(".test-deps/runtime/lua/vim/tests/spec.lua");
        let ignored_glob_file = workspace.write_file(".test-deps/runtime/lua/vim/async.spec.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "library": [{{
                        "path": {},
                        "ignoreDir": [{}],
                        "ignoreGlobs": ["**/*.spec.lua"]
                    }}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path(".test-deps"))),
            json_string(&to_string(&library_root)),
            json_string(&to_string(&library_root.join("tests"))),
        ));

        let files = collect_workspace_files(
            &[
                WorkspaceFolder::new(workspace.root.clone(), false),
                WorkspaceFolder::new(library_root.clone(), true),
            ],
            &emmyrc,
            None,
            None,
        );

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&kept_file));
        assert!(!loaded.contains(&ignored_dir_file));
        assert!(!loaded.contains(&ignored_glob_file));
    }

    #[test]
    fn global_ignore_globs_still_apply_to_libraries() {
        let workspace = TestWorkspace::new();
        let library_root = workspace.path(".test-deps/runtime/lua/vim");
        let kept_file = workspace.write_file(".test-deps/runtime/lua/vim/keep.lua");
        let ignored_file = workspace.write_file(".test-deps/runtime/lua/vim/tests/spec.skip.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "ignoreGlobs": ["**/*.skip.lua"],
                    "library": [{}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path(".test-deps"))),
            json_string(&to_string(&library_root)),
        ));

        let files = collect_workspace_files(
            &[
                WorkspaceFolder::new(workspace.root.clone(), false),
                WorkspaceFolder::new(library_root.clone(), true),
            ],
            &emmyrc,
            None,
            None,
        );

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&kept_file));
        assert!(!loaded.contains(&ignored_file));
    }

    #[test]
    fn configured_workspace_root_is_indexed_even_when_parent_is_globally_ignored() {
        let workspace = TestWorkspace::new();
        let main_file = workspace.write_file("lua/main.lua");
        let configured_root = workspace.path(".generated/runtime");
        let configured_file = workspace.write_file(".generated/runtime/shared.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "workspaceRoots": [{}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path(".generated"))),
            json_string(&to_string(&configured_root)),
        ));

        let workspace_folders = build_workspace_folders(
            &[WorkspaceFolder::new(workspace.root.clone(), false)],
            &emmyrc,
        );
        let files = collect_workspace_files(&workspace_folders, &emmyrc, None, None);

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&main_file));
        assert!(loaded.contains(&configured_file));
    }

    #[test]
    fn workspace_root_is_indexed_even_when_parent_is_globally_ignored() {
        let workspace = TestWorkspace::new();
        let nested_root = workspace.path("packages/app");
        let nested_file = workspace.write_file("packages/app/init.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path("packages"))),
        ));

        let files = collect_workspace_files(
            &[WorkspaceFolder::new(nested_root.clone(), false)],
            &emmyrc,
            None,
            None,
        );

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&nested_file));
    }

    #[test]
    fn nested_global_ignore_dirs_still_apply_inside_library_roots() {
        let workspace = TestWorkspace::new();
        let library_root = workspace.path("libs/runtime/lua/vim");
        let kept_file = workspace.write_file("libs/runtime/lua/vim/keep.lua");
        let ignored_file = workspace.write_file("libs/runtime/lua/vim/tests/spec.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "library": [{}]
                }}
            }}"#,
            json_string(&to_string(&library_root.join("tests"))),
            json_string(&to_string(&library_root)),
        ));

        let files = collect_workspace_files(
            &[
                WorkspaceFolder::new(workspace.root.clone(), false),
                WorkspaceFolder::new(library_root.clone(), true),
            ],
            &emmyrc,
            None,
            None,
        );

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&kept_file));
        assert!(!loaded.contains(&ignored_file));
    }

    #[test]
    fn package_is_indexed_even_when_parent_is_globally_ignored() {
        let workspace = TestWorkspace::new();
        let main_file = workspace.write_file("lua/main.lua");
        let package_root = workspace.path(".rocks/share/lua/5.1/module");
        let package_file = workspace.write_file(".rocks/share/lua/5.1/module/init.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "packages": [{}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path(".rocks"))),
            json_string(&to_string(&package_root)),
        ));

        let workspace_folders = build_workspace_folders(
            &[WorkspaceFolder::new(workspace.root.clone(), false)],
            &emmyrc,
        );
        let files = collect_workspace_files(&workspace_folders, &emmyrc, None, None);

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&main_file));
        assert!(loaded.contains(&package_file));
    }

    #[test]
    fn package_specific_ignores_still_apply() {
        let workspace = TestWorkspace::new();
        let package_root = workspace.path(".rocks/share/lua/5.1/module");
        let kept_file = workspace.write_file(".rocks/share/lua/5.1/module/keep.lua");
        let ignored_dir_file = workspace.write_file(".rocks/share/lua/5.1/module/tests/spec.lua");
        let ignored_glob_file = workspace.write_file(".rocks/share/lua/5.1/module/async.spec.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "packages": [{{
                        "path": {},
                        "ignoreDir": [{}],
                        "ignoreGlobs": ["**/*.spec.lua"]
                    }}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path(".rocks"))),
            json_string(&to_string(&package_root)),
            json_string(&to_string(&package_root.join("tests"))),
        ));

        let workspace_folders = build_workspace_folders(
            &[WorkspaceFolder::new(workspace.root.clone(), false)],
            &emmyrc,
        );
        let files = collect_workspace_files(&workspace_folders, &emmyrc, None, None);

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&kept_file));
        assert!(!loaded.contains(&ignored_dir_file));
        assert!(!loaded.contains(&ignored_glob_file));
    }

    #[test]
    fn sibling_packages_under_shared_parent_only_index_configured_dirs() {
        let workspace = TestWorkspace::new();
        let main_file = workspace.write_file("lua/main.lua");
        let package_parent = workspace.path(".rocks/share/lua/5.1/module");
        let socket_root = package_parent.join("socket");
        let net_root = package_parent.join("net");
        let socket_file = workspace.write_file(".rocks/share/lua/5.1/module/socket/init.lua");
        let net_file = workspace.write_file(".rocks/share/lua/5.1/module/net/init.lua");
        let http_file = workspace.write_file(".rocks/share/lua/5.1/module/http/init.lua");

        let emmyrc = emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "ignoreDir": [{}],
                    "packages": [{}, {}]
                }}
            }}"#,
            json_string(&to_string(&workspace.path(".rocks"))),
            json_string(&to_string(&socket_root)),
            json_string(&to_string(&net_root)),
        ));

        let workspace_folders = build_workspace_folders(
            &[WorkspaceFolder::new(workspace.root.clone(), false)],
            &emmyrc,
        );
        assert!(workspace_folders.iter().any(|folder| {
            folder.root == package_parent
                && folder.import == WorkspaceImport::Package(PathBuf::from("socket"))
                && folder.is_library
        }));
        assert!(workspace_folders.iter().any(|folder| {
            folder.root == package_parent
                && folder.import == WorkspaceImport::Package(PathBuf::from("net"))
                && folder.is_library
        }));

        let files = collect_workspace_files(&workspace_folders, &emmyrc, None, None);

        let loaded = loaded_paths(files);
        assert!(loaded.contains(&main_file));
        assert!(loaded.contains(&socket_file));
        assert!(loaded.contains(&net_file));
        assert!(!loaded.contains(&http_file));

        let mut analysis = EmmyLuaAnalysis::new();
        analysis.update_config(Arc::new(emmyrc.clone()));
        for workspace in workspace_folders.iter().filter(|folder| folder.is_library) {
            analysis.add_library_workspace(workspace);
        }
        analysis.update_files_by_path(vec![
            (socket_file.clone(), Some("return true\n".to_string())),
            (net_file.clone(), Some("return true\n".to_string())),
        ]);

        let socket_file_id = analysis
            .get_file_id(&file_path_to_uri(&socket_file).unwrap())
            .unwrap();
        let net_file_id = analysis
            .get_file_id(&file_path_to_uri(&net_file).unwrap())
            .unwrap();

        assert_ne!(
            analysis
                .compilation
                .get_db()
                .get_module_index()
                .get_workspace_id(socket_file_id),
            analysis
                .compilation
                .get_db()
                .get_module_index()
                .get_workspace_id(net_file_id)
        );
    }
}
