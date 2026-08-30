use super::RegisterCapabilities;
use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query};
use crate::util::parse_desc;
use emmylua_code_analysis::{DocumentView, Emmyrc, WorkspaceId};
use emmylua_parser::{LuaAstNode, LuaDocDescription};
use lsp_types::{
    ClientCapabilities, SelectionRange, SelectionRangeParams, SelectionRangeProviderCapability,
    ServerCapabilities,
};
use rowan::{TextRange, TextSize, TokenAtOffset};
use tokio_util::sync::CancellationToken;

pub async fn on_document_selection_range_handle(
    context: ServerContextSnapshot,
    params: SelectionRangeParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<SelectionRange>> {
    let uri = params.text_document.uri;
    let position = params.positions;
    let cache_key = format!("selection:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let semantic_model = analysis.semantic_model(file_id)?;
            let document = analysis.salsa.document(file_id)?;
            let root = semantic_model.chunk()?;
            let emmyrc = analysis.get_emmyrc();
            let mut result = Vec::new();
            for pos in &position {
                let offset = document.get_offset(pos.line as usize, pos.character as usize)?;
                let token = match root.syntax().token_at_offset(offset) {
                    TokenAtOffset::Single(token) => token,
                    TokenAtOffset::Between(_, right) => right,
                    TokenAtOffset::None => {
                        return None;
                    }
                };

                let mut ranges = Vec::new();

                let description = token.parent().and_then(LuaDocDescription::cast);
                if let Some(description) = description {
                    add_detail_ranges(&document, &emmyrc, description, offset, &mut ranges);
                } else {
                    let range = token.text_range();
                    ranges.push(range);
                }

                for ancestor in token.parent_ancestors() {
                    let range = ancestor.text_range();
                    ranges.push(range);
                }

                let mut parent: Option<Box<SelectionRange>> = None;
                for range in ranges.into_iter().rev() {
                    let lsp_range = document.to_lsp_range(range)?;
                    let selection_range = SelectionRange {
                        range: lsp_range,
                        parent,
                    };
                    parent = Some(Box::new(selection_range));
                }
                if let Some(selection_range) = parent {
                    result.push(*selection_range);
                }
            }

            Some(result)
        },
    )
    .await
}

fn add_detail_ranges(
    document: &DocumentView,
    emmyrc: &Emmyrc,
    description: LuaDocDescription,
    offset: TextSize,
    result: &mut Vec<TextRange>,
) {
    let text = document.get_text();

    let mut items = parse_desc(WorkspaceId::MAIN, emmyrc, text, description, None);

    items.sort_by_key(|item| item.range.len());

    result.extend(
        items
            .into_iter()
            .map(|item| item.range)
            .filter(|range| range.contains(offset)),
    );
}

pub struct DocumentSelectionRangeCapabilities;

impl RegisterCapabilities for DocumentSelectionRangeCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.selection_range_provider =
            Some(SelectionRangeProviderCapability::Simple(true));
    }
}
