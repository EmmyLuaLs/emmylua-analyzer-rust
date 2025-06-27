mod comment;
mod exprs;
mod stats;

use emmylua_parser::{LuaAst, LuaAstNode, LuaBlock, LuaChunk, LuaExpr};

use crate::{
    compilation::analyzer::flow::{
        bind_analyze::{
            comment::bind_comment,
            exprs::bind_expr,
            stats::{
                bind_assign_stat, bind_break_stat, bind_call_expr_stat, bind_do_stat,
                bind_for_range_stat, bind_for_stat, bind_func_stat, bind_goto_stat, bind_if_stat,
                bind_label_stat, bind_local_func_stat, bind_local_stat, bind_repeat_stat,
                bind_return_stat, bind_while_stat,
            },
        },
        binder::FlowBinder,
    },
    FlowAntecedent, FlowId, FlowNodeKind,
};

#[allow(unused)]
pub fn bind_analyze(binder: &mut FlowBinder, chunk: LuaChunk) -> Option<()> {
    let block = chunk.get_block()?;
    let start = binder.start;
    bind_block(binder, block, start);
    Some(())
}

fn bind_block(binder: &mut FlowBinder, block: LuaBlock, current: FlowId) -> FlowId {
    let mut return_flow_id = current;
    for node in block.children::<LuaAst>() {
        return_flow_id = bind_node(binder, node, return_flow_id);
        if let Some(flow_node) = binder.get_flow(return_flow_id) {
            match &flow_node.kind {
                FlowNodeKind::Return | FlowNodeKind::Break => {
                    return_flow_id = binder.unreachable;
                    break;
                }
                _ => {}
            }
        }
    }

    return_flow_id
}

fn bind_each_child(binder: &mut FlowBinder, ast_node: LuaAst, mut current: FlowId) -> FlowId {
    for node in ast_node.children::<LuaAst>() {
        current = bind_node(binder, node, current);
    }

    current
}

fn bind_node(binder: &mut FlowBinder, node: LuaAst, current: FlowId) -> FlowId {
    match node {
        LuaAst::LuaBlock(block) => bind_block(binder, block, current),
        // stat
        LuaAst::LuaAssignStat(assign_stat) => bind_assign_stat(binder, assign_stat, current),
        LuaAst::LuaLocalStat(local_stat) => bind_local_stat(binder, local_stat, current),
        LuaAst::LuaCallExprStat(call_expr_stat) => {
            bind_call_expr_stat(binder, call_expr_stat, current)
        }
        LuaAst::LuaLabelStat(label_stat) => bind_label_stat(binder, label_stat, current),
        LuaAst::LuaBreakStat(break_stat) => bind_break_stat(binder, break_stat, current),
        LuaAst::LuaGotoStat(goto_stat) => bind_goto_stat(binder, goto_stat, current),
        LuaAst::LuaReturnStat(return_stat) => bind_return_stat(binder, return_stat, current),
        LuaAst::LuaDoStat(do_stat) => bind_do_stat(binder, do_stat, current),
        LuaAst::LuaWhileStat(while_stat) => bind_while_stat(binder, while_stat, current),
        LuaAst::LuaRepeatStat(repeat_stat) => bind_repeat_stat(binder, repeat_stat, current),
        LuaAst::LuaIfStat(if_stat) => bind_if_stat(binder, if_stat, current),
        LuaAst::LuaForStat(for_stat) => bind_for_stat(binder, for_stat, current),
        LuaAst::LuaForRangeStat(for_range_stat) => {
            bind_for_range_stat(binder, for_range_stat, current)
        }
        LuaAst::LuaFuncStat(func_stat) => bind_func_stat(binder, func_stat, current),
        LuaAst::LuaLocalFuncStat(local_func_stat) => {
            bind_local_func_stat(binder, local_func_stat, current)
        }
        // LuaAst::LuaElseIfClauseStat(else_if_clause_stat) => todo!(),
        // LuaAst::LuaElseClauseStat(else_clause_stat) => todo!(),

        // exprs
        LuaAst::LuaNameExpr(_)
        | LuaAst::LuaIndexExpr(_)
        | LuaAst::LuaTableExpr(_)
        | LuaAst::LuaBinaryExpr(_)
        | LuaAst::LuaUnaryExpr(_)
        | LuaAst::LuaParenExpr(_)
        | LuaAst::LuaCallExpr(_)
        | LuaAst::LuaLiteralExpr(_)
        | LuaAst::LuaClosureExpr(_) => bind_expr(
            binder,
            LuaExpr::cast(node.syntax().clone()).unwrap(),
            current,
        ),

        LuaAst::LuaComment(comment) => bind_comment(binder, comment, current),
        LuaAst::LuaTableField(_)
        | LuaAst::LuaParamList(_)
        | LuaAst::LuaParamName(_)
        | LuaAst::LuaCallArgList(_)
        | LuaAst::LuaLocalName(_) => bind_each_child(binder, node, current),

        _ => current,
    }
}

fn finish_flow_label(binder: &mut FlowBinder, label: FlowId, default: FlowId) -> FlowId {
    if let Some(flow_node) = binder.get_flow(label) {
        if let Some(antecedent) = &flow_node.antecedent {
            if let FlowAntecedent::Single(existing_id) = antecedent {
                return *existing_id;
            }
        } else {
            return default;
        }
    } else {
        // This should not happen, but if it does, we can safely ignore it.
        // It means that the label was never used.
        return binder.unreachable;
    }
    label
}
