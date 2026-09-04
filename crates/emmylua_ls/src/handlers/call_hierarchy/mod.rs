mod build_call_hierarchy;

use build_call_hierarchy::{
    CallHierarchyItemData, build_call_hierarchy_item, build_incoming_hierarchy,
};
use emmylua_parser::{LuaAstNode, LuaTokenKind};
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    ClientCapabilities, ServerCapabilities,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};

use super::RegisterCapabilities;

pub async fn on_prepare_call_hierarchy_handler(
    context: ServerContextSnapshot,
    params: CallHierarchyPrepareParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<CallHierarchyItem>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let model = analysis.semantic_model(file_id)?;
            let document = analysis.salsa.document(file_id)?;
            let root = model.chunk()?;
            let position_offset =
                document.get_offset(position.line as usize, position.character as usize)?;

            if position_offset > root.syntax().text_range().end() {
                return None;
            }

            let token = match root.syntax().token_at_offset(position_offset) {
                TokenAtOffset::Single(token) => token,
                TokenAtOffset::Between(left, right) => {
                    if left.kind() == LuaTokenKind::TkName.into() {
                        left
                    } else {
                        right
                    }
                }
                TokenAtOffset::None => {
                    return None;
                }
            };

            let semantic_decl = model.find_decl(token.into())?;

            Some(vec![build_call_hierarchy_item(
                &model,
                &analysis.salsa,
                &semantic_decl,
            )?])
        },
    )
    .await
}

pub async fn on_incoming_calls_handler(
    context: ServerContextSnapshot,
    params: CallHierarchyIncomingCallsParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<CallHierarchyIncomingCall>> {
    let item = params.item;
    let Some(data) = item.data.as_ref() else {
        return RequestOutcome::Missing;
    };
    let Ok(data) = serde_json::from_value::<CallHierarchyItemData>(data.clone()) else {
        return RequestOutcome::Missing;
    };
    let Some(semantic_decl) = data.semantic_decl.to_semantic_id() else {
        return RequestOutcome::Missing;
    };
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| build_incoming_hierarchy(&analysis.salsa, &semantic_decl),
    )
    .await
}

pub async fn on_outgoing_calls_handler(
    _: ServerContextSnapshot,
    _: CallHierarchyOutgoingCallsParams,
    _: CancellationToken,
) -> RequestOutcome<Vec<CallHierarchyOutgoingCall>> {
    RequestOutcome::Missing
}

pub struct CallHierarchyCapabilities;

impl RegisterCapabilities for CallHierarchyCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.call_hierarchy_provider = Some(true.into());
    }
}
