mod comment;
mod exprs;
mod stats;

use emmylua_parser::{LuaAst, LuaAstNode, LuaBlock, LuaChunk};

use crate::compilation::analyzer::flow::{
    bind_analyze::{comment::bind_comment, stats::{bind_assign_stat, bind_call_expr_stat, bind_local_stat}},
    binder::FlowBinder,
};

#[allow(unused)]
pub fn bind_analyze(binder: &mut FlowBinder, chunk: LuaChunk) -> Option<()> {
    let block = chunk.get_block()?;
    bind_block(binder, block)?;
    Some(())
}

fn bind_block(binder: &mut FlowBinder, block: LuaBlock) -> Option<()> {
    bind_each_child(binder, LuaAst::LuaBlock(block))
}

fn bind_each_child(binder: &mut FlowBinder, ast_node: LuaAst) -> Option<()> {
    for node in ast_node.children::<LuaAst>() {
        bind_node(binder, node);
    }
    Some(())
}

fn bind_node(binder: &mut FlowBinder, node: LuaAst) -> Option<()> {
    match node {
        LuaAst::LuaBlock(block) => bind_block(binder, block),
        LuaAst::LuaAssignStat(assign_stat) => bind_assign_stat(binder, assign_stat),
        LuaAst::LuaLocalStat(local_stat) => bind_local_stat(binder, local_stat),
        LuaAst::LuaCallExprStat(call_expr_stat) => bind_call_expr_stat(binder, call_expr_stat),
        LuaAst::LuaLabelStat(_) => Some(()),
        LuaAst::LuaBreakStat(break_stat) => todo!(),
        LuaAst::LuaGotoStat(goto_stat) => todo!(),
        LuaAst::LuaDoStat(do_stat) => todo!(),
        LuaAst::LuaWhileStat(while_stat) => todo!(),
        LuaAst::LuaRepeatStat(repeat_stat) => todo!(),
        LuaAst::LuaIfStat(if_stat) => todo!(),
        LuaAst::LuaForStat(for_stat) => todo!(),
        LuaAst::LuaForRangeStat(for_range_stat) => todo!(),
        LuaAst::LuaFuncStat(func_stat) => todo!(),
        LuaAst::LuaLocalFuncStat(local_func_stat) => todo!(),
        LuaAst::LuaReturnStat(return_stat) => todo!(),
        LuaAst::LuaNameExpr(name_expr) => todo!(),
        LuaAst::LuaIndexExpr(index_expr) => todo!(),
        LuaAst::LuaTableExpr(table_expr) => todo!(),
        LuaAst::LuaBinaryExpr(binary_expr) => todo!(),
        LuaAst::LuaUnaryExpr(unary_expr) => todo!(),
        LuaAst::LuaParenExpr(paren_expr) => todo!(),
        LuaAst::LuaCallExpr(call_expr) => todo!(),
        LuaAst::LuaLiteralExpr(literal_expr) => todo!(),
        LuaAst::LuaClosureExpr(closure_expr) => todo!(),
        LuaAst::LuaElseIfClauseStat(else_if_clause_stat) => todo!(),
        LuaAst::LuaElseClauseStat(else_clause_stat) => todo!(),
        LuaAst::LuaComment(comment) => bind_comment(binder, comment),
        LuaAst::LuaTableField(_)
        | LuaAst::LuaParamList(_)
        | LuaAst::LuaParamName(_)
        | LuaAst::LuaCallArgList(_)
        | LuaAst::LuaLocalName(_) => bind_each_child(binder, node),

        _ => Some(()),
    }
}
