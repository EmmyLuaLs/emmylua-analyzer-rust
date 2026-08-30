mod rename_decl;
mod rename_member;
mod rename_type;

use std::collections::HashMap;

use emmylua_code_analysis::{SalsaDatabase, SalsaSemanticModel, SemanticId};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaComment, LuaDocTagParam, LuaLiteralExpr, LuaSyntaxKind, LuaSyntaxNode,
    LuaSyntaxToken, LuaTokenKind,
};
use lsp_types::{
    ClientCapabilities, OneOf, PrepareRenameResponse, RenameOptions, RenameParams,
    ServerCapabilities, TextDocumentPositionParams, Uri, WorkspaceEdit,
};
use rename_decl::rename_decl_references;
use rename_member::rename_member_references;
use rename_type::rename_type_references;
use rowan::TokenAtOffset;
use tokio_util::sync::CancellationToken;

use crate::context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query};
use crate::handlers::common::type_def_of_id;

use super::RegisterCapabilities;

pub async fn on_rename_handler(
    context: ServerContextSnapshot,
    params: RenameParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<WorkspaceEdit> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let new_name = params.new_name;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            rename(analysis, file_id, position, new_name.clone())
        },
    )
    .await
}

pub async fn on_prepare_rename_handler(
    context: ServerContextSnapshot,
    params: TextDocumentPositionParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<PrepareRenameResponse> {
    let uri = params.text_document.uri;
    let position = params.position;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
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
        },
    )
    .await
}

pub fn rename(
    analysis: &emmylua_code_analysis::EmmyLuaAnalysis,
    file_id: emmylua_code_analysis::FileId,
    position: lsp_types::Position,
    new_name: String,
) -> Option<WorkspaceEdit> {
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

    rename_references(&model, &analysis.salsa, token, new_name)
}

#[allow(clippy::mutable_key_type)]
fn rename_references(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: LuaSyntaxToken,
    new_name: String,
) -> Option<WorkspaceEdit> {
    let mut result: HashMap<Uri, HashMap<lsp_types::Range, String>> = HashMap::new();
    let semantic_decl = match get_target_node(token.clone()) {
        Some(node) => model.find_decl(node.into()),
        None => model.find_decl(token.clone().into()),
    }?;

    match &semantic_decl {
        SemanticId::Decl(_) => {
            rename_decl_references(model, salsa, &semantic_decl, new_name, &mut result);
        }
        SemanticId::Member(_) => {
            rename_member_references(salsa, &semantic_decl, new_name, &mut result);
        }
        SemanticId::TypeDef(_) => {
            if let Some(def) = type_def_of_id(model, &semantic_decl) {
                rename_type_references(salsa, &def, new_name, &mut result);
            }
        }
        _ => {}
    }

    let changes = result
        .into_iter()
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

fn get_target_node(token: LuaSyntaxToken) -> Option<LuaSyntaxNode> {
    let parent = token.parent()?;
    match parent.kind().into() {
        LuaSyntaxKind::LiteralExpr => {
            let literal_expr = LuaLiteralExpr::cast(parent)?;
            // Integer/string keys for table fields `[1] = v` / index keys `t[1]`:
            // target = the literal itself (member key range hit); otherwise target = literal parent node.
            let parent_kind = literal_expr.syntax().parent().map(|p| p.kind().into());
            if matches!(
                parent_kind,
                Some(LuaSyntaxKind::TableFieldAssign)
                    | Some(LuaSyntaxKind::TableFieldValue)
                    | Some(LuaSyntaxKind::IndexExpr)
            ) {
                return Some(literal_expr.syntax().clone());
            }
            literal_expr.syntax().parent()
        }
        LuaSyntaxKind::DocTagParam => {
            let doc_tag_param = LuaDocTagParam::cast(parent)?;
            let name = doc_tag_param.get_name_token()?;
            let name_text = name.get_name_text();
            let comment = doc_tag_param.get_parent::<LuaComment>()?;
            let owner = comment.get_owner()?;
            match owner {
                LuaAst::LuaLocalFuncStat(local_func_stat) => {
                    let closure_expr = local_func_stat.get_closure()?;
                    let param_list = closure_expr.get_params_list()?;
                    let param_name = param_list.get_params().find(|param| {
                        if let Some(name_token) = param.get_name_token() {
                            name_token.get_name_text() == name_text
                        } else {
                            false
                        }
                    })?;
                    Some(param_name.syntax().clone())
                }
                LuaAst::LuaFuncStat(func_stat) => {
                    let closure_expr = func_stat.get_closure()?;
                    let param_list = closure_expr.get_params_list()?;
                    let param_name = param_list.get_params().find(|param| {
                        if let Some(name_token) = param.get_name_token() {
                            name_token.get_name_text() == name_text
                        } else {
                            false
                        }
                    })?;
                    Some(param_name.syntax().clone())
                }
                _ => None,
            }
        }
        _ => None,
    }
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
