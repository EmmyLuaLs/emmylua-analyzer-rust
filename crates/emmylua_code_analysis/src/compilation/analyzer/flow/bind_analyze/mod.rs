mod comment;
mod exprs;
mod stats;

use emmylua_parser::{LuaAst, LuaAstNode, LuaBlock, LuaChunk};

use crate::compilation::analyzer::flow::{
    bind_analyze::stats::bind_local_stat, binder::FlowBinder,
};

#[allow(unused)]
pub fn bind_analyze(binder: &mut FlowBinder, chunk: LuaChunk) -> Option<()> {
    let block = chunk.get_block()?;
    bind_block(binder, block);
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
        LuaAst::LuaAssignStat(lua_assign_stat) => todo!(),
        LuaAst::LuaLocalStat(lua_local_stat) => bind_local_stat(binder, lua_local_stat),
        LuaAst::LuaCallExprStat(lua_call_expr_stat) => todo!(),
        LuaAst::LuaLabelStat(lua_label_stat) => todo!(),
        LuaAst::LuaBreakStat(lua_break_stat) => todo!(),
        LuaAst::LuaGotoStat(lua_goto_stat) => todo!(),
        LuaAst::LuaDoStat(lua_do_stat) => todo!(),
        LuaAst::LuaWhileStat(lua_while_stat) => todo!(),
        LuaAst::LuaRepeatStat(lua_repeat_stat) => todo!(),
        LuaAst::LuaIfStat(lua_if_stat) => todo!(),
        LuaAst::LuaForStat(lua_for_stat) => todo!(),
        LuaAst::LuaForRangeStat(lua_for_range_stat) => todo!(),
        LuaAst::LuaFuncStat(lua_func_stat) => todo!(),
        LuaAst::LuaLocalFuncStat(lua_local_func_stat) => todo!(),
        LuaAst::LuaReturnStat(lua_return_stat) => todo!(),
        LuaAst::LuaNameExpr(lua_name_expr) => todo!(),
        LuaAst::LuaIndexExpr(lua_index_expr) => todo!(),
        LuaAst::LuaTableExpr(lua_table_expr) => todo!(),
        LuaAst::LuaBinaryExpr(lua_binary_expr) => todo!(),
        LuaAst::LuaUnaryExpr(lua_unary_expr) => todo!(),
        LuaAst::LuaParenExpr(lua_paren_expr) => todo!(),
        LuaAst::LuaCallExpr(lua_call_expr) => todo!(),
        LuaAst::LuaLiteralExpr(lua_literal_expr) => todo!(),
        LuaAst::LuaClosureExpr(lua_closure_expr) => todo!(),
        LuaAst::LuaTableField(lua_table_field) => todo!(),
        LuaAst::LuaParamList(lua_param_list) => todo!(),
        LuaAst::LuaParamName(lua_param_name) => todo!(),
        LuaAst::LuaCallArgList(lua_call_arg_list) => todo!(),
        LuaAst::LuaLocalName(lua_local_name) => todo!(),
        LuaAst::LuaLocalAttribute(lua_local_attribute) => todo!(),
        LuaAst::LuaElseIfClauseStat(lua_else_if_clause_stat) => todo!(),
        LuaAst::LuaElseClauseStat(lua_else_clause_stat) => todo!(),
        LuaAst::LuaComment(lua_comment) => todo!(),
        _ => Some(()),
    }
}
