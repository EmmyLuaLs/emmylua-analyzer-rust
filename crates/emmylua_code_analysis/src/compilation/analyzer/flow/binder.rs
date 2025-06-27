use std::collections::HashMap;

use emmylua_parser::LuaSyntaxId;
use internment::ArcIntern;
use rowan::TextSize;
use smol_str::SmolStr;

use crate::{
    AnalyzeError, DbIndex, FileId, FlowAntecedent, FlowId, FlowNode, FlowNodeKind, FlowTree,
    LuaClosureId, LuaDeclId,
};

#[derive(Debug)]
pub struct FlowBinder<'a> {
    pub db: &'a mut DbIndex,
    pub file_id: FileId,
    pub decl_bind_flow_ref: HashMap<LuaDeclId, FlowId>,
    pub start: FlowId,
    pub unreachable: FlowId,
    pub loop_label: FlowId,
    pub loop_post_label: FlowId,
    pub true_target: FlowId,
    pub false_target: FlowId,
    flow_nodes: Vec<FlowNode>,
    multiple_antecedents: Vec<Vec<FlowId>>,
    labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
    goto_stats: Vec<GotoCache>,
    bindings: HashMap<LuaSyntaxId, FlowId>,
}

impl<'a> FlowBinder<'a> {
    pub fn new(db: &'a mut DbIndex, file_id: FileId) -> Self {
        let mut binder = FlowBinder {
            db,
            file_id,
            flow_nodes: Vec::new(),
            multiple_antecedents: Vec::new(),
            decl_bind_flow_ref: HashMap::new(),
            labels: HashMap::new(),
            start: FlowId::default(),
            unreachable: FlowId::default(),
            loop_post_label: FlowId::default(),
            bindings: HashMap::new(),
            goto_stats: Vec::new(),
            loop_label: FlowId::default(),
            true_target: FlowId::default(),
            false_target: FlowId::default(),
        };

        binder.start = binder.create_start();
        binder.unreachable = binder.create_unreachable();
        binder.loop_post_label = binder.unreachable;
        binder.loop_label = binder.unreachable;
        binder.true_target = binder.unreachable;
        binder.false_target = binder.unreachable;

        binder
    }

    pub fn create_node(&mut self, kind: FlowNodeKind) -> FlowId {
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
        self.create_node(FlowNodeKind::BranchLabel)
    }

    pub fn create_loop_label(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::LoopLabel)
    }

    pub fn create_name_label(&mut self, name: &str, closure_id: LuaClosureId) -> FlowId {
        let label_id = self.create_node(FlowNodeKind::NamedLabel(ArcIntern::from(SmolStr::new(
            name,
        ))));
        self.labels
            .entry(closure_id)
            .or_default()
            .insert(SmolStr::new(name), label_id);

        label_id
    }

    pub fn create_start(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::Start)
    }

    pub fn create_unreachable(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::Unreachable)
    }

    pub fn create_break(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::Break)
    }

    pub fn create_return(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::Return)
    }

    pub fn create_decl(&mut self, position: TextSize) -> FlowId {
        self.create_node(FlowNodeKind::DeclPosition(position))
    }

    pub fn add_antecedent(&mut self, node_id: FlowId, antecedent: FlowId) {
        if antecedent == self.unreachable || node_id == self.unreachable {
            // If the antecedent is the unreachable node, we don't need to add it
            return;
        }

        if let Some(existing) = self.flow_nodes.get_mut(node_id.0 as usize) {
            match existing.antecedent {
                Some(FlowAntecedent::Single(existing_id)) => {
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
                    existing.antecedent = Some(FlowAntecedent::Single(antecedent));
                }
            };
        }
    }

    pub fn bind_syntax_node(&mut self, syntax_id: LuaSyntaxId, flow_id: FlowId) {
        self.bindings.insert(syntax_id, flow_id);
    }

    pub fn cache_goto_flow(&mut self, closure_id: LuaClosureId, label: &str, flow_id: FlowId) {
        self.goto_stats.push(GotoCache {
            closure_id,
            label: SmolStr::new(label),
            flow_id,
        });
    }

    pub fn get_flow(&self, flow_id: FlowId) -> Option<&FlowNode> {
        self.flow_nodes.get(flow_id.0 as usize)
    }

    pub fn report_error(&mut self, error: AnalyzeError) {
        self.db
            .get_diagnostic_index_mut()
            .add_diagnostic(self.file_id, error);
    }

    pub fn finish(self) -> FlowTree {
        FlowTree::new(
            self.decl_bind_flow_ref,
            self.flow_nodes,
            self.multiple_antecedents,
            // self.labels,
            self.bindings,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GotoCache {
    pub closure_id: LuaClosureId,
    pub label: SmolStr,
    pub flow_id: FlowId,
}
