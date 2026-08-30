mod reference_searcher;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::{LuaAstNode, LuaTokenKind};
use lsp_types::{
    ClientCapabilities, Location, OneOf, Position, ReferenceParams, ServerCapabilities,
};
use reference_searcher::search_references;
// Legacy DbIndex reference search (temporarily reused before the M3 batch 3 migration of call_hierarchy / implementation / code_lens;
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;

pub async fn on_references_handler(
    context: ServerContextSnapshot,
    params: ReferenceParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<Location>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let include_declaration = params.context.include_declaration;
    let cache_key = format!("references:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            references(analysis, file_id, position, include_declaration)
        },
    )
    .await
}

pub fn references(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    if !analysis.get_emmyrc().references.enable {
        return None;
    }
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

    search_references(&model, &analysis.salsa, token, include_declaration)
}

pub struct ReferencesCapabilities;

impl RegisterCapabilities for ReferencesCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.references_provider = Some(OneOf::Left(true));
    }
}
