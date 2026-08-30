use std::collections::HashMap;

use crate::handlers::command::make_auto_doc_tag_command;
use emmylua_code_analysis::{DocumentView, SalsaSemanticModel};
use emmylua_parser::{LuaAstNode, LuaExpr};
use lsp_types::{CodeAction, CodeActionKind, CodeActionOrCommand, Range, TextEdit, WorkspaceEdit};
use rowan::{NodeOrToken, TokenAtOffset};

pub fn build_need_check_nil(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    actions: &mut Vec<CodeActionOrCommand>,
    range: Range,
    _data: &Option<serde_json::Value>,
) -> Option<()> {
    let offset = document.get_offset(range.end.line as usize, range.end.character as usize)?;
    let root = model.chunk()?;
    let token = match root.syntax().token_at_offset(offset) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(_, token) => token,
        _ => return None,
    };
    // Get the parent node of the previous token
    let node_or_token = token.prev_sibling_or_token()?;
    if let NodeOrToken::Node(expr_node) = node_or_token
        && LuaExpr::can_cast(expr_node.kind().into())
    {
        let expr = LuaExpr::cast(expr_node)?;
        let range = expr.syntax().text_range();
        let mut lsp_range = document.to_lsp_range(range)?;
        // Narrow the range to the last character
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
                changes: Some(HashMap::from([(document.get_uri()?, vec![text_edit])])),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    Some(())
}

pub fn build_add_doc_tag(
    actions: &mut Vec<CodeActionOrCommand>,
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
            t!("Add @%{name} to the list of known tags", name = tag_name).as_ref(),
            tag_name,
        )),

        ..Default::default()
    }));

    Some(())
}

pub fn build_preferred_local_alias_fix(
    document: &DocumentView,
    actions: &mut Vec<CodeActionOrCommand>,
    range: Range,
    data: &Option<serde_json::Value>,
) -> Option<()> {
    let alias_name = data.as_ref()?.get("preferredAlias")?.as_str()?;
    let text_edit = TextEdit {
        range,
        new_text: alias_name.to_string(),
    };

    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
        title: t!("Replace with local alias '%{name}'", name = alias_name).to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        edit: Some(WorkspaceEdit {
            changes: Some(HashMap::from([(document.get_uri()?, vec![text_edit])])),
            ..Default::default()
        }),
        ..Default::default()
    }));

    Some(())
}
