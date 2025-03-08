mod register_file_watch;
mod set_trace;
mod text_document_handler;
mod watched_file_handler;

use lsp_types::{
    ClientCapabilities, SaveOptions, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncSaveOptions,
};
pub use register_file_watch::register_files_watch;
pub use set_trace::on_set_trace;
pub use text_document_handler::{
    on_did_change_text_document, on_did_close_document, on_did_open_text_document,
    on_did_save_text_document,
};
pub use watched_file_handler::on_did_change_watched_files;

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    _: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.text_document_sync = Some(TextDocumentSyncCapability::Options(
        lsp_types::TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::FULL),
            will_save: None,
            will_save_wait_until: None,
            save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                include_text: Some(false),
            })),
        },
    ));

    Some(())
}
