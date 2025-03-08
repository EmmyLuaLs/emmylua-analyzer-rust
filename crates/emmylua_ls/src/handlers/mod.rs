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

pub use initialized::initialized_handler;
pub use initialized::{init_analysis, ClientConfig};
use lsp_types::{ClientCapabilities, ServerCapabilities};
pub use notification_handler::on_notification_handler;
pub use request_handler::on_req_handler;
pub use response_handler::on_response_handler;

pub fn server_capabilities(client_capabilities: &ClientCapabilities) -> ServerCapabilities {
    let mut server_capabilities = ServerCapabilities::default();
    macro_rules! capabilities {
        ($module:ident) => {
            $module::register_capabilities(&mut server_capabilities, &client_capabilities);
        };
    }

    capabilities!(text_document);
    capabilities!(document_symbol);
    capabilities!(document_color);
    capabilities!(document_link);
    capabilities!(document_selection_range);
    capabilities!(document_highlight);
    capabilities!(document_formatting);
    capabilities!(document_range_formatting);
    capabilities!(completion);
    capabilities!(inlay_hint);
    capabilities!(definition);
    capabilities!(references);
    capabilities!(rename);
    capabilities!(code_lens);
    capabilities!(signature_helper);
    capabilities!(hover);
    capabilities!(fold_range);
    capabilities!(semantic_token);
    capabilities!(command);
    capabilities!(code_actions);
    capabilities!(inline_values);
    capabilities!(workspace_symbol);
    capabilities!(configuration);

    server_capabilities
}
