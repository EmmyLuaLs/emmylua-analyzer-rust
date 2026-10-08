use emmylua_code_analysis::{Emmyrc, read_file_with_encoding, uri_to_file_path};
use lsp_types::{DidChangeWatchedFilesParams, FileChangeType, Uri};
use std::sync::Arc;

use crate::context::{ServerContextSnapshot, UpdateEvent};

pub async fn on_did_change_watched_files(
    context: ServerContextSnapshot,
    params: DidChangeWatchedFilesParams,
) -> Option<()> {
    let _ = context
        .update_tx()
        .send(UpdateEvent::DidChangeWatchedFiles(params));
    Some(())
}

pub async fn process_did_change_watched_files(
    context: ServerContextSnapshot,
    params: DidChangeWatchedFilesParams,
) -> Option<()> {
    // Read workspace state and collect file events only while holding the outer lock; don't hold the lock across await/update.
    let (encoding, interval, lsp_features) = {
        // let workspace = context.workspace_manager().lock().await;
        let emmyrc = context
            .analysis()
            .with_snapshot(|analysis| analysis.get_emmyrc())
            .unwrap_or_else(|| Arc::new(Emmyrc::default()));
        let encoding = emmyrc.workspace.encoding.clone();
        let interval = emmyrc.diagnostics.diagnostic_interval.unwrap_or(500);
        (encoding, interval, context.lsp_features())
    };

    let mut watched_lua_files: Vec<(Uri, Option<String>)> = Vec::new();
    let mut removed_uris: Vec<Uri> = Vec::new();
    let mut emmyrc_updates: Vec<std::path::PathBuf> = Vec::new();

    {
        let workspace = context.workspace_manager().lock().await;
        for file_event in params.changes.into_iter() {
            let file_type = get_file_type(&file_event.uri);
            match file_type {
                Some(WatchedFileType::Lua) => {
                    if file_event.typ == FileChangeType::DELETED {
                        removed_uris.push(file_event.uri.clone());
                        if !lsp_features.supports_pull_diagnostic() {
                            context
                                .file_diagnostic()
                                .clear_push_file_diagnostics(file_event.uri);
                        }
                        continue;
                    }

                    if !workspace.is_open_file(&file_event.uri) {
                        if !workspace.is_workspace_file(&file_event.uri) {
                            continue;
                        }

                        collect_lua_files(
                            &mut watched_lua_files,
                            file_event.uri,
                            file_event.typ,
                            &encoding,
                        );
                    }
                }
                Some(WatchedFileType::Emmyrc) => {
                    if file_event.typ == FileChangeType::DELETED {
                        continue;
                    }
                    if let Some(config_path) = uri_to_file_path(&file_event.uri) {
                        emmyrc_updates.push(config_path);
                    }
                }
                None => {}
            }
        }
    }

    // Config updates must happen after releasing the workspace lock, as they re-acquire it internally.
    for config_path in emmyrc_updates {
        context
            .workspace_manager()
            .lock()
            .await
            .add_update_emmyrc_task(context.clone(), config_path)
            .await;
    }

    let file_ids = context
        .analysis()
        .update(|analysis| {
            for uri in &removed_uris {
                analysis.remove_file_by_uri(uri);
            }
            analysis.update_files_by_uri(watched_lua_files)
        })
        .await;
    context
        .file_diagnostic()
        .add_files_diagnostic_task(file_ids, interval)
        .await;

    Some(())
}

fn collect_lua_files(
    watched_lua_files: &mut Vec<(Uri, Option<String>)>,
    uri: Uri,
    file_change_event: FileChangeType,
    encoding: &str,
) {
    match file_change_event {
        FileChangeType::CREATED | FileChangeType::CHANGED => {
            let path = uri_to_file_path(&uri).unwrap();
            if let Some(text) = read_file_with_encoding(&path, encoding) {
                watched_lua_files.push((uri, Some(text)));
            }
        }
        FileChangeType::DELETED => {
            watched_lua_files.push((uri, None));
        }
        _ => {}
    }
}

enum WatchedFileType {
    Lua,
    Emmyrc,
}

fn get_file_type(uri: &Uri) -> Option<WatchedFileType> {
    let path = uri_to_file_path(uri)?;
    let file_name = path.file_name()?.to_str()?;
    match file_name {
        ".emmyrc.json" | ".luarc.json" | ".emmyrc.lua" => Some(WatchedFileType::Emmyrc),
        _ => Some(WatchedFileType::Lua),
    }
}
