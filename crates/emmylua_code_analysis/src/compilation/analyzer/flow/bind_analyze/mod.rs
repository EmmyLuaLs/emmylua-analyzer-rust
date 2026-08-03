mod check_goto;
mod comment;
mod engine;
mod exprs;
mod stats;

use emmylua_parser::LuaChunk;

use crate::{FlowAntecedent, FlowId, compilation::analyzer::flow::binder::FlowBinder};
pub use check_goto::check_goto_label;

pub fn bind_analyze(binder: &mut FlowBinder, chunk: LuaChunk) -> Option<()> {
    let block = chunk.get_block()?;
    let start = binder.start;
    engine::run_bind_block(binder, block, start);
    Some(())
}

pub(super) fn finish_flow_label(binder: &mut FlowBinder, label: FlowId, default: FlowId) -> FlowId {
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
