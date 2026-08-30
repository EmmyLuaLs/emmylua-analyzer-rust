mod goto_def_definition;
mod goto_label;
mod goto_module_file;
mod goto_string;

use emmylua_code_analysis::{EmmyLuaAnalysis, FileId};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaStringToken, LuaTokenKind};
use lsp_types::{
    ClientCapabilities, GotoDefinitionParams, GotoDefinitionResponse, OneOf, Position,
    ServerCapabilities,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;
use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};

pub async fn on_goto_definition_handler(
    context: ServerContextSnapshot,
    params: GotoDefinitionParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<GotoDefinitionResponse> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            definition(analysis, file_id, position)
        },
    )
    .await
}

pub fn definition(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    position: Position,
) -> Option<GotoDefinitionResponse> {
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
            if left.kind() == LuaTokenKind::TkName.into()
                || (left.kind() == LuaTokenKind::TkLeftBracket.into()
                    && right.kind() == LuaTokenKind::TkInt.into())
            {
                left
            } else {
                right
            }
        }
        TokenAtOffset::None => {
            return None;
        }
    };

    // 1. Label definition (goto / label).
    if let Some(response) = goto_label::goto_label_definition(&model, &analysis.salsa, &token) {
        return Some(response);
    }

    // 2. String token: require module file / string template reference.
    if let Some(string_token) = LuaStringToken::cast(token.clone()) {
        if let Some(response) =
            goto_module_file::goto_module_file(&analysis.salsa, string_token.clone())
        {
            return Some(response);
        }
        if let Some(response) =
            goto_string::goto_str_tpl_ref_definition(&model, &analysis.salsa, string_token)
        {
            return Some(response);
        }
        return None;
    }

    // 2.5 Doc description reference / `@see`.
    if let Some(response) = goto_def_definition::goto_doc_definition(
        &model,
        &analysis.salsa,
        &token,
        position_offset,
        &analysis.get_emmyrc(),
    ) {
        return Some(response);
    }

    // 3. Semantic declarations (decl / member / typedef).
    let decl = model.find_decl(token.clone().into())?;
    goto_def_definition::goto_def_definition(&model, &analysis.salsa, &decl, &token)
}

pub struct DefinitionCapabilities;

impl RegisterCapabilities for DefinitionCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.definition_provider = Some(OneOf::Left(true));
    }
}
