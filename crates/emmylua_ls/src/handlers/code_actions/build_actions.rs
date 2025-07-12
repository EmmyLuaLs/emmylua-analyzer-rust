use std::str::FromStr;

use emmylua_code_analysis::{DiagnosticCode, FileId, SemanticModel};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse, Diagnostic,
    NumberOrString, Range, WorkspaceEdit,
};

use super::actions::{
    build_add_doc_tag, build_disable_file_changes, build_disable_next_line_changes,
};
use crate::handlers::{
    code_actions::actions::build_need_check_nil,
    command::{make_disable_code_command, DisableAction},
};

pub fn build_actions(
    semantic_model: &SemanticModel,
    diagnostics: Vec<Diagnostic>,
) -> Option<CodeActionResponse> {
    let mut actions = Vec::new();
    let file_id = semantic_model.get_file_id();
    for diagnostic in diagnostics {
        if diagnostic.source.is_none() {
            continue;
        }

        let source = diagnostic.source.unwrap();
        if source != "EmmyLua" {
            continue;
        }

        if let Some(code) = diagnostic.code {
            if let NumberOrString::String(action_string) = code {
                if let Some(diagnostic_code) = DiagnosticCode::from_str(&action_string).ok() {
                    add_fix_code_action(
                        &semantic_model,
                        &mut actions,
                        diagnostic_code,
                        file_id,
                        diagnostic.range,
                        &diagnostic.data,
                    );
                    add_disable_code_action(
                        &semantic_model,
                        &mut actions,
                        diagnostic_code,
                        file_id,
                        diagnostic.range,
                    );
                }
            }
        }
    }

    if actions.is_empty() {
        return None;
    }

    Some(actions)
}

#[allow(unused_variables)]
fn add_fix_code_action(
    semantic_model: &SemanticModel,
    actions: &mut Vec<CodeActionOrCommand>,
    diagnostic_code: DiagnosticCode,
    file_id: FileId,
    range: Range,
    data: &Option<serde_json::Value>,
) -> Option<()> {
    match diagnostic_code {
        DiagnosticCode::NeedCheckNil => build_need_check_nil(semantic_model, actions, range, data),
        DiagnosticCode::UnknownDocTag => build_add_doc_tag(semantic_model, actions, range, data),
        _ => Some(()),
    }
}

fn add_disable_code_action(
    semantic_model: &SemanticModel,
    actions: &mut Vec<CodeActionOrCommand>,
    diagnostic_code: DiagnosticCode,
    file_id: FileId,
    range: Range,
) -> Option<()> {
    // LuaSyntaxError no need to disable
    if diagnostic_code == DiagnosticCode::SyntaxError {
        return Some(());
    }

    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
        title: t!(
            "Disable current line diagnostic (%{name})",
            name = diagnostic_code.get_name()
        )
        .to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        edit: Some(WorkspaceEdit {
            changes: build_disable_next_line_changes(semantic_model, range.start, diagnostic_code),
            ..Default::default()
        }),
        ..Default::default()
    }));

    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
        title: t!(
            "Disable all diagnostics in current file (%{name})",
            name = diagnostic_code.get_name()
        )
        .to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        edit: Some(WorkspaceEdit {
            changes: build_disable_file_changes(semantic_model, diagnostic_code),
            ..Default::default()
        }),
        ..Default::default()
    }));

    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
        title: t!(
            "Disable all diagnostics in current project (%{name})",
            name = diagnostic_code.get_name()
        )
        .to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        command: Some(make_disable_code_command(
            &t!(
                "Disable all diagnostics in current project (%{name})",
                name = diagnostic_code.get_name()
            )
            .to_string(),
            DisableAction::DisableProject,
            diagnostic_code,
            file_id,
            range,
        )),

        ..Default::default()
    }));

    Some(())
}
