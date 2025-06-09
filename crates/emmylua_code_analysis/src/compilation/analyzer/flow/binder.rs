use emmylua_parser::LuaAst;

use crate::compilation::analyzer::flow::flow_node::{
    FlowAntecedent, FlowId, FlowNode, FlowNodeKind,
};

#[derive(Debug)]
pub struct FlowBinder {
    flow_nodes: Vec<FlowNode>,
    multiple_antecedents: Vec<Vec<FlowId>>,
    pub none_label: FlowId,
    pub start: FlowId,
    pub unreachable: FlowId,
    pub current: FlowId,
    pub loop_label: FlowId,
}

impl FlowBinder {
    pub fn new() -> Self {
        let mut binder = FlowBinder {
            flow_nodes: Vec::new(),
            multiple_antecedents: Vec::new(),
            start: FlowId::default(),
            none_label: FlowId::default(),
            unreachable: FlowId::default(),
            current: FlowId::default(),
            loop_label: FlowId::default(),
        };

        binder.none_label = binder.create_node(FlowNodeKind::None, None, None);
        binder.start = binder.create_start();
        binder.unreachable = binder.create_unreachable();
        binder.current = binder.start;
        binder.loop_label = binder.none_label;

        binder
    }

    pub fn create_node(
        &mut self,
        kind: FlowNodeKind,
        node: Option<LuaAst>,
        antecedent: Option<FlowAntecedent>,
    ) -> FlowId {
        let id = FlowId(self.flow_nodes.len() as u32);
        let flow_node = FlowNode {
            id,
            kind,
            node,
            antecedent,
        };
        self.flow_nodes.push(flow_node);
        id
    }

    pub fn create_branch_label(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::BranchLabel, None, None)
    }

    pub fn create_loop_label(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::LoopLabel, None, None)
    }

    pub fn create_start(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::Start, None, None)
    }

    pub fn create_unreachable(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::Unreachable, None, None)
    }

    pub fn create_flow_mutation(
        &mut self,
        node: Option<LuaAst>,
        antecedent: Option<FlowId>,
    ) -> FlowId {
        self.create_node(
            FlowNodeKind::Assignment,
            node,
            antecedent.map(|id| FlowAntecedent::Node(id)),
        )
    }

    pub fn add_antecedent(&mut self, node_id: FlowId, antecedent: FlowId) {
        if let Some(existing) = self.flow_nodes.get_mut(node_id.0 as usize) {
            match existing.antecedent {
                Some(FlowAntecedent::Node(existing_id)) => {
                    // If the existing antecedent is a single node, convert it to multiple
                    if existing_id == antecedent {
                        return; // No change needed if it's the same antecedent
                    }
                    existing.antecedent = Some(FlowAntecedent::Multiple(
                        self.multiple_antecedents.len() as u32,
                    ));
                    self.multiple_antecedents
                        .push(vec![existing_id, antecedent]);
                }
                Some(FlowAntecedent::Multiple(index)) => {
                    // Add to multiple antecedents
                    if let Some(multiple) = self.multiple_antecedents.get_mut(index as usize) {
                        multiple.push(antecedent);
                    } else {
                        self.multiple_antecedents.push(vec![antecedent]);
                    }
                }
                _ => {
                    // Set new antecedent
                    existing.antecedent = Some(FlowAntecedent::Node(antecedent));
                }
            };
        }
    }
}
