use std::str::FromStr;

use emmylua_code_analysis::{DiagnosticCode, FileId, SemanticModel};
use lsp_types::{CodeActionOrCommand, CodeActionResponse, Diagnostic, NumberOrString, Range};

use crate::handlers::command::{make_disable_code_command, DisableAction};

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
                    add_fix_code_action(&mut actions, diagnostic_code, file_id, diagnostic.range);
                    add_disable_code_action(
                        &mut actions,
                        diagnostic_code,
                        file_id,
                        diagnostic.range,
                    );
                }
            }
        }
    }

    Some(actions)
}

#[allow(unused_variables)]
fn add_fix_code_action(
    actions: &mut Vec<CodeActionOrCommand>,
    diagnostic_code: DiagnosticCode,
    file_id: FileId,
    range: Range,
) -> Option<()> {
    Some(())
}

fn add_disable_code_action(
    actions: &mut Vec<CodeActionOrCommand>,
    diagnostic_code: DiagnosticCode,
    file_id: FileId,
    range: Range,
) -> Option<()> {
    // LuaSyntaxError no need to disable
    if diagnostic_code == DiagnosticCode::LuaSyntaxError {
        return Some(());
    }
    actions.push(CodeActionOrCommand::Command(make_disable_code_command(
        &t!(
            "Disable current line diagnostic (%{name})",
            name = diagnostic_code.get_name()
        )
        .to_string(),
        DisableAction::DisableLine,
        diagnostic_code,
        file_id,
        range,
    )));

    actions.push(CodeActionOrCommand::Command(make_disable_code_command(
        &t!(
            "Disable all diagnostics in current file (%{name})",
            name = diagnostic_code.get_name()
        )
        .to_string(),
        DisableAction::DisableFile,
        diagnostic_code,
        file_id,
        range,
    )));

    actions.push(CodeActionOrCommand::Command(make_disable_code_command(
        &t!(
            "Disable all diagnostics in current project (%{name})",
            name = diagnostic_code.get_name()
        )
        .to_string(),
        DisableAction::DisableProject,
        diagnostic_code,
        file_id,
        range,
    )));

    Some(())
}
