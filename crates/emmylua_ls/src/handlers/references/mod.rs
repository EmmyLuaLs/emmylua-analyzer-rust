mod reference_searcher;

use crate::context::ServerContextSnapshot;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::{LuaAstNode, LuaTokenKind};
use lsp_types::{
    ClientCapabilities, Location, OneOf, Position, ReferenceParams, ServerCapabilities,
};
use reference_searcher::search_references;
pub use reference_searcher::{search_decl_references, search_member_references};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;

pub async fn on_references_handler(
    context: ServerContextSnapshot,
    params: ReferenceParams,
    _: CancellationToken,
) -> Option<Vec<Location>> {
    let uri = params.text_document_position.text_document.uri;
    let analysis = context.analysis().read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let position = params.text_document_position.position;

    references(&analysis, file_id, position)
}

pub fn references(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    position: Position,
) -> Option<Vec<Location>> {
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    if !semantic_model.get_emmyrc().references.enable {
        return None;
    }

    let root = semantic_model.get_root();
    let position_offset = {
        let document = semantic_model.get_document();
        document.get_offset(position.line as usize, position.character as usize)?
    };

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

    search_references(&semantic_model, &analysis.compilation, token)
}

pub struct ReferencesCapabilities;

impl RegisterCapabilities for ReferencesCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.references_provider = Some(OneOf::Left(true));
    }
}
