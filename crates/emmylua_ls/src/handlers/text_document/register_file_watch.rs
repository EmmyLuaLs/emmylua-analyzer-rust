use emmylua_code_analysis::file_path_to_uri;
use log::{info, warn};
use lsp_types::{
    ClientCapabilities, DidChangeWatchedFilesParams, DidChangeWatchedFilesRegistrationOptions,
    FileEvent, FileSystemWatcher, GlobPattern, Registration, RegistrationParams, WatchKind,
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    sync::{mpsc::channel, Arc},
    time::Duration,
};

use crate::{
    context::{ClientProxy, ServerContextSnapshot},
    handlers::text_document::on_did_change_watched_files,
};

pub async fn register_files_watch(
    context: ServerContextSnapshot,
    client_capabilities: &ClientCapabilities,
) {
    let lsp_client_can_watch_files = is_lsp_client_can_watch_files(client_capabilities);

    if lsp_client_can_watch_files {
        register_files_watch_use_lsp_client(context.client);
    } else {
        info!("use notify to watch files");
        register_files_watch_use_fsnotify(context).await;
    }
}

fn is_lsp_client_can_watch_files(client_capabilities: &ClientCapabilities) -> bool {
    if let Some(workspace) = &client_capabilities.workspace {
        if let Some(did_change_watched_files) = &workspace.did_change_watched_files {
            if let Some(dynamic_registration) = &did_change_watched_files.dynamic_registration {
                return dynamic_registration.clone();
            }
        }
    }
    false
}

fn register_files_watch_use_lsp_client(client: Arc<ClientProxy>) {
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: vec![
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.lua".into()),
                kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete),
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/.editorconfig".into()),
                kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete),
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/.luarc.json".into()),
                kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete),
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/.emmyrc.json".into()),
                kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete),
            },
        ],
    };

    let registration = Registration {
        id: "emmylua_watch_files".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(serde_json::to_value(options).unwrap()),
    };
    client.dynamic_register_capability(RegistrationParams {
        registrations: vec![registration],
    });
}

const WATCH_FILE_EXTENSIONS: [&str; 4] = [".lua", ".editorconfig", ".luarc.json", ".emmyrc.json"];

async fn register_files_watch_use_fsnotify(context: ServerContextSnapshot) -> Option<()> {
    let (tx, rx) = channel();
    let config = Config::default().with_poll_interval(Duration::from_secs(5));
    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            match res {
                Ok(event) => {
                    match tx.send(event) {
                        Ok(_) => {}
                        Err(e) => {
                            warn!("send notify event failed: {:?}", e);
                        }
                    };
                }
                _ => {}
            };
        },
        config,
    ) {
        Ok(watcher) => watcher,
        Err(e) => {
            log::error!("create notify watcher failed: {:?}", e);
            return None;
        }
    };

    let mut config_manager = context.workspace_manager.write().await;
    for workspace in &config_manager.workspace_folders {
        if let Err(e) = watcher.watch(workspace, RecursiveMode::Recursive) {
            warn!("can not watch {:?}: {:?}", workspace, e);
        }
    }
    config_manager.watcher = Some(watcher);
    drop(config_manager);

    tokio::spawn(async move {
        loop {
            match rx.recv() {
                Ok(event) => {
                    let typ = match event.kind {
                        notify::event::EventKind::Create(_) => lsp_types::FileChangeType::CREATED,
                        notify::event::EventKind::Modify(_) => lsp_types::FileChangeType::CHANGED,
                        notify::event::EventKind::Remove(_) => lsp_types::FileChangeType::DELETED,
                        _ => {
                            break;
                        }
                    };
                    let mut file_events = vec![];
                    for path in event.paths.iter() {
                        for ext in WATCH_FILE_EXTENSIONS.iter() {
                            if path.as_os_str().to_string_lossy().ends_with(ext) {
                                if let Some(uri) = file_path_to_uri(&path) {
                                    file_events.push(FileEvent { uri, typ });
                                }
                                break;
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

    info!("watch files use notify success");

    Some(())
}
