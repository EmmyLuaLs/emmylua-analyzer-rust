use emmylua_code_analysis::{read_file_with_encoding, uri_to_file_path};
use lsp_types::{DidChangeWatchedFilesParams, FileChangeType, Uri};

use crate::context::ServerContextSnapshot;

pub async fn on_did_change_watched_files(
    context: ServerContextSnapshot,
    params: DidChangeWatchedFilesParams,
) -> Option<()> {
    let mut analysis = context.analysis.write().await;
    let emmyrc = analysis.get_emmyrc();
    let encoding = &emmyrc.workspace.encoding;
    let interval = emmyrc.diagnostics.diagnostic_interval.unwrap_or(500);
    let mut watched_lua_files: Vec<(Uri, Option<String>)> = Vec::new();
    // let
    for file_event in params.changes.into_iter() {
        let file_type = get_file_type(&file_event.uri);
        match file_type {
            Some(WatchedFileType::Lua) => {
                collect_lua_files(
                    &mut watched_lua_files,
                    file_event.uri,
                    file_event.typ,
                    encoding,
                );
            }
            Some(WatchedFileType::Editorconfig) => {
                if file_event.typ == FileChangeType::DELETED {
                    continue;
                }
                let editorconfig_path = uri_to_file_path(&file_event.uri).unwrap();
                context
                    .workspace_manager
                    .read()
                    .await
                    .update_editorconfig(editorconfig_path);
            }
            Some(WatchedFileType::Emmyrc) => {
                if file_event.typ == FileChangeType::DELETED {
                    continue;
                }
                let emmyrc_path = uri_to_file_path(&file_event.uri).unwrap();
                let file_dir = emmyrc_path.parent().unwrap().to_path_buf();
                context
                    .workspace_manager
                    .read()
                    .await
                    .add_update_emmyrc_task(file_dir)
                    .await;
            }
            None => {}
        }
    }

    let file_ids = analysis.update_files_by_uri(watched_lua_files);
    context
        .file_diagnostic
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
            let text = read_file_with_encoding(&path, encoding).unwrap();
            watched_lua_files.push((uri, Some(text)));
        }
        FileChangeType::DELETED => {
            watched_lua_files.push((uri, None));
        }
        _ => {}
    }
}

enum WatchedFileType {
    Lua,
    Editorconfig,
    Emmyrc,
}

fn get_file_type(uri: &Uri) -> Option<WatchedFileType> {
    let path = uri_to_file_path(uri)?;
    let file_name = path.file_name()?.to_str()?;
    match file_name {
        ".editorconfig" => Some(WatchedFileType::Editorconfig),
        ".emmyrc.json" | ".luarc.json" => Some(WatchedFileType::Emmyrc),
        _ => Some(WatchedFileType::Lua),
    }
}
