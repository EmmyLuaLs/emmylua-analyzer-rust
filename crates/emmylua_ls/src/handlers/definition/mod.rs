mod goto_def_definition;
mod goto_doc_see;
mod goto_function;
mod goto_module_file;
mod goto_path;

use emmylua_code_analysis::{EmmyLuaAnalysis, FileId, SemanticDeclLevel, WorkspaceId};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocDescription, LuaDocTagSee, LuaGeneralToken, LuaStringToken,
    LuaTokenKind,
};
pub use goto_def_definition::goto_def_definition;
use goto_def_definition::goto_str_tpl_ref_definition;
pub use goto_doc_see::goto_doc_see;
pub use goto_function::compare_function_types;
pub use goto_module_file::goto_module_file;
use lsp_types::{
    ClientCapabilities, GotoDefinitionParams, GotoDefinitionResponse, OneOf, Position,
    ServerCapabilities,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use super::RegisterCapabilities;
use crate::context::ServerContextSnapshot;
use crate::handlers::definition::goto_path::goto_path;
use crate::util::find_ref_at;
pub use goto_function::extract_semantic_decl_from_signature;

pub async fn on_goto_definition_handler(
    context: ServerContextSnapshot,
    params: GotoDefinitionParams,
    _: CancellationToken,
) -> Option<GotoDefinitionResponse> {
    let uri = params.text_document_position_params.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let position = params.text_document_position_params.position;

    definition(&analysis, file_id, position)
}

pub fn definition(
    analysis: &EmmyLuaAnalysis,
    file_id: FileId,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
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
            } else if left.kind() == LuaTokenKind::TkLeftBracket.into()
                && right.kind() == LuaTokenKind::TkInt.into()
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

    if let Some(semantic_decl) =
        semantic_model.find_decl(token.clone().into(), SemanticDeclLevel::default())
    {
        return goto_def_definition(
            &semantic_model,
            &analysis.compilation,
            semantic_decl,
            &token,
        );
    } else if let Some(string_token) = LuaStringToken::cast(token.clone()) {
        if let Some(module_response) = goto_module_file(&semantic_model, string_token.clone()) {
            return Some(module_response);
        }
        if let Some(str_tpl_ref_response) =
            goto_str_tpl_ref_definition(&semantic_model, string_token)
        {
            return Some(str_tpl_ref_response);
        }
    } else if token.kind() == LuaTokenKind::TkDocSeeContent.into() {
        let general_token = LuaGeneralToken::cast(token.clone())?;
        if let Some(_) = general_token.get_parent::<LuaDocTagSee>() {
            return goto_doc_see(&semantic_model, general_token);
        }
    } else if token.kind() == LuaTokenKind::TkDocDetail.into() {
        let parent = token.parent()?;
        let description = LuaDocDescription::cast(parent)?;
        let document = semantic_model.get_document();

        let path = find_ref_at(
            semantic_model
                .get_module()
                .map(|m| m.workspace_id)
                .unwrap_or(WorkspaceId::MAIN),
            semantic_model.get_emmyrc(),
            document.get_text(),
            description.clone(),
            position_offset,
        )?;

        return goto_path(&semantic_model, &analysis.compilation, &path, &token);
    }

    None
}

pub struct DefinitionCapabilities;

impl RegisterCapabilities for DefinitionCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.definition_provider = Some(OneOf::Left(true));
    }
}
