use std::str::FromStr;

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaBlock, LuaChunk, LuaDocTagDiagnostic};
use rowan::TextRange;

use crate::{
    DiagnosticCode,
    db_index::{DiagnosticAction, DiagnosticActionKind},
};

use super::DocAnalyzer;

pub fn analyze_diagnostic(
    analyzer: &mut DocAnalyzer,
    diagnostic: LuaDocTagDiagnostic,
) -> Option<()> {
    let token = diagnostic.get_action_token()?;
    let action = token.get_text();
    match action {
        "disable" => analyze_diagnostic_disable(analyzer, diagnostic)?,
        "disable-next-line" => analyze_diagnostic_disable_next_line(analyzer, diagnostic)?,
        "disable-line" => analyze_diagnostic_disable_line(analyzer, diagnostic)?,
        "enable" => analyze_diagnostic_enable(analyzer, diagnostic)?,
        _ => {}
    };

    Some(())
}

fn analyze_diagnostic_disable(
    analyzer: &mut DocAnalyzer,
    diagnostic: LuaDocTagDiagnostic,
) -> Option<()> {
    let comment = analyzer.comment.clone();
    let owner_block = comment.ancestors::<LuaBlock>().next()?;
    let owner_block_range = owner_block.get_range();
    let is_file_disable = owner_block.get_parent::<LuaChunk>().is_some();
    let file_id = analyzer.file_id;

    let diagnostic_index = analyzer.get_db().get_diagnostic_index_mut();
    if let Some(diagnostic_code_list) = diagnostic.get_code_list() {
        for code in diagnostic_code_list.get_codes() {
            let name = code.get_name_text();
            let diagnostic_code = if let Ok(code) = DiagnosticCode::from_str(name) {
                code
            } else {
                continue;
            };

            if is_file_disable {
                diagnostic_index.add_file_diagnostic_disabled(file_id, diagnostic_code);
            } else {
                diagnostic_index.add_diagnostic_action(
                    file_id,
                    DiagnosticAction::new(
                        owner_block_range,
                        DiagnosticActionKind::Disable(diagnostic_code),
                    ),
                );
            }
        }
    } else {
        diagnostic_index.add_diagnostic_action(
            file_id,
            DiagnosticAction::new(owner_block_range, DiagnosticActionKind::DisableAll),
        );
    }

    Some(())
}

fn analyze_diagnostic_disable_next_line(
    analyzer: &mut DocAnalyzer,
    diagnostic: LuaDocTagDiagnostic,
) -> Option<()> {
    let comment = analyzer.comment.clone();
    let comment_range = comment.get_range();
    let file_id = analyzer.file_id;
    let document = analyzer.get_db().get_vfs().get_document(&file_id)?;
    let comment_end_line = document.get_line(comment_range.end())?;
    let line_range = document.get_line_range(comment_end_line + 1)?;
    let valid_range = TextRange::new(comment_range.start(), line_range.end());

    let diagnostic_index = analyzer.get_db().get_diagnostic_index_mut();
    if let Some(diagnostic_code_list) = diagnostic.get_code_list() {
        for code in diagnostic_code_list.get_codes() {
            let name = code.get_name_text();
            let diagnostic_code = if let Ok(code) = DiagnosticCode::from_str(name) {
                code
            } else {
                continue;
            };

            diagnostic_index.add_diagnostic_action(
                file_id,
                DiagnosticAction::new(valid_range, DiagnosticActionKind::Disable(diagnostic_code)),
            );
        }
    } else {
        diagnostic_index.add_diagnostic_action(
            file_id,
            DiagnosticAction::new(valid_range, DiagnosticActionKind::DisableAll),
        );
    }

    Some(())
}

fn analyze_diagnostic_disable_line(
    analyzer: &mut DocAnalyzer,
    diagnostic: LuaDocTagDiagnostic,
) -> Option<()> {
    let comment = analyzer.comment.clone();
    let comment_range = comment.get_range();
    let file_id = analyzer.file_id;
    let document = analyzer.get_db().get_vfs().get_document(&file_id)?;
    let comment_end_line = document.get_line(comment_range.end())?;
    let valid_range = document.get_line_range(comment_end_line)?;

    let diagnostic_index = analyzer.get_db().get_diagnostic_index_mut();
    if let Some(diagnostic_code_list) = diagnostic.get_code_list() {
        for code in diagnostic_code_list.get_codes() {
            let name = code.get_name_text();
            let diagnostic_code = if let Ok(code) = DiagnosticCode::from_str(name) {
                code
            } else {
                continue;
            };

            diagnostic_index.add_diagnostic_action(
                file_id,
                DiagnosticAction::new(valid_range, DiagnosticActionKind::Disable(diagnostic_code)),
            );
        }
    } else {
        diagnostic_index.add_diagnostic_action(
            file_id,
            DiagnosticAction::new(valid_range, DiagnosticActionKind::DisableAll),
        );
    }

    Some(())
}

fn analyze_diagnostic_enable(
    analyzer: &mut DocAnalyzer,
    diagnostic: LuaDocTagDiagnostic,
) -> Option<()> {
    let file_id = analyzer.file_id;
    let diagnostic_index = analyzer.get_db().get_diagnostic_index_mut();
    let diagnostic_code_list = diagnostic.get_code_list()?;
    for code in diagnostic_code_list.get_codes() {
        let name = code.get_name_text();
        let diagnostic_code = if let Ok(code) = DiagnosticCode::from_str(name) {
            code
        } else {
            continue;
        };

        diagnostic_index.add_file_diagnostic_enabled(file_id, diagnostic_code);
    }

    Some(())
}
