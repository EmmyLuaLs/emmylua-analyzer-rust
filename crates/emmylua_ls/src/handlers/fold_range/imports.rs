use emmylua_code_analysis::Emmyrc;
use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr, LuaStat};
use lsp_types::{FoldingRange, FoldingRangeKind};
use rowan::TextSize;

use super::builder::FoldingRangeBuilder;

pub fn build_imports_fold_range(
    builder: &mut FoldingRangeBuilder,
    root: LuaChunk,
    emmyrc: &Emmyrc,
) -> Option<()> {
    let root_block = root.get_block()?;
    let require_like_func = &emmyrc.runtime.require_like_function;

    let mut start: Option<TextSize> = None;
    let mut end: Option<TextSize> = None;
    for stat in root_block.get_stats() {
        if is_require_stat(stat.clone(), require_like_func).unwrap_or(false) {
            let range = stat.get_range();
            if start.is_none() {
                start = Some(range.start());
            }
            end = Some(range.end());
        } else if start.is_some() && end.is_some() {
            let start_pos = start.unwrap();
            let end_pos = end.unwrap();
            let docucment = builder.get_document();
            let start_line_col = docucment.get_line_col(start_pos)?;
            let end_line_col = docucment.get_line_col(end_pos)?;
            let fold_range = FoldingRange {
                start_line: start_line_col.0 as u32,
                start_character: Some(start_line_col.1 as u32),
                end_line: end_line_col.0 as u32,
                end_character: Some(end_line_col.1 as u32),
                kind: Some(FoldingRangeKind::Imports),
                collapsed_text: Some("imports ...".to_string()),
            };
            builder.push(fold_range);
            start = None;
            end = None;
        }
    }

    // if just only require stat, then donot fold it
    Some(())
}

fn is_require_stat(stat: LuaStat, require_like_func: &Vec<String>) -> Option<bool> {
    match stat {
        LuaStat::LocalStat(local_stat) => {
            let exprs = local_stat.get_value_exprs();
            for expr in exprs {
                if is_require_expr(expr, require_like_func).unwrap_or(false) {
                    return Some(true);
                }
            }
        }
        LuaStat::AssignStat(assign_stat) => {
            let (_, exprs) = assign_stat.get_var_and_expr_list();
            for expr in exprs {
                if is_require_expr(expr, require_like_func).unwrap_or(false) {
                    return Some(true);
                }
            }
        }
        LuaStat::CallExprStat(call_expr_stat) => {
            let expr = call_expr_stat.get_call_expr()?;
            if is_require_expr(expr.into(), require_like_func).unwrap_or(false) {
                return Some(true);
            }
        }
        _ => {}
    }

    Some(false)
}

fn is_require_expr(expr: LuaExpr, require_like_func: &Vec<String>) -> Option<bool> {
    if let LuaExpr::CallExpr(call_expr) = expr {
        let name = call_expr.get_prefix_expr()?;
        if let LuaExpr::NameExpr(name_expr) = name {
            let name = name_expr.get_name_text()?;
            if require_like_func.contains(&name.to_string()) || name == "require" {
                return Some(true);
            }
        }
    }

    Some(false)
}
