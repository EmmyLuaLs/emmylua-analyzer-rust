mod call_hierarchy;
mod code_actions;
mod code_lens;
mod command;
mod completion;
mod configuration;
mod definition;
mod diagnostic;
mod document_color;
mod document_formatting;
mod document_highlight;
mod document_link;
mod document_range_formatting;
mod document_selection_range;
mod document_symbol;
mod document_type_format;
mod emmy_annotator;
mod emmy_gutter;
mod emmy_syntax_tree;
mod fold_range;
mod hover;
mod implementation;
mod initialized;
mod inlay_hint;
mod inline_values;
mod notification_handler;
mod references;
mod rename;
mod request_handler;
mod response_handler;
mod semantic_token;
mod signature_helper;
mod text_document;
mod workspace;
mod workspace_symbol;

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_lib;

pub use initialized::{ClientConfig, init_analysis, initialized_handler};
use lsp_types::{ClientCapabilities, ServerCapabilities};
pub use notification_handler::on_notification_handler;
pub use request_handler::on_request_handler;
pub use response_handler::on_response_handler;
pub use text_document::register_files_watch;

pub trait RegisterCapabilities {
    fn register_capabilities(
        server_capabilities: &mut ServerCapabilities,
        client_capabilities: &ClientCapabilities,
    );
}

macro_rules! capabilities {
    // module name => capability type mapping
    (modules: {
        $($module:ident => $capability:ident),* $(,)?
    }) => {
        pub fn server_capabilities(client_capabilities: &ClientCapabilities) -> ServerCapabilities {
            let mut server_capabilities = ServerCapabilities::default();

            $(
                $module::$capability::register_capabilities(&mut server_capabilities, client_capabilities);
            )*

            server_capabilities
        }
    };
}

capabilities!(modules: {
    text_document => TextDocumentCapabilities,
    document_symbol => DocumentSymbolCapabilities,
    document_color => DocumentColorCapabilities,
    document_link => DocumentLinkCapabilities,
    document_selection_range => DocumentSelectionRangeCapabilities,
    document_highlight => DocumentHighlightCapabilities,
    document_formatting => DocumentFormattingCapabilities,
    document_range_formatting => DocumentRangeFormattingCapabilities,
    // document_type_format => DocumentTypeFormattingCapabilities,
    completion => CompletionCapabilities,
    inlay_hint => InlayHintCapabilities,
    definition => DefinitionCapabilities,
    implementation => ImplementationCapabilities,
    references => ReferencesCapabilities,
    rename => RenameCapabilities,
    code_lens => CodeLensCapabilities,
    signature_helper => SignatureHelperCapabilities,
    hover => HoverCapabilities,
    fold_range => FoldRangeCapabilities,
    semantic_token => SemanticTokenCapabilities,
    command => CommandCapabilities,
    code_actions => CodeActionsCapabilities,
    inline_values => InlineValuesCapabilities,
    workspace_symbol => WorkspaceSymbolCapabilities,
    configuration => ConfigurationCapabilities,
    call_hierarchy => CallHierarchyCapabilities,
    workspace => WorkspaceCapabilities,
    diagnostic => DiagnosticCapabilities,
});
