use std::collections::HashMap;

use internment::ArcIntern;
use smol_str::SmolStr;

use crate::{
    compilation::analyzer::flow::flow_node::{FlowAntecedent, FlowId, FlowNode, FlowNodeType},
    DbIndex, FileId, LuaDeclId, LuaClosureId,
};

#[derive(Debug)]
pub struct FlowBinder<'a> {
    pub db: &'a DbIndex,
    pub file_id: FileId,
    flow_nodes: Vec<FlowNode>,
    multiple_antecedents: Vec<Vec<FlowId>>,
    labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
    pub decl_bind_flow_ref: HashMap<LuaDeclId, FlowId>,
    pub none_label: FlowId,
    pub start: FlowId,
    pub unreachable: FlowId,
    pub loop_label: FlowId,
    pub current: FlowId,
}

impl<'a> FlowBinder<'a> {
    pub fn new(db: &'a DbIndex, file_id: FileId) -> Self {
        let mut binder = FlowBinder {
            db,
            file_id,
            flow_nodes: Vec::new(),
            multiple_antecedents: Vec::new(),
            decl_bind_flow_ref: HashMap::new(),
            labels: HashMap::new(),
            start: FlowId::default(),
            none_label: FlowId::default(),
            unreachable: FlowId::default(),
            loop_label: FlowId::default(),
            current: FlowId::default(),
        };

        binder.none_label = binder.create_node(FlowNodeType::None);
        binder.start = binder.create_start();
        binder.unreachable = binder.create_unreachable();
        binder.loop_label = binder.none_label;
        binder.current = binder.start;

        binder
    }

    pub fn create_node(&mut self, kind: FlowNodeType) -> FlowId {
        let id = FlowId(self.flow_nodes.len() as u32);
        let flow_node = FlowNode {
            id,
            kind,
            antecedent: None,
        };
        self.flow_nodes.push(flow_node);
        id
    }
    
    pub fn create_branch_label(&mut self) -> FlowId {
        self.create_node(FlowNodeType::BranchLabel)
    }

    pub fn create_loop_label(&mut self) -> FlowId {
        self.create_node(FlowNodeType::LoopLabel)
    }

    pub fn create_name_label(&mut self, name: String) -> FlowId {
        self.create_node(FlowNodeType::NameLabel(ArcIntern::from(SmolStr::new(name))))
    }

    pub fn create_start(&mut self) -> FlowId {
        self.create_node(FlowNodeType::Start)
    }

    pub fn create_unreachable(&mut self) -> FlowId {
        self.create_node(FlowNodeType::Unreachable)
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
