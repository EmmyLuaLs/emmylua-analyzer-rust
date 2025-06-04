mod build_flow_tree;
mod cast_analyze;
mod flow_node;
mod var_analyze;
mod cfg;

use std::collections::HashMap;

use crate::{
    db_index::DbIndex, profile::Profile, FileId, LuaVarRefId, LuaVarRefNode, TypeAssertion,
};
use build_flow_tree::{build_flow_tree, LuaFlowTreeBuilder};
use cast_analyze::analyze_cast;
pub use cast_analyze::CastAction;
use emmylua_parser::{BinaryOperator, LuaAst, LuaAstNode, LuaBinaryExpr, LuaBlock};
use flow_node::BlockId;
use rowan::TextRange;
use var_analyze::{
    analyze_ref_assign, analyze_ref_expr, broadcast_up, UnResolveTraceId, VarTrace, VarTraceInfo,
};

use super::AnalyzeContext;

pub(crate) fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext) {
    let _p = Profile::cond_new("flow analyze", context.tree_list.len() > 1);
    let tree_list = context.tree_list.clone();
    // build decl and ref flow chain
    for in_filed_tree in &tree_list {
        let flow_tree = build_flow_tree(db, in_filed_tree.file_id, in_filed_tree.value.clone());
        analyze_flow(db, in_filed_tree.file_id, flow_tree, context);
    }
}

fn analyze_flow(
    db: &mut DbIndex,
    file_id: FileId,
    flow_tree: LuaFlowTreeBuilder,
    context: &mut AnalyzeContext,
) {
    let var_ref_ids = flow_tree.get_var_ref_ids();
    let mut var_trace_map: HashMap<LuaVarRefId, VarTrace> = HashMap::new();
    for var_ref_id in var_ref_ids {
        let var_ref_nodes = match flow_tree.get_var_ref_nodes(&var_ref_id) {
            Some(nodes) => nodes,
            None => continue,
        };

        let mut var_trace = var_trace_map.entry(var_ref_id.clone()).or_insert_with(|| {
            VarTrace::new(var_ref_id.clone(), var_ref_nodes.clone(), &flow_tree)
        });
        for (var_ref_node, flow_id) in var_ref_nodes {
            var_trace.set_current_flow_id(*flow_id);
            match var_ref_node {
                LuaVarRefNode::UseRef(var_expr) => {
                    analyze_ref_expr(db, &mut var_trace, &var_expr);
                }
                LuaVarRefNode::AssignRef(var_expr) => {
                    analyze_ref_assign(db, &mut var_trace, &var_expr, file_id);
                }
                LuaVarRefNode::CastRef(tag_cast) => {
                    analyze_cast(&mut var_trace, file_id, tag_cast.clone(), context);
                }
            }
        }
        let last_flow_id = var_trace.get_current_flow_id();
        let mut guard_count = 0;
        while var_trace.has_unresolve_traces() {
            resolve_flow_analyze(db, &mut var_trace);
            guard_count += 1;
            if guard_count > 10 {
                break;
            }
        }
        if let Some(last_flow_id) = last_flow_id {
            var_trace.set_current_flow_id(last_flow_id);
        }
    }

    for (_, var_trace) in var_trace_map {
        db.get_flow_index_mut()
            .add_flow_chain(file_id, var_trace.finish());
    }
}

fn resolve_flow_analyze(db: &mut DbIndex, var_trace: &mut VarTrace) -> Option<()> {
    let all_trace = var_trace.pop_all_unresolve_traces();
    for (trace_id, uresolve_trace_info) in all_trace {
        var_trace.set_current_flow_id(uresolve_trace_info.0);
        match trace_id {
            UnResolveTraceId::Expr(expr) => {
                let binary_expr = expr.get_parent::<LuaBinaryExpr>()?;
                let op = binary_expr.get_op_token()?.get_op();
                let trace_info = uresolve_trace_info.1.get_trace_info()?;
                if op == BinaryOperator::OpAnd || op == BinaryOperator::OpOr {
                    broadcast_up(
                        db,
                        var_trace,
                        VarTraceInfo::new(
                            trace_info.type_assertion.clone(),
                            LuaAst::cast(binary_expr.syntax().clone())?,
                        )
                        .into(),
                        binary_expr.get_parent::<LuaAst>()?,
                    );
                }
            }
            UnResolveTraceId::If(if_stat) => {
                let var_trace_infos = uresolve_trace_info.1.get_trace_infos()?;
                let mut trace_map = HashMap::new();
                for trace_info in var_trace_infos {
                    let block_id = BlockId::from_ast(trace_info.node.clone())?;
                    trace_map
                        .entry(block_id)
                        .or_insert_with(Vec::new)
                        .push(trace_info);
                }

                let mut or_asserts = Vec::new();
                for (_, mut trace_infos) in trace_map {
                    match trace_infos.len() {
                        0 => {}
                        1 => {
                            or_asserts.push(trace_infos[0].type_assertion.clone());
                        }
                        _ => {
                            trace_infos
                                .sort_by(|a, b| a.node.get_position().cmp(&b.node.get_position()));
                            let and_asserts = trace_infos
                                .iter()
                                .map(|x| x.type_assertion.clone())
                                .collect::<Vec<_>>();
                            or_asserts.push(TypeAssertion::And(and_asserts.into()));
                        }
                    }
                }

                let block = if_stat.get_parent::<LuaBlock>()?;
                let block_end = block.get_range().end();
                let if_end = if_stat.get_range().end();
                if if_end < block_end {
                    let range = TextRange::new(if_end, block_end);
                    var_trace.add_assert(TypeAssertion::Or(or_asserts.into()), range);
                }
            }
        }
    }

    Some(())
}
