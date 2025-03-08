use crate::LuaFlowId;
use std::collections::HashMap;

use super::flow_nodes::{FlowNode, FlowNodes};

#[derive(Debug)]
pub struct FlowBuilder {
    current_flow_id: LuaFlowId,
    flow_id_stack: Vec<LuaFlowId>,
    flow_trees: HashMap<LuaFlowId, FlowNodes>,
}

impl FlowBuilder {
    pub fn new() -> FlowBuilder {
        let mut builder = FlowBuilder {
            current_flow_id: LuaFlowId::chunk(),
            flow_id_stack: Vec::new(),
            flow_trees: HashMap::new(),
        };

        builder.enter_flow(LuaFlowId::chunk());
        builder
    }

    pub fn enter_flow(&mut self, flow_id: LuaFlowId) {
        self.flow_id_stack.push(flow_id);
        self.current_flow_id = flow_id;
        self.flow_trees.insert(flow_id, FlowNodes::new());
    }

    pub fn pop_flow(&mut self) {
        self.flow_id_stack.pop();
        self.current_flow_id = *self.flow_id_stack.last().unwrap();
    }

    pub fn add_flow_node(&mut self, var_name: &str, flow_node: FlowNode) -> Option<()> {
        let flow_id = self.flow_id_stack.last()?;
        let flow_tree = self.flow_trees.get_mut(flow_id)?;
        flow_tree.add_var_ref(var_name, flow_node);

        Some(())
    }

    pub fn finish(self) -> Vec<(LuaFlowId, FlowNodes)> {
        self.flow_trees.into_iter().collect()
    }
}
