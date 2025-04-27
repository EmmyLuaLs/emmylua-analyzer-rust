mod call_hierarchy;
mod code_actions;
mod code_lens;
mod command;
mod completion;
mod configuration;
mod definition;
mod document_color;
mod document_formatting;
mod document_highlight;
mod document_link;
mod document_range_formatting;
mod document_selection_range;
mod document_symbol;
mod emmy_annotator;
mod fold_range;
mod hover;
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
mod workspace_symbol;
mod test_lib;

pub use initialized::initialized_handler;
pub use initialized::{init_analysis, ClientConfig};
use lsp_types::{ClientCapabilities, ServerCapabilities};
pub use notification_handler::on_notification_handler;
pub use request_handler::on_req_handler;
pub use response_handler::on_response_handler;

pub trait RegisterCapabilities {
    fn register_capabilities(
        server_capabilities: &mut ServerCapabilities,
        client_capabilities: &ClientCapabilities,
    );
}

fn register<T: RegisterCapabilities>(
    server_capabilities: &mut ServerCapabilities,
    client_capabilities: &ClientCapabilities,
) {
    T::register_capabilities(server_capabilities, client_capabilities);
}

pub fn server_capabilities(client_capabilities: &ClientCapabilities) -> ServerCapabilities {
    let mut server_capabilities = ServerCapabilities::default();

    register::<text_document::TextDocumentCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<document_symbol::DocumentSymbolCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<document_color::DocumentColorCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<document_link::DocumentLinkCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<document_selection_range::DocumentSelectionRangeCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<document_highlight::DocumentHighlightCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<document_formatting::DocumentFormattingCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<document_range_formatting::DocumentRangeFormatting>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<completion::CompletionCapabilities>(&mut server_capabilities, client_capabilities);
    register::<inlay_hint::InlayHintCapabilities>(&mut server_capabilities, client_capabilities);
    register::<definition::DefinitionCapabilities>(&mut server_capabilities, client_capabilities);
    register::<references::ReferencesCapabilities>(&mut server_capabilities, client_capabilities);
    register::<rename::RenameCapabilities>(&mut server_capabilities, client_capabilities);
    register::<code_lens::CodeLensCapabilities>(&mut server_capabilities, client_capabilities);
    register::<signature_helper::SignatureHelperCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<hover::HoverCapabilities>(&mut server_capabilities, client_capabilities);
    register::<fold_range::FoldRangeCapabilities>(&mut server_capabilities, client_capabilities);
    register::<semantic_token::SemanticTokenCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<command::CommandCapabilities>(&mut server_capabilities, client_capabilities);
    register::<code_actions::CodeActionsCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<inline_values::InlineValuesCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<workspace_symbol::WorkspaceSymbolCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<configuration::ConfigurationCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );
    register::<call_hierarchy::CallHierarchyCapabilities>(
        &mut server_capabilities,
        client_capabilities,
    );

    server_capabilities
}
