mod rename_decl;
mod rename_member;
mod rename_type;

use std::collections::HashMap;

use emmylua_code_analysis::{LuaCompilation, LuaSemanticDeclId, SemanticDeclLevel, SemanticModel};
use emmylua_parser::{LuaAstNode, LuaLiteralExpr, LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind};
use lsp_types::{
    ClientCapabilities, OneOf, PrepareRenameResponse, RenameOptions, RenameParams,
    ServerCapabilities, TextDocumentPositionParams, WorkspaceEdit,
};
use rename_decl::rename_decl_references;
use rename_member::rename_member_references;
use rename_type::rename_type_references;
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

use super::RegisterCapabilities;

pub async fn on_rename_handler(
    context: ServerContextSnapshot,
    params: RenameParams,
    _: CancellationToken,
) -> Option<WorkspaceEdit> {
    let uri = params.text_document_position.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let position = params.text_document_position.position;
    rename(&analysis, file_id, position, params.new_name)
}

pub async fn on_prepare_rename_handler(
    context: ServerContextSnapshot,
    params: TextDocumentPositionParams,
    _: CancellationToken,
) -> Option<PrepareRenameResponse> {
    let uri = params.text_document.uri;
    let analysis = context.analysis.read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let position = params.position;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    let root = semantic_model.get_root();
    let document = semantic_model.get_document();
    let position_offset =
        document.get_offset(position.line as usize, position.character as usize)?;

    if position_offset > root.syntax().text_range().end() {
        return None;
    }

    let token = match root.syntax().token_at_offset(position_offset) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, right) => {
            if left.kind() == LuaTokenKind::TkName.into()
                || left.kind() == LuaTokenKind::TkInt.into()
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
    if matches!(
        token.kind().into(),
        LuaTokenKind::TkName | LuaTokenKind::TkInt | LuaTokenKind::TkString
    ) {
        let range = document.to_lsp_range(token.text_range())?;
        let placeholder = token.text().to_string();
        Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder })
    } else {
        None
    }
}

pub fn rename(
    analysis: &emmylua_code_analysis::EmmyLuaAnalysis,
    file_id: emmylua_code_analysis::FileId,
    position: lsp_types::Position,
    new_name: String,
) -> Option<WorkspaceEdit> {
    let mut semantic_model = analysis.compilation.get_semantic_model(file_id)?;
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

    rename_references(&mut semantic_model, &analysis.compilation, token, new_name)
}

fn rename_references(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    token: LuaSyntaxToken,
    new_name: String,
) -> Option<WorkspaceEdit> {
    let mut result = HashMap::new();
    let semantic_decl = match get_literal_expr_parent(token.clone()) {
        Some(node) => semantic_model.find_decl(node.into(), SemanticDeclLevel::NoTrace),
        None => semantic_model.find_decl(token.into(), SemanticDeclLevel::NoTrace),
    }?;

    match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            rename_decl_references(semantic_model, compilation, decl_id, new_name, &mut result);
        }
        LuaSemanticDeclId::Member(member_id) => {
            rename_member_references(
                semantic_model,
                compilation,
                member_id,
                new_name,
                &mut result,
            );
        }
        LuaSemanticDeclId::TypeDecl(type_decl_id) => {
            rename_type_references(semantic_model, type_decl_id, new_name, &mut result);
        }
        _ => {}
    }

    let changes = result
        .into_iter()
        .filter(|(uri, _)| {
            if let Some(file_id) = semantic_model.get_db().get_vfs().get_file_id(uri) {
                !semantic_model.get_db().get_module_index().is_std(&file_id)
            } else {
                true
            }
        })
        .map(|(uri, ranges)| {
            let text_edits = ranges
                .into_iter()
                .map(|(range, new_text)| lsp_types::TextEdit { range, new_text })
                .collect();
            (uri, text_edits)
        })
        .collect();

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

fn get_literal_expr_parent(token: LuaSyntaxToken) -> Option<LuaSyntaxNode> {
    let parent = token.parent()?;
    let literal_expr = LuaLiteralExpr::cast(parent)?;
    literal_expr.syntax().parent()
}

pub struct RenameCapabilities;

impl RegisterCapabilities for RenameCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.rename_provider = Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        }));
    }
}
