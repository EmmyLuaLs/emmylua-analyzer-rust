use std::str::FromStr;

use emmylua_code_analysis::{DiagnosticCode, DocumentView, Emmyrc, FileId, SalsaSemanticModel};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse, Diagnostic,
    NumberOrString, Range, WorkspaceEdit,
};

use super::actions::{
    build_add_doc_tag, build_disable_file_changes, build_disable_next_line_changes,
    build_need_check_nil, build_preferred_local_alias_fix,
};
use crate::handlers::command::{DisableAction, make_disable_code_command};

pub fn build_actions(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    emmyrc: &Emmyrc,
    file_id: FileId,
    diagnostics: Vec<Diagnostic>,
) -> Option<CodeActionResponse> {
    let mut actions = Vec::new();
    for diagnostic in diagnostics {
        if diagnostic.source.is_none() {
            continue;
        }

        let source = diagnostic.source.unwrap();
        if source != "EmmyLua" {
            continue;
        }

        if let Some(code) = diagnostic.code
            && let NumberOrString::String(action_string) = code
            && let Ok(diagnostic_code) = DiagnosticCode::from_str(&action_string)
        {
            add_fix_code_action(
                model,
                document,
                &mut actions,
                diagnostic_code,
                diagnostic.range,
                &diagnostic.data,
            );
            add_disable_code_action(
                model,
                document,
                emmyrc,
                &mut actions,
                diagnostic_code,
                file_id,
                diagnostic.range,
            );
        }
    }

    if actions.is_empty() {
        return None;
    }

    Some(actions)
}

#[allow(unused_variables)]
fn add_fix_code_action(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    actions: &mut Vec<CodeActionOrCommand>,
    diagnostic_code: DiagnosticCode,
    range: Range,
    data: &Option<serde_json::Value>,
) -> Option<()> {
    match diagnostic_code {
        DiagnosticCode::NeedCheckNil => build_need_check_nil(model, document, actions, range, data),
        DiagnosticCode::UnknownDocTag => build_add_doc_tag(actions, data),
        DiagnosticCode::PreferredLocalAlias => {
            build_preferred_local_alias_fix(document, actions, range, data)
        }
        _ => Some(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn add_disable_code_action(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    emmyrc: &Emmyrc,
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
            changes: build_disable_next_line_changes(
                model,
                document,
                emmyrc,
                range.start,
                diagnostic_code,
            ),
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
            changes: build_disable_file_changes(model, document, emmyrc, diagnostic_code),
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
            t!(
                "Disable all diagnostics in current project (%{name})",
                name = diagnostic_code.get_name()
            )
            .as_ref(),
            DisableAction::Project,
            diagnostic_code,
            file_id,
            range,
        )),

        ..Default::default()
    }));

    Some(())
}
