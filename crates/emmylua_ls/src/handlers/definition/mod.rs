mod goto_def_definition;
mod goto_doc_see;
mod goto_function;
mod goto_module_file;

use emmylua_code_analysis::{EmmyLuaAnalysis, FileId, SemanticDeclLevel};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaDocTagSee, LuaGeneralToken, LuaStringToken, LuaTokenKind,
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

use crate::context::ServerContextSnapshot;
pub use goto_function::extract_semantic_decl_from_signature;

use super::RegisterCapabilities;

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
    }

    // goto self
    let document = semantic_model.get_document();
    let lsp_location = document.to_lsp_location(token.text_range())?;
    Some(GotoDefinitionResponse::Scalar(lsp_location))
}

pub struct DefinitionCapabilities;

impl RegisterCapabilities for DefinitionCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.definition_provider = Some(OneOf::Left(true));
    }
}
