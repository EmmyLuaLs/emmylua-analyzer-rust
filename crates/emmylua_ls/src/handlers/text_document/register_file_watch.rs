use emmylua_code_analysis::file_path_to_uri;
use log::{info, warn};
use lsp_types::{
    DidChangeWatchedFilesParams, DidChangeWatchedFilesRegistrationOptions, FileEvent,
    FileSystemWatcher, GlobPattern, Registration, RegistrationParams, Unregistration,
    UnregistrationParams, WatchKind,
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::channel,
    time::Duration,
};

use crate::{
    context::{ClientProxy, ServerContextSnapshot},
    handlers::text_document::on_did_change_watched_files,
};
use emmylua_code_analysis::{WorkspaceFileMatcher, WorkspaceFolder};

const WATCH_FILES_REGISTRATION_ID: &str = "emmylua_watch_files";
const WATCHED_CONFIG_GLOBS: [&str; 4] = [
    "**/.editorconfig",
    "**/.luarc.json",
    "**/.emmyrc.json",
    "**/.emmyrc.lua",
];
const WATCHED_CONFIG_FILE_NAMES: [&str; 4] = [
    ".editorconfig",
    ".luarc.json",
    ".emmyrc.json",
    ".emmyrc.lua",
];

pub async fn register_files_watch(context: ServerContextSnapshot) {
    let supports_dynamic_watch = context
        .lsp_features()
        .supports_dynamic_watched_files_registration();
    let (watch_roots, workspace_folders, match_file_pattern) = {
        let workspace_manager = context.workspace_manager().read().await;
        (
            reduce_watch_roots(workspace_manager.match_file_pattern.watch_roots()),
            workspace_manager.workspace_folders.clone(),
            workspace_manager.match_file_pattern.clone(),
        )
    };
    let watch_plan =
        build_watch_registration_plan(supports_dynamic_watch, &workspace_folders, watch_roots);

    if watch_plan.use_lsp_client {
        register_files_watch_use_lsp_client(
            context.client(),
            match_file_pattern.source_file_globs(),
        );
    } else if supports_dynamic_watch {
        unregister_files_watch_use_lsp_client(context.client());
    }

    if watch_plan.notify_roots.is_empty() {
        let mut workspace_manager = context.workspace_manager().write().await;
        workspace_manager.watcher = None;
        return;
    }

    info!("use notify to watch files: {:?}", watch_plan.notify_roots);
    let registered = register_files_watch_use_fsnotify(
        context.clone(),
        watch_plan.notify_roots,
        match_file_pattern,
    )
    .await;
    if !registered {
        if watch_plan.use_lsp_client {
            warn!(
                "notify registration failed for external roots; continuing with client watched files"
            );
        } else {
            warn!("notify registration failed; no watched-file backend is available");
        }
    }
}

#[derive(Debug)]
struct WatchRegistrationPlan {
    use_lsp_client: bool,
    notify_roots: Vec<PathBuf>,
}

fn build_watch_registration_plan(
    supports_dynamic_watch: bool,
    workspace_folders: &[WorkspaceFolder],
    watch_roots: Vec<PathBuf>,
) -> WatchRegistrationPlan {
    if !supports_dynamic_watch {
        return WatchRegistrationPlan {
            use_lsp_client: false,
            notify_roots: watch_roots,
        };
    }

    WatchRegistrationPlan {
        use_lsp_client: !workspace_folders.is_empty(),
        notify_roots: watch_roots
            .into_iter()
            .filter(|root| !is_root_covered_by_workspace_folders(root, workspace_folders))
            .collect(),
    }
}

fn is_root_covered_by_workspace_folders(
    path: &Path,
    workspace_folders: &[WorkspaceFolder],
) -> bool {
    workspace_folders
        .iter()
        .any(|workspace| is_path_covered_by_workspace(path, &workspace.root))
}

fn register_files_watch_use_lsp_client(client: &ClientProxy, source_file_globs: &[String]) {
    unregister_files_watch_use_lsp_client(client);

    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: lsp_file_watchers(&build_watch_file_globs(source_file_globs)),
    };

    let registration = Registration {
        id: WATCH_FILES_REGISTRATION_ID.to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(serde_json::to_value(options).unwrap()),
    };
    client.dynamic_register_capability(RegistrationParams {
        registrations: vec![registration],
    });
}

fn build_watch_file_globs(source_file_globs: &[String]) -> Vec<String> {
    let mut globs = source_file_globs.to_vec();
    globs.extend(WATCHED_CONFIG_GLOBS.into_iter().map(str::to_string));
    globs.sort();
    globs.dedup();
    globs
}

fn lsp_file_watchers(watch_file_globs: &[String]) -> Vec<FileSystemWatcher> {
    watch_file_globs
        .iter()
        .map(|glob_pattern| FileSystemWatcher {
            glob_pattern: GlobPattern::String(glob_pattern.clone()),
            kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete),
        })
        .collect()
}

fn unregister_files_watch_use_lsp_client(client: &ClientProxy) {
    client.dynamic_unregister_capability(UnregistrationParams {
        unregisterations: vec![Unregistration {
            id: WATCH_FILES_REGISTRATION_ID.to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
        }],
    });
}

async fn register_files_watch_use_fsnotify(
    context: ServerContextSnapshot,
    watch_roots: Vec<PathBuf>,
    match_file_pattern: WorkspaceFileMatcher,
) -> bool {
    let (tx, rx) = channel();
    let config = Config::default().with_poll_interval(Duration::from_secs(5));
    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                match tx.send(event) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("send notify event failed: {:?}", e);
                    }
                };
            };
        },
        config,
    ) {
        Ok(watcher) => watcher,
        Err(e) => {
            log::error!("create notify watcher failed: {:?}", e);
            return false;
        }
    };

    let mut watched_roots = Vec::new();
    for root in watch_roots {
        match watcher.watch(&root, RecursiveMode::Recursive) {
            Ok(()) => watched_roots.push(root),
            Err(e) => warn!("can not watch {:?}: {:?}", root, e),
        }
    }

    if watched_roots.is_empty() {
        warn!("can not watch any workspace roots with notify");
        return false;
    }

    let mut workspace_manager = context.workspace_manager().write().await;
    workspace_manager.watcher = Some(watcher);
    drop(workspace_manager);

    tokio::spawn(async move {
        loop {
            match rx.recv() {
                Ok(event) => {
                    let typ = match event.kind {
                        notify::event::EventKind::Create(_) => lsp_types::FileChangeType::CREATED,
                        notify::event::EventKind::Modify(_) => lsp_types::FileChangeType::CHANGED,
                        notify::event::EventKind::Remove(_) => lsp_types::FileChangeType::DELETED,
                        _ => {
                            continue;
                        }
                    };
                    let mut file_events = vec![];
                    for path in event.paths.iter() {
                        if is_watched_path(&match_file_pattern, path) {
                            if let Some(uri) = file_path_to_uri(path) {
                                file_events.push(FileEvent { uri, typ });
                            }
                        }
                    }

                    if file_events.is_empty() {
                        continue;
                    }
                    let params = DidChangeWatchedFilesParams {
                        changes: file_events,
                    };
                    on_did_change_watched_files(context.clone(), params).await;
                }
                Err(e) => {
                    warn!("watch files notify error: {:?}", e);
                    break;
                }
            }
        }
    });

    info!("watch files use notify success: {:?}", watched_roots);
    true
}

fn is_watched_path(match_file_pattern: &WorkspaceFileMatcher, path: &Path) -> bool {
    is_config_watch_path(path) || match_file_pattern.is_match(path)
}

fn is_config_watch_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| WATCHED_CONFIG_FILE_NAMES.contains(&name))
}

fn is_path_covered_by_workspace(path: &Path, workspace_root: &Path) -> bool {
    path.starts_with(workspace_root)
}

fn reduce_watch_roots<I>(roots: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut roots = roots.into_iter().collect::<Vec<_>>();
    roots.sort();

    let mut reduced_roots: Vec<PathBuf> = Vec::new();
    for root in roots {
        if reduced_roots
            .iter()
            .any(|existing| is_path_covered_by_workspace(&root, existing))
        {
            continue;
        }

        reduced_roots.retain(|existing| !is_path_covered_by_workspace(existing, &root));
        reduced_roots.push(root);
    }

    reduced_roots
}
