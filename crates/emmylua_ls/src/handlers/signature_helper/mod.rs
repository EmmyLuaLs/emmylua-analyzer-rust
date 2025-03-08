mod build_signature_helper;

use crate::context::ServerContextSnapshot;
use build_signature_helper::build_signature_helper;
pub use build_signature_helper::get_current_param_index;
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaSyntaxKind, LuaTokenKind};
use lsp_types::{
    ClientCapabilities, ServerCapabilities, SignatureHelp, SignatureHelpContext,
    SignatureHelpOptions, SignatureHelpParams, SignatureHelpTriggerKind,
};
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

pub async fn on_signature_helper_handler(
    context: ServerContextSnapshot,
    params: SignatureHelpParams,
    _: CancellationToken,
) -> Option<SignatureHelp> {
    let uri = params.text_document_position_params.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let mut semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    let root = semantic_model.get_root();
    let position = params.text_document_position_params.position;
    let position_offset = {
        let document = semantic_model.get_document();
        document.get_offset(position.line as usize, position.character as usize)?
    };

    if position_offset > root.syntax().text_range().end() {
        return None;
    }

    let token = match root.syntax().token_at_offset(position_offset) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, _) => left,
        TokenAtOffset::None => {
            return None;
        }
    };

    let param_context = params.context.unwrap_or(SignatureHelpContext {
        trigger_kind: SignatureHelpTriggerKind::INVOKED,
        trigger_character: None,
        is_retrigger: false,
        active_signature_help: None,
    });

    if !param_context.is_retrigger {
        let node = token.parent()?;
        match node.kind().into() {
            LuaSyntaxKind::CallArgList => {
                let call_expr = LuaCallExpr::cast(node.parent()?)?;
                build_signature_helper(&mut semantic_model, call_expr, token)
            }
            // todo
            LuaSyntaxKind::TypeGeneric | LuaSyntaxKind::DocTypeList => None,
            _ => None,
        }
    } else if matches!(
        token.kind().into(),
        LuaTokenKind::TkWhitespace | LuaTokenKind::TkEndOfLine
    ) {
        if token.parent()?.kind() == LuaSyntaxKind::CallArgList.into() {
            param_context.active_signature_help
        } else {
            None
        }
    } else {
        let node = token.parent_ancestors().find(|node| {
            matches!(
                node.kind().into(),
                LuaSyntaxKind::CallArgList
                    | LuaSyntaxKind::TypeGeneric
                    | LuaSyntaxKind::DocTypeList
            )
        })?;
        match node.kind().into() {
            LuaSyntaxKind::CallArgList => {
                let call_expr = LuaCallExpr::cast(node.parent()?)?;
                build_signature_helper(&mut semantic_model, call_expr, token)
            }
            // todo
            LuaSyntaxKind::TypeGeneric | LuaSyntaxKind::DocTypeList => None,
            _ => None,
        }
    }
}

pub fn register_capabilities(
    server_capabilities: &mut ServerCapabilities,
    _: &ClientCapabilities,
) -> Option<()> {
    server_capabilities.signature_help_provider = Some(SignatureHelpOptions {
        trigger_characters: Some(vec!["(", ","].iter().map(|s| s.to_string()).collect()),
        retrigger_characters: Some(vec!["(", ","].iter().map(|s| s.to_string()).collect()),
        ..Default::default()
    });
    Some(())
}
