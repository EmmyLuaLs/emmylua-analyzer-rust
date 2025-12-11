use std::{collections::HashMap, time::Duration};

use emmylua_code_analysis::FileId;
use emmylua_parser::{LuaAstNode, LuaExpr, LuaStat};
use lsp_types::{ApplyWorkspaceEditParams, Command, Position, TextEdit, WorkspaceEdit};
use serde_json::Value;

use crate::{context::ServerContextSnapshot, util::time_cancel_token};

use super::CommandSpec;

pub struct AutoRequireCommand;

impl CommandSpec for AutoRequireCommand {
    const COMMAND: &str = "emmy.auto.require";

    async fn handle(context: ServerContextSnapshot, args: Vec<Value>) -> Option<()> {
        let add_to: FileId = serde_json::from_value(args.first()?.clone()).ok()?;
        let need_require_file_id: FileId = serde_json::from_value(args.get(1)?.clone()).ok()?;
        let position: Position = serde_json::from_value(args.get(2)?.clone()).ok()?;
        let local_name: String = serde_json::from_value(args.get(3)?.clone()).ok()?;
        let member_name: String = serde_json::from_value(args.get(4)?.clone()).ok()?;

        let analysis = context.analysis().read().await;
        let semantic_model = analysis.compilation.get_semantic_model(add_to)?;
        let module_info = semantic_model
            .get_db()
            .get_module_index()
            .get_module(need_require_file_id)?;
        let emmyrc = semantic_model.get_emmyrc();
        let require_like_func = &emmyrc.runtime.require_like_function;
        let auto_require_func = emmyrc.completion.auto_require_function.clone();
        let require_separator = emmyrc.completion.auto_require_separator.clone();
        let full_module_path = match require_separator.as_str() {
            "." | "" => module_info.full_module_name.clone(),
            _ => module_info
                .full_module_name
                .replace(".", &require_separator),
        };

        let require_str = format!(
            "local {} = {}(\"{}\"){}",
            if member_name.is_empty() {
                local_name
            } else {
                member_name.clone()
            },
            auto_require_func,
            full_module_path,
            if !member_name.is_empty() {
                format!(".{}", member_name)
            } else {
                "".to_string()
            }
        );
        let document = semantic_model.get_document();
        let offset = document.get_offset(position.line as usize, position.character as usize)?;
        let root_block = semantic_model.get_root().get_block()?;
        let mut last_require_stat: Option<LuaStat> = None;
        for stat in root_block.get_stats() {
            if stat.get_position() > offset {
                break;
            }

            if is_require_stat(stat.clone(), require_like_func).unwrap_or(false) {
                last_require_stat = Some(stat);
            }
        }

        let line = if let Some(last_require_stat) = last_require_stat {
            let last_require_stat_end = last_require_stat.get_range().end();
            document.get_line(last_require_stat_end)? + 1
        } else {
            0
        };

        let text_edit = TextEdit {
            range: lsp_types::Range {
                start: Position {
                    line: line as u32,
                    character: 0,
                },
                end: Position {
                    line: line as u32,
                    character: 0,
                },
            },
            new_text: format!("{}\n", require_str),
        };

        let uri = document.get_uri();
        #[allow(clippy::mutable_key_type)]
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), vec![text_edit.clone()]);

        let cancel_token = time_cancel_token(Duration::from_secs(5));
        let apply_edit_params = ApplyWorkspaceEditParams {
            label: None,
            edit: WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            },
        };

        let context_clone = context.clone();
        tokio::spawn(async move {
            let res = context_clone
                .client()
                .apply_edit(apply_edit_params, cancel_token)
                .await;
            if let Some(res) = res
                && !res.applied
            {
                log::error!("Failed to apply edit: {:?}", res.failure_reason);
            }
        });

        Some(())
    }
}

fn is_require_stat(stat: LuaStat, require_like_func: &[String]) -> Option<bool> {
    match stat {
        LuaStat::LocalStat(local_stat) => {
            let exprs = local_stat.get_value_exprs();
            for expr in exprs {
                if is_require_expr(expr, require_like_func, 0).unwrap_or(false) {
                    return Some(true);
                }
            }
        }
        LuaStat::AssignStat(assign_stat) => {
            let (_, exprs) = assign_stat.get_var_and_expr_list();
            for expr in exprs {
                if is_require_expr(expr, require_like_func, 0).unwrap_or(false) {
                    return Some(true);
                }
            }
        }
        LuaStat::CallExprStat(call_expr_stat) => {
            let expr = call_expr_stat.get_call_expr()?;
            if is_require_expr(expr.into(), require_like_func, 0).unwrap_or(false) {
                return Some(true);
            }
        }
        _ => {}
    }

    Some(false)
}

fn is_require_expr(expr: LuaExpr, require_like_func: &[String], depth: usize) -> Option<bool> {
    if depth > 5 {
        return Some(false);
    }
    match expr {
        LuaExpr::CallExpr(call_expr) => {
            let name = call_expr.get_prefix_expr()?;
            match name {
                LuaExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_text()?;
                    if require_like_func.contains(&name.to_string()) || name == "require" {
                        return Some(true);
                    }
                }
                LuaExpr::CallExpr(prefix_call_expr) => {
                    if is_require_expr(prefix_call_expr.into(), require_like_func, depth + 1)
                        .unwrap_or(false)
                    {
                        return Some(true);
                    }
                }
                _ => {}
            }
        }
        LuaExpr::IndexExpr(index_expr) => {
            if is_require_expr(index_expr.get_prefix_expr()?, require_like_func, depth + 1)
                .unwrap_or(false)
            {
                return Some(true);
            }
        }
        _ => {}
    }

    Some(false)
}

pub fn make_auto_require(
    title: &str,
    add_to: FileId,
    need_require_file_id: FileId,
    position: Position,
    local_name: String,          // 导入时使用的名称
    member_name: Option<String>, // 导入的成员名, 不要包含前缀`.`号, 它将拼接到 `require` 后面. 例如 require("a").member
) -> Command {
    let args = vec![
        serde_json::to_value(add_to).unwrap(),
        serde_json::to_value(need_require_file_id).unwrap(),
        serde_json::to_value(position).unwrap(),
        serde_json::to_value(local_name).unwrap(),
        serde_json::to_value(member_name.unwrap_or_default()).unwrap(),
    ];

    Command {
        title: title.to_string(),
        command: AutoRequireCommand::COMMAND.to_string(),
        arguments: Some(args),
    }
}
