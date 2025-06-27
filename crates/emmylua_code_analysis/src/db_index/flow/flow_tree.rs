use std::collections::HashMap;

use emmylua_parser::LuaSyntaxId;
use smol_str::SmolStr;

use crate::{FlowId, FlowNode, LuaDeclId};

#[derive(Debug)]
pub struct FlowTree {
    #[allow(unused)]
    decl_bind_flow_ref: HashMap<LuaDeclId, FlowId>,
    flow_nodes: Vec<FlowNode>,
    multiple_antecedents: Vec<Vec<FlowId>>,
    // labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
    bindings: HashMap<LuaSyntaxId, FlowId>,
}

impl FlowTree {
    pub fn new(
        decl_bind_flow_ref: HashMap<LuaDeclId, FlowId>,
        flow_nodes: Vec<FlowNode>,
        multiple_antecedents: Vec<Vec<FlowId>>,
        // labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
        bindings: HashMap<LuaSyntaxId, FlowId>,
    ) -> Self {
        Self {
            decl_bind_flow_ref,
            flow_nodes,
            multiple_antecedents,
            // labels,
            bindings,
        }
    }

    pub fn get_flow_id(&self, syntax_id: LuaSyntaxId) -> Option<FlowId> {
        self.bindings.get(&syntax_id).cloned()
    }

    pub fn get_flow_node(&self, flow_id: FlowId) -> Option<&FlowNode> {
        self.flow_nodes.get(flow_id.0 as usize)
    }

    pub fn get_multi_antecedents(&self, id: u32) -> Option<&[FlowId]> {
        self.multiple_antecedents
            .get(id as usize)
            .map(|v| v.as_slice())
    }
}
