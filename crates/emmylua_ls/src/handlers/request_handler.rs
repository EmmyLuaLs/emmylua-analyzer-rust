use std::error::Error;

use log::error;
use lsp_server::{Request, Response};
use lsp_types::request::{
    CallHierarchyIncomingCalls, CallHierarchyOutgoingCalls, CallHierarchyPrepare,
    CodeActionRequest, CodeLensRequest, CodeLensResolve, ColorPresentationRequest, Completion,
    DocumentColor, DocumentDiagnosticRequest, DocumentHighlightRequest, DocumentLinkRequest,
    DocumentLinkResolve, DocumentSymbolRequest, ExecuteCommand, FoldingRangeRequest, Formatting,
    GotoDefinition, GotoImplementation, HoverRequest, InlayHintRequest, InlayHintResolveRequest,
    InlineValueRequest, OnTypeFormatting, PrepareRenameRequest, RangeFormatting, References,
    Rename, Request as LspRequest, ResolveCompletionItem, SelectionRangeRequest,
    SemanticTokensFullRequest, SignatureHelpRequest, WorkspaceDiagnosticRequest,
    WorkspaceSymbolRequest,
};

use crate::{
    context::ServerContext,
    handlers::{
        diagnostic::{on_pull_document_diagnostic, on_pull_workspace_diagnostic},
        document_type_format::on_type_formatting_handler,
        emmy_gutter::{
            EmmyGutterDetailRequest, EmmyGutterRequest, on_emmy_gutter_detail_handler,
            on_emmy_gutter_handler,
        },
        emmy_syntax_tree::{EmmySyntaxTreeRequest, on_emmy_syntax_tree_handler},
    },
};

use super::{
    call_hierarchy::{
        on_incoming_calls_handler, on_outgoing_calls_handler, on_prepare_call_hierarchy_handler,
    },
    code_actions::on_code_action_handler,
    code_lens::{on_code_lens_handler, on_resolve_code_lens_handler},
    command::on_execute_command_handler,
    completion::{on_completion_handler, on_completion_resolve_handler},
    definition::on_goto_definition_handler,
    document_color::{on_document_color, on_document_color_presentation},
    document_formatting::on_formatting_handler,
    document_highlight::on_document_highlight_handler,
    document_link::{on_document_link_handler, on_document_link_resolve_handler},
    document_range_formatting::on_range_formatting_handler,
    document_selection_range::on_document_selection_range_handle,
    document_symbol::on_document_symbol,
    emmy_annotator::{EmmyAnnotatorRequest, on_emmy_annotator_handler},
    fold_range::on_folding_range_handler,
    hover::on_hover,
    implementation::on_implementation_handler,
    inlay_hint::{on_inlay_hint_handler, on_resolve_inlay_hint},
    inline_values::on_inline_values_handler,
    references::on_references_handler,
    rename::{on_prepare_rename_handler, on_rename_handler},
    semantic_token::on_semantic_token_handler,
    signature_helper::on_signature_helper_handler,
    workspace_symbol::on_workspace_symbol_handler,
};

macro_rules! dispatch_request {
    ($request:expr, $context:expr, {
        $($req_type:ty => $handler:expr),* $(,)?
    }) => {
        match $request.method.as_str() {
            $(
                <$req_type>::METHOD => {
                    if let Ok((id, params)) = $request.extract::<<$req_type as LspRequest>::Params>(<$req_type>::METHOD) {
                        let snapshot = $context.snapshot();
                        $context.task(id.clone(), |cancel_token| async move {
                            $handler(snapshot, params, cancel_token).await
                        }).await;
                        return Ok(());
                    }
                }
            )*
            method => {
                error!("handler not found for request: {}", method);
                let response = Response::new_err(
                    $request.id.clone(),
                    lsp_server::ErrorCode::MethodNotFound as i32,
                    "handler not found".to_string(),
                );
                $context.send(response);
            }
        }
    };
}

pub async fn on_request_handler(
    req: Request,
    server_context: &mut ServerContext,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    dispatch_request!(req, server_context, {
        HoverRequest => on_hover,
        DocumentSymbolRequest => on_document_symbol,
        FoldingRangeRequest => on_folding_range_handler,
        DocumentColor => on_document_color,
        ColorPresentationRequest => on_document_color_presentation,
        DocumentLinkRequest => on_document_link_handler,
        DocumentLinkResolve => on_document_link_resolve_handler,
        EmmyAnnotatorRequest => on_emmy_annotator_handler,
        EmmyGutterRequest => on_emmy_gutter_handler,
        EmmyGutterDetailRequest => on_emmy_gutter_detail_handler,
        EmmySyntaxTreeRequest => on_emmy_syntax_tree_handler,
        SelectionRangeRequest => on_document_selection_range_handle,
        Completion => on_completion_handler,
        ResolveCompletionItem => on_completion_resolve_handler,
        InlayHintRequest => on_inlay_hint_handler,
        InlayHintResolveRequest => on_resolve_inlay_hint,
        GotoDefinition => on_goto_definition_handler,
        GotoImplementation => on_implementation_handler,
        References => on_references_handler,
        Rename => on_rename_handler,
        PrepareRenameRequest => on_prepare_rename_handler,
        CodeLensRequest => on_code_lens_handler,
        CodeLensResolve => on_resolve_code_lens_handler,
        SignatureHelpRequest => on_signature_helper_handler,
        DocumentHighlightRequest => on_document_highlight_handler,
        SemanticTokensFullRequest => on_semantic_token_handler,
        ExecuteCommand => on_execute_command_handler,
        CodeActionRequest => on_code_action_handler,
        InlineValueRequest => on_inline_values_handler,
        WorkspaceSymbolRequest => on_workspace_symbol_handler,
        Formatting => on_formatting_handler,
        RangeFormatting => on_range_formatting_handler,
        OnTypeFormatting => on_type_formatting_handler,
        CallHierarchyPrepare => on_prepare_call_hierarchy_handler,
        CallHierarchyIncomingCalls => on_incoming_calls_handler,
        CallHierarchyOutgoingCalls => on_outgoing_calls_handler,
        DocumentDiagnosticRequest => on_pull_document_diagnostic,
        WorkspaceDiagnosticRequest => on_pull_workspace_diagnostic,
    });

    Ok(())
}
