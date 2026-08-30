mod build_hover;
pub(crate) mod desc;
mod keyword_hover;
pub(crate) mod render;

use super::RegisterCapabilities;
use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};
use build_hover::build_semantic_info_hover;
use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::{LuaAstNode, LuaTokenKind};
use keyword_hover::{hover_keyword, is_keyword};
use lsp_types::{
    ClientCapabilities, Hover, HoverContents, HoverParams, HoverProviderCapability, MarkupContent,
    Position, ServerCapabilities,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;
// Old DbIndex-version hover (temporary reuse before batch 3 signature_helper / batch 4 completion migration;

pub async fn on_hover(
    context: ServerContextSnapshot,
    params: HoverParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Hover> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let cache_key = format!("hover:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            hover(analysis, file_id, position)
        },
    )
    .await
}

pub fn hover(analysis: &EmmyLuaAnalysis, file_id: FileId, position: Position) -> Option<Hover> {
    if !analysis.get_emmyrc().hover.enable {
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
            if matches!(
                right.kind().into(),
                LuaTokenKind::TkDot
                    | LuaTokenKind::TkColon
                    | LuaTokenKind::TkLeftBracket
                    | LuaTokenKind::TkRightBracket
                    | LuaTokenKind::TkWhitespace
            ) {
                left
            } else {
                right
            }
        }
        TokenAtOffset::None => return None,
    };
    match token {
        keywords if is_keyword(keywords.clone()) => Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: hover_keyword(keywords.clone()),
            }),
            range: document.to_lsp_range(keywords.text_range()),
        }),
        literal
            if matches!(
                literal.kind().into(),
                LuaTokenKind::TkString | LuaTokenKind::TkLongString
            ) =>
        {
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: literal.text().to_string(),
                }),
                range: document.to_lsp_range(literal.text_range()),
            })
        }
        _ => build_semantic_info_hover(&model, &analysis.salsa, token.clone(), token.text_range()),
    }
}

pub struct HoverCapabilities;

impl RegisterCapabilities for HoverCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.hover_provider = Some(HoverProviderCapability::Simple(true));
    }
}
