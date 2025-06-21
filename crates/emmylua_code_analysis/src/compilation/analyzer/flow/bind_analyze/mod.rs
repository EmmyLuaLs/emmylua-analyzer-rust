mod comment;
mod exprs;
mod stats;

use emmylua_parser::{LuaAst, LuaAstNode, LuaBlock, LuaChunk};

use crate::compilation::analyzer::flow::{
    bind_analyze::{
        comment::bind_comment,
        exprs::{
            bind_binary_expr, bind_call_expr, bind_closure_expr, bind_index_expr, bind_name_expr,
            bind_paren_expr, bind_table_expr, bind_unary_expr,
        },
        stats::{
            bind_assign_stat, bind_break_stat, bind_call_expr_stat, bind_do_stat,
            bind_for_range_stat, bind_for_stat, bind_func_stat, bind_goto_stat, bind_if_stat,
            bind_label_stat, bind_local_func_stat, bind_local_stat, bind_repeat_stat,
            bind_return_stat, bind_while_stat,
        },
    },
    binder::FlowBinder,
    flow_node::FlowId,
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
        LuaAst::LuaNameExpr(name_expr) => bind_name_expr(binder, name_expr, current),
        LuaAst::LuaIndexExpr(index_expr) => bind_index_expr(binder, index_expr, current),
        LuaAst::LuaTableExpr(table_expr) => bind_table_expr(binder, table_expr, current),
        LuaAst::LuaBinaryExpr(binary_expr) => bind_binary_expr(binder, binary_expr, current),
        LuaAst::LuaUnaryExpr(unary_expr) => bind_unary_expr(binder, unary_expr, current),
        LuaAst::LuaParenExpr(paren_expr) => bind_paren_expr(binder, paren_expr, current),
        LuaAst::LuaCallExpr(call_expr) => bind_call_expr(binder, call_expr, current),
        LuaAst::LuaLiteralExpr(_) => current,
        LuaAst::LuaClosureExpr(closure_expr) => bind_closure_expr(binder, closure_expr, current),

        LuaAst::LuaComment(comment) => bind_comment(binder, comment, current),
        LuaAst::LuaTableField(_)
        | LuaAst::LuaParamList(_)
        | LuaAst::LuaParamName(_)
        | LuaAst::LuaCallArgList(_)
        | LuaAst::LuaLocalName(_) => bind_each_child(binder, node, current),

        _ => current,
    }
}
