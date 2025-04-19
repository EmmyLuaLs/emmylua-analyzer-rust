mod build_flow_tree;
mod cast_analyze;
mod flow_node;
mod flow_tree;
mod var_analyze;

use std::collections::HashMap;

use crate::{db_index::DbIndex, profile::Profile, FileId, TypeAssertion, VarRefId};
use build_flow_tree::build_flow_tree;
use cast_analyze::analyze_cast;
use emmylua_parser::{BinaryOperator, LuaAst, LuaAstNode, LuaBinaryExpr, LuaBlock};
use flow_tree::{FlowTree, VarRefNode};
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
    flow_tree: FlowTree,
    context: &mut AnalyzeContext,
) {
    let var_ref_ids = flow_tree.get_var_ref_ids();
    let mut var_trace_map: HashMap<VarRefId, VarTrace> = HashMap::new();
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
                VarRefNode::UseRef(var_expr) => {
                    analyze_ref_expr(db, &mut var_trace, &var_expr);
                }
                VarRefNode::AssignRef(var_expr) => {
                    analyze_ref_assign(db, &mut var_trace, &var_expr, file_id);
                }
                VarRefNode::CastRef(tag_cast) => {
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
                let asserts = uresolve_trace_info
                    .1
                    .get_trace_infos()?
                    .into_iter()
                    .map(|trace_info| trace_info.type_assertion.clone())
                    .collect::<Vec<_>>();
                let block = if_stat.get_parent::<LuaBlock>()?;
                let block_end = block.get_range().end();
                let if_end = if_stat.get_range().end();
                if if_end < block_end {
                    let range = TextRange::new(if_end, block_end);
                    var_trace.add_assert(TypeAssertion::Or(asserts.into()), range);
                }
            }
        }
    }

    Some(())
}
