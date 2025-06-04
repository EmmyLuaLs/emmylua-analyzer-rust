mod build_hover;
mod function_humanize;
mod hover_builder;
mod hover_humanize;
mod keyword_hover;
mod member_origin;
mod std_hover;

pub use build_hover::build_hover_content_for_completion;
use build_hover::build_semantic_info_hover;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::LuaAstNode;
pub use hover_builder::HoverBuilder;
pub use hover_humanize::infer_prefix_global_name;
use keyword_hover::{hover_keyword, is_keyword};
use lsp_types::{
    ClientCapabilities, Hover, HoverContents, HoverParams, HoverProviderCapability, MarkupContent,
    Position, ServerCapabilities,
};
pub use member_origin::find_member_origin_owner;
use rowan::TokenAtOffset;
pub use std_hover::{hover_std_description, is_std};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

use super::RegisterCapabilities;

pub async fn on_hover(
    context: ServerContextSnapshot,
    params: HoverParams,
    _: CancellationToken,
) -> Option<Hover> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    hover(&analysis, file_id, position)
}

pub fn hover(analysis: &EmmyLuaAnalysis, file_id: FileId, position: Position) -> Option<Hover> {
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    if !semantic_model.get_emmyrc().hover.enable {
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
        TokenAtOffset::Between(_, right) => right,
        TokenAtOffset::None => {
            return None;
        }
    };
    match token {
        keywords if is_keyword(keywords.clone()) => {
            let document = semantic_model.get_document();
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: hover_keyword(keywords.clone()),
                }),
                range: document.to_lsp_range(keywords.text_range()),
            });
        }
        _ => {
            let semantic_info = semantic_model.get_semantic_info(token.clone().into())?;
            let db = semantic_model.get_db();
            let document = semantic_model.get_document();
            build_semantic_info_hover(&semantic_model, db, &document, token, semantic_info)
        }
    }
}

pub struct HoverCapabilities;

impl RegisterCapabilities for HoverCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.hover_provider = Some(HoverProviderCapability::Simple(true));
    }
}
