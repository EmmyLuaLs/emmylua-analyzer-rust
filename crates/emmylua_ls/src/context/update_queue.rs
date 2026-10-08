//! # File update queue
//!
//! All filesystem changes are enqueued and processed serially by one worker, guaranteeing:
//! - open/change/close/watch/rename ordering;
//! - the notification loop never blocks;
//! - `$/cancelRequest` is always handled promptly.

use std::sync::Arc;

use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, RenameFilesParams,
};
use tokio::sync::mpsc;

use crate::handlers;

use super::snapshot::{ServerContextInner, ServerContextSnapshot};

pub enum UpdateEvent {
    DidOpen(DidOpenTextDocumentParams),
    DidChange(DidChangeTextDocumentParams),
    DidClose(DidCloseTextDocumentParams),
    DidChangeWatchedFiles(DidChangeWatchedFilesParams),
    DidRenameFiles(RenameFilesParams),
}

pub fn spawn_update_queue(
    inner: Arc<ServerContextInner>,
    mut rx: mpsc::UnboundedReceiver<UpdateEvent>,
) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let snapshot = ServerContextSnapshot::new(inner.clone());
            match event {
                UpdateEvent::DidOpen(params) => {
                    handlers::process_did_open_text_document(snapshot, params).await;
                }
                UpdateEvent::DidChange(params) => {
                    handlers::process_did_change_text_document(snapshot, params).await;
                }
                UpdateEvent::DidClose(params) => {
                    handlers::process_did_close_document(snapshot, params).await;
                }
                UpdateEvent::DidChangeWatchedFiles(params) => {
                    handlers::process_did_change_watched_files(snapshot, params).await;
                }
                UpdateEvent::DidRenameFiles(params) => {
                    handlers::process_did_rename_files_handler(snapshot, params).await;
                }
            }
        }
    });
}
