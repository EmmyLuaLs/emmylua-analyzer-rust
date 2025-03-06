use std::{collections::HashMap, fs::OpenOptions, io::Write, sync::Arc, time::Duration};

use emmylua_code_analysis::{load_configs, DiagnosticCode, FileId, SemanticModel};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaComment, LuaCommentOwner, LuaDocTag, LuaDocTagDiagnostic, LuaStat,
    LuaTokenKind,
};
use lsp_types::{ApplyWorkspaceEditParams, Command, Position, Range, TextEdit, WorkspaceEdit};
use rowan::TokenAtOffset;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{
    context::{ClientProxy, WorkspaceManager, ServerContextSnapshot},
    util::time_cancel_token,
};

pub const COMMAND: &str = "emmy.disable.code";
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisableAction {
    DisableLine,
    DisableFile,
    DisableProject,
}

pub async fn handle(context: ServerContextSnapshot, args: Vec<Value>) -> Option<()> {
    let action: DisableAction = serde_json::from_value(args.get(0)?.clone()).ok()?;
    let file_id: FileId = serde_json::from_value(args.get(1)?.clone()).ok()?;
    let range: Range = serde_json::from_value(args.get(2)?.clone()).ok()?;
    let code: DiagnosticCode = serde_json::from_value(args.get(3)?.clone()).ok()?;

    let analysis = context.analysis.read().await;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    let client = context.client;
    match action {
        DisableAction::DisableLine => {
            let start = range.start;
            add_disable_next_line_comment(client, semantic_model, start, code);
        }
        DisableAction::DisableFile => {
            add_disable_file_comment(client, semantic_model, code);
        }
        DisableAction::DisableProject => {
            add_disable_project(context.workspace_manager, code).await;
        }
    }

    Some(())
}

fn add_disable_next_line_comment(
    client: Arc<ClientProxy>,
    semantic_model: SemanticModel<'_>,
    start: Position,
    code: DiagnosticCode,
) -> Option<()> {
    let document = semantic_model.get_document();
    let offset = document.get_offset(start.line as usize, start.character as usize)?;
    let root = semantic_model.get_root();
    let token = match root.syntax().token_at_offset(offset.into()) {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(_, token) => token,
        _ => return None,
    };

    let stat = token.parent_ancestors().find_map(LuaStat::cast)?;
    let text_edit = if let Some(comment) = stat.get_left_comment() {
        if let Some(diagnostic_tag) =
            find_diagnostic_disable_tag(comment.clone(), DisableAction::DisableLine)
        {
            let new_start = if let Some(actions_list) = diagnostic_tag.get_code_list() {
                actions_list.get_range().end()
            } else {
                diagnostic_tag.get_range().end()
            };

            let (line, col) = document.get_line_col(new_start)?;
            TextEdit {
                range: Range {
                    start: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                    end: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                },
                new_text: format!(", {}", code.get_name()),
            }
        } else {
            let indent_text = if let Some(prefix_token) = comment.syntax().prev_sibling_or_token() {
                if prefix_token.kind() == LuaTokenKind::TkWhitespace.into() {
                    prefix_token.into_token()?.text().to_string()
                } else {
                    "".to_string()
                }
            } else {
                "".to_string()
            };

            let line = document.get_line(comment.get_position())?;
            TextEdit {
                range: Range {
                    start: Position {
                        line: line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: line as u32,
                        character: 0,
                    },
                },
                new_text: format!(
                    "{}---@diagnostic disable-next-line: {}\n",
                    indent_text,
                    code.get_name()
                ),
            }
        }
    } else {
        let indent_text = if let Some(prefix_token) = stat.syntax().prev_sibling_or_token() {
            if prefix_token.kind() == LuaTokenKind::TkWhitespace.into() {
                prefix_token.into_token()?.text().to_string()
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        };
        let line = document.get_line(stat.get_position())?;
        TextEdit {
            range: Range {
                start: Position {
                    line: line as u32,
                    character: 0,
                },
                end: Position {
                    line: line as u32,
                    character: 0,
                },
            },
            new_text: format!(
                "{}---@diagnostic disable-next-line: {}\n",
                indent_text,
                code.get_name()
            ),
        }
    };

    let mut changes = HashMap::new();
    let uri = document.get_uri();
    changes.insert(uri, vec![text_edit]);
    let cancel_token = time_cancel_token(Duration::from_secs(5));
    tokio::spawn(async move {
        let params = ApplyWorkspaceEditParams {
            label: None,
            edit: WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            },
        };

        let res = client.apply_edit(params, cancel_token).await;
        if let Some(res) = res {
            if !res.applied {
                log::error!("Failed to apply edit: {:?}", res.failure_reason);
            }
        }
    });

    Some(())
}

fn add_disable_file_comment(
    client: Arc<ClientProxy>,
    semantic_model: SemanticModel<'_>,
    code: DiagnosticCode,
) -> Option<()> {
    let root = semantic_model.get_root();
    let first_block = root.get_block()?;
    let first_child = first_block.children::<LuaAst>().next()?;
    let document = semantic_model.get_document();
    let text_edit = if let LuaAst::LuaComment(comment) = first_child {
        if let Some(diagnostic_tag) =
            find_diagnostic_disable_tag(comment.clone(), DisableAction::DisableFile)
        {
            let new_start = if let Some(actions_list) = diagnostic_tag.get_code_list() {
                actions_list.get_range().end()
            } else {
                diagnostic_tag.get_range().end()
            };

            let (line, col) = document.get_line_col(new_start)?;
            TextEdit {
                range: Range {
                    start: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                    end: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                },
                new_text: format!(", {}", code.get_name()),
            }
        } else {
            TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                new_text: format!("---@diagnostic disable: {}\n", code.get_name()),
            }
        }
    } else {
        TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            new_text: format!("---@diagnostic disable: {}\n", code.get_name()),
        }
    };

    let mut changes = HashMap::new();
    let uri = document.get_uri();
    changes.insert(uri, vec![text_edit]);
    let cancel_token = time_cancel_token(Duration::from_secs(5));
    tokio::spawn(async move {
        let params = ApplyWorkspaceEditParams {
            label: None,
            edit: WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            },
        };

        let res = client.apply_edit(params, cancel_token).await;
        if let Some(res) = res {
            if !res.applied {
                log::error!("Failed to apply edit: {:?}", res.failure_reason);
            }
        }
    });

    Some(())
}

fn find_diagnostic_disable_tag(
    comment: LuaComment,
    action: DisableAction,
) -> Option<LuaDocTagDiagnostic> {
    let diagnostic_tags = comment.get_doc_tags().into_iter().filter_map(|tag| {
        if let LuaDocTag::Diagnostic(diagnostic) = tag {
            Some(diagnostic)
        } else {
            None
        }
    });

    for diagnostic_tag in diagnostic_tags {
        let action_token = diagnostic_tag.get_action_token()?;
        let action_token_text = action_token.get_name_text();
        match action {
            DisableAction::DisableLine => {
                if action_token_text == "disable-next-line" {
                    return Some(diagnostic_tag);
                }
            }
            DisableAction::DisableFile | DisableAction::DisableProject => {
                if action_token_text == "disable" {
                    return Some(diagnostic_tag);
                }
            }
        }
    }
    None
}

pub fn make_disable_code_command(
    title: &str,
    action: DisableAction,
    code: DiagnosticCode,
    file_id: FileId,
    range: Range,
) -> Command {
    let args = vec![
        serde_json::to_value(action).unwrap(),
        serde_json::to_value(file_id).unwrap(),
        serde_json::to_value(range).unwrap(),
        serde_json::to_value(code.get_name()).unwrap(),
    ];

    Command {
        title: title.to_string(),
        command: COMMAND.to_string(),
        arguments: Some(args),
    }
}

async fn add_disable_project(
    config_manager: Arc<RwLock<WorkspaceManager>>,
    code: DiagnosticCode,
) -> Option<()> {
    let config_manager = config_manager.read().await;
    let main_workspace = config_manager.workspace_folders.get(0)?;
    let emmyrc_path = main_workspace.join(".emmyrc.json");
    let mut emmyrc = load_configs(vec![emmyrc_path.clone()], None);
    emmyrc.diagnostics.disable.push(code);
    drop(config_manager);

    let emmyrc_json = serde_json::to_string_pretty(&emmyrc).ok()?;
    if let Ok(mut file) = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&emmyrc_path)
    {
        if let Err(err) = file.write_all(emmyrc_json.as_bytes()) {
            log::error!("write emmyrc file failed: {:?}", err);
            return None;
        }
    } else {
        log::error!("Failed to open/create emmyrc file: {:?}", emmyrc_path);
        return None;
    }

    Some(())
}
