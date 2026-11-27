use hashbrown::HashMap;

use emmylua_parser::{
    LuaAstNode, LuaAstPtr, LuaBlock, LuaExpr, LuaGotoStat, LuaNameToken, LuaSyntaxId,
};
use internment::ArcIntern;
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use super::{
    DeclMultiReturnRefAt, FlowAntecedent, FlowEffect, FlowId, FlowNode, FlowNodeKind, FlowTree,
    LuaClosureId,
};
use crate::FileId;
use crate::semantic_db::def::SemanticId;
use crate::semantic_db::facts::FileFacts;

#[derive(Debug)]
pub struct FlowBinder<'a> {
    pub facts: &'a FileFacts,
    pub file_id: FileId,
    pub decl_bind_expr_ref: HashMap<SemanticId, LuaAstPtr<LuaExpr>>,
    pub decl_multi_return_ref: HashMap<SemanticId, Vec<DeclMultiReturnRefAt>>,
    pub start: FlowId,
    pub unreachable: FlowId,
    pub loop_label: FlowId,
    pub break_target_label: FlowId,
    pub true_target: FlowId,
    pub false_target: FlowId,
    flow_nodes: Vec<FlowNode>,
    node_effects: HashMap<FlowId, Vec<FlowEffect>>,
    multiple_antecedents: Vec<Vec<FlowId>>,
    loop_antecedents: HashMap<FlowId, Vec<FlowId>>,
    labels: HashMap<TextRange, HashMap<SmolStr, FlowId>>,
    goto_stats: Vec<GotoCache>,
    bindings: HashMap<LuaSyntaxId, FlowId>,
}

impl<'a> FlowBinder<'a> {
    pub fn new(file_id: FileId, facts: &'a FileFacts) -> Self {
        let mut binder = FlowBinder {
            facts,
            file_id,
            flow_nodes: Vec::new(),
            node_effects: HashMap::new(),
            multiple_antecedents: Vec::new(),
            loop_antecedents: HashMap::new(),
            decl_bind_expr_ref: HashMap::new(),
            decl_multi_return_ref: HashMap::new(),
            labels: HashMap::new(),
            start: FlowId::default(),
            unreachable: FlowId::default(),
            break_target_label: FlowId::default(),
            bindings: HashMap::new(),
            goto_stats: Vec::new(),
            loop_label: FlowId::default(),
            true_target: FlowId::default(),
            false_target: FlowId::default(),
        };

        binder.start = binder.create_start();
        binder.unreachable = binder.create_unreachable();
        binder.break_target_label = binder.unreachable;
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

    /// Give loop reads an entry that stays in the graph as back edges are added.
    pub(super) fn create_loop_entry(&mut self, incoming: FlowId, loop_label: FlowId) -> FlowId {
        if incoming == self.unreachable {
            return incoming;
        }
        let entry = self.create_branch_label();
        self.add_antecedent(entry, incoming);
        self.add_loop_antecedent(entry, loop_label);
        entry
    }

    pub fn create_name_label(&mut self, name: &str, block: TextRange) -> FlowId {
        let label_id = self.create_node(FlowNodeKind::NamedLabel(ArcIntern::from(SmolStr::new(
            name,
        ))));
        self.labels
            .entry(block)
            .or_default()
            .insert(SmolStr::new(name), label_id);

        label_id
    }

    fn get_label(&self, block: TextRange, name: &str) -> Option<FlowId> {
        self.labels.get(&block)?.get(name).copied()
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

    pub fn create_continue(&mut self) -> FlowId {
        self.create_node(FlowNodeKind::Continue)
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

    /// Record a loop path for write reachability while preserving value inference.
    pub(super) fn add_loop_antecedent(&mut self, node: FlowId, antecedent: FlowId) {
        if node != self.unreachable && antecedent != self.unreachable {
            self.loop_antecedents
                .entry(node)
                .or_default()
                .push(antecedent);
        }
    }

    pub fn get_bind_flow(&self, syntax_id: LuaSyntaxId) -> Option<FlowId> {
        self.bindings.get(&syntax_id).copied()
    }

    pub fn cache_goto_flow(
        &mut self,
        stat: &LuaGotoStat,
        label_token: LuaNameToken,
        flow_id: FlowId,
    ) {
        let closure = LuaClosureId::from_node(stat.syntax()).get_range();
        let blocks = stat
            .ancestors::<LuaBlock>()
            .map(|block| block.get_range())
            .take_while(|range| closure.contains_range(*range))
            .collect();
        let label = label_token.get_name_text().into();
        self.goto_stats.push(GotoCache {
            blocks,
            label_token,
            label,
            flow_id,
        });
    }

    #[allow(unused)]
    pub fn get_goto_caches(&mut self) -> Vec<GotoCache> {
        std::mem::take(&mut self.goto_stats)
    }

    pub fn get_flow(&self, flow_id: FlowId) -> Option<&FlowNode> {
        self.flow_nodes.get(flow_id.0 as usize)
    }

    pub fn add_effect(&mut self, flow_id: FlowId, effect: FlowEffect) {
        self.node_effects.entry(flow_id).or_default().push(effect);
    }

    pub fn finish(mut self) -> FlowTree {
        // Binding resumes at labels before all jumps are known. Some of those
        // labels may turn out to have no path from the start of the file.
        let has_disconnected_labels =
            self.labels
                .values()
                .flat_map(|labels| labels.values())
                .any(|label| {
                    self.get_flow(*label)
                        .is_some_and(|node| node.antecedent.is_none())
                });
        // Post-process goto edges: the flow recorded for `goto label` becomes the
        // antecedent of the nearest visible label, including forward targets.
        let goto_stats = std::mem::take(&mut self.goto_stats);
        for goto in goto_stats {
            if let Some(label_id) = goto
                .blocks
                .into_iter()
                .find_map(|block| self.get_label(block, goto.label.as_str()))
            {
                self.add_antecedent(label_id, goto.flow_id);
            }
        }
        if has_disconnected_labels {
            FlowTree::prune_disconnected_antecedents(
                self.start,
                &mut self.flow_nodes,
                &mut self.multiple_antecedents,
                &mut self.loop_antecedents,
            );
        }
        FlowTree::new(
            self.decl_bind_expr_ref,
            self.decl_multi_return_ref,
            self.flow_nodes,
            self.node_effects,
            self.multiple_antecedents,
            self.loop_antecedents,
            // self.labels,
            self.bindings,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GotoCache {
    blocks: Vec<TextRange>,
    pub label_token: LuaNameToken,
    pub label: SmolStr,
    pub flow_id: FlowId,
}
