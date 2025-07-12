use std::collections::HashMap;

use crate::handlers::command::make_auto_doc_tag_command;
use emmylua_code_analysis::SemanticModel;
use emmylua_parser::{LuaAstNode, LuaExpr};
use lsp_types::{CodeAction, CodeActionKind, CodeActionOrCommand, Range, TextEdit, WorkspaceEdit};
use rowan::{NodeOrToken, TokenAtOffset};

pub fn build_need_check_nil(
    semantic_model: &SemanticModel,
    actions: &mut Vec<CodeActionOrCommand>,
    range: Range,
    _data: &Option<serde_json::Value>,
) -> Option<()> {
    let document = semantic_model.get_document();
    let offset = document.get_offset(range.end.line as usize, range.end.character as usize)?;
    let root = semantic_model.get_root();
    let token = match root.syntax().token_at_offset(offset.into()) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(_, token) => token,
        _ => return None,
    };
    // 取上一个token的父节点
    let node_or_token = token.prev_sibling_or_token()?;
    match node_or_token {
        NodeOrToken::Node(node) => match node {
            expr_node if LuaExpr::can_cast(expr_node.kind().into()) => {
                let expr = LuaExpr::cast(expr_node)?;
                let range = expr.syntax().text_range();
                let mut lsp_range = document.to_lsp_range(range)?;
                // 将范围缩小到最尾部的字符
                lsp_range.start.line = lsp_range.end.line;
                lsp_range.start.character = lsp_range.end.character;

                let text_edit = TextEdit {
                    range: lsp_range,
                    new_text: "--[[@cast -?]]".to_string(),
                };

                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: t!("use cast to remove nil").to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(document.get_uri(), vec![text_edit])])),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
            _ => {}
        },
        _ => {}
    }

    Some(())
}

pub fn build_add_doc_tag(
    _semantic_model: &SemanticModel,
    actions: &mut Vec<CodeActionOrCommand>,
    _range: Range,
    data: &Option<serde_json::Value>,
) -> Option<()> {
    let Some(data) = data else {
        return None;
    };

    let tag_name = data.as_str()?;
    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
        title: t!("Add @%{name} to the list of known tags", name = tag_name).to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        command: Some(make_auto_doc_tag_command(
            &t!("Add @%{name} to the list of known tags", name = tag_name).to_string(),
            tag_name,
        )),

        ..Default::default()
    }));

    Some(())
}
