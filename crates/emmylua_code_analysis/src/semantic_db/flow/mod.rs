//! # flow -- per-file control flow graph (CFG)
//!
//! Ported from the old `compilation/analyzer/flow` system; the decl model is now `SemanticId`.
//! `FlowTree` is the per-file fact layer (like `file_facts`): it is invalidated automatically on file changes.
//! Used by later flow-sensitive type queries (narrowing / assignment flow).

mod bind_binary_expr;
mod binder;
mod comment;
mod engine;
mod exprs;
mod flow_node;
mod flow_tree;
mod stats;

use emmylua_parser::LuaChunk;

use crate::FileId;

use super::SemanticDatabase;
pub use binder::FlowBinder;
pub use flow_node::*;
pub use flow_tree::*;

/// Merges a label's incoming flow (folds back predecessors), matching the old `bind_analyze::finish_flow_label`.
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
        return binder.unreachable;
    }
    label
}

pub(super) fn build_flow_tree(db: &SemanticDatabase, file_id: FileId) -> FlowTree {
    let facts = db.file_facts_of(file_id).expect("file facts must exist");
    let tree = db.syntax_tree_of(file_id).expect("syntax tree must exist");
    let chunk: LuaChunk = tree.get_chunk_node();
    let mut binder = FlowBinder::new(file_id, facts);
    let start = binder.start;
    if let Some(block) = chunk.get_block() {
        engine::run_bind_block(&mut binder, block, start);
    }
    binder.finish()
}
