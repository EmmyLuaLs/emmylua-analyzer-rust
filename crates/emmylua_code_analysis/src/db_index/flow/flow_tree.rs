use std::collections::VecDeque;

use hashbrown::{HashMap, HashSet};

use emmylua_parser::{LuaAstPtr, LuaCallExpr, LuaExpr, LuaSyntaxId};
use rowan::TextSize;

use crate::{FlowAntecedent, FlowId, FlowNode, FlowNodeKind, LuaDeclId};

#[derive(Debug)]
pub struct FlowTree {
    decl_bind_expr_ref: HashMap<LuaDeclId, LuaAstPtr<LuaExpr>>,
    decl_multi_return_ref: HashMap<LuaDeclId, Vec<DeclMultiReturnRefAt>>,
    flow_nodes: Vec<FlowNode>,
    has_tag_cast: bool,
    multiple_antecedents: Vec<Vec<FlowId>>,
    // labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
    bindings: HashMap<LuaSyntaxId, FlowId>,
}

impl FlowTree {
    pub fn new(
        decl_bind_expr_ref: HashMap<LuaDeclId, LuaAstPtr<LuaExpr>>,
        decl_multi_return_ref: HashMap<LuaDeclId, Vec<DeclMultiReturnRefAt>>,
        flow_nodes: Vec<FlowNode>,
        multiple_antecedents: Vec<Vec<FlowId>>,
        // labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
        bindings: HashMap<LuaSyntaxId, FlowId>,
    ) -> Self {
        let has_tag_cast = flow_nodes
            .iter()
            .any(|node| matches!(node.kind, FlowNodeKind::TagCast(_)));
        Self {
            decl_bind_expr_ref,
            decl_multi_return_ref,
            flow_nodes,
            has_tag_cast,
            multiple_antecedents,
            bindings,
        }
    }

    pub fn get_flow_id(&self, syntax_id: LuaSyntaxId) -> Option<FlowId> {
        self.bindings.get(&syntax_id).cloned()
    }

    pub fn get_flow_node(&self, flow_id: FlowId) -> Option<&FlowNode> {
        self.flow_nodes.get(flow_id.0 as usize)
    }

    pub fn has_tag_cast(&self) -> bool {
        self.has_tag_cast
    }

    pub fn get_multi_antecedents(&self, id: u32) -> Option<&[FlowId]> {
        self.multiple_antecedents
            .get(id as usize)
            .map(|v| v.as_slice())
    }

    /// Returns the first backward-reachable flow node shared by all starting flows.
    pub fn get_nearest_common_antecedent(&self, flow_ids: &[FlowId]) -> Option<FlowId> {
        let (first_flow_id, rest_flow_ids) = flow_ids.split_first()?;
        let first_antecedents = self.collect_antecedents(*first_flow_id);
        let rest_antecedents = rest_flow_ids
            .iter()
            .map(|flow_id| {
                self.collect_antecedents(*flow_id)
                    .into_iter()
                    .collect::<HashSet<_>>()
            })
            .collect::<Vec<_>>();

        first_antecedents
            .into_iter()
            .find(|flow_id| rest_antecedents.iter().all(|set| set.contains(flow_id)))
    }

    pub fn get_decl_ref_expr(&self, decl_id: &LuaDeclId) -> Option<LuaAstPtr<LuaExpr>> {
        self.decl_bind_expr_ref.get(decl_id).cloned()
    }

    pub fn has_shared_multi_return_refs(
        &self,
        left_decl_id: &LuaDeclId,
        right_decl_id: &LuaDeclId,
    ) -> bool {
        let Some(left_refs) = self.decl_multi_return_ref.get(left_decl_id) else {
            return false;
        };
        let Some(right_refs) = self.decl_multi_return_ref.get(right_decl_id) else {
            return false;
        };

        left_refs
            .iter()
            .filter_map(|entry| entry.reference.as_ref())
            .any(|left_ref| {
                let left_call_id = left_ref.call_expr.get_syntax_id();
                right_refs
                    .iter()
                    .filter_map(|entry| entry.reference.as_ref())
                    .any(|right_ref| right_ref.call_expr.get_syntax_id() == left_call_id)
            })
    }

    /// Chooses the search roots used to resolve correlated multi-return refs.
    ///
    /// If either declaration already has a multi-return ref reachable on the current
    /// straight-line history, the caller can analyze the current flow directly and we
    /// return `current_flow_id` as the only search root.
    ///
    /// Otherwise the current flow sits after a branch merge, so we walk backward to the
    /// nearest multi-antecedent join and return each incoming branch flow separately.
    /// This lets downstream correlation logic analyze branch-local histories without
    /// mixing refs from different branches together.
    pub fn get_decl_multi_return_search_roots(
        &self,
        discriminant_decl_id: &LuaDeclId,
        target_decl_id: &LuaDeclId,
        position: TextSize,
        current_flow_id: FlowId,
    ) -> Vec<FlowId> {
        if self.has_decl_multi_return_ref_on_linear_history(
            discriminant_decl_id,
            position,
            current_flow_id,
        ) || self.has_decl_multi_return_ref_on_linear_history(
            target_decl_id,
            position,
            current_flow_id,
        ) {
            vec![current_flow_id]
        } else {
            self.get_nearest_branch_antecedents(current_flow_id)
        }
    }

    pub fn get_decl_multi_return_ref_summary_at(
        &self,
        decl_id: &LuaDeclId,
        position: TextSize,
        flow_id: FlowId,
    ) -> (Vec<DeclMultiReturnRef>, bool) {
        let mut refs = Vec::new();
        let mut has_non_reference_origin = false;
        let mut visited = HashSet::new();
        self.collect_decl_multi_return_refs_at(
            decl_id,
            position,
            flow_id,
            &mut visited,
            &mut refs,
            &mut has_non_reference_origin,
        );
        (refs, has_non_reference_origin)
    }

    fn collect_decl_multi_return_refs_at(
        &self,
        decl_id: &LuaDeclId,
        position: TextSize,
        flow_id: FlowId,
        visited: &mut HashSet<FlowId>,
        refs: &mut Vec<DeclMultiReturnRef>,
        has_non_reference_origin: &mut bool,
    ) {
        if !visited.insert(flow_id) {
            return;
        }

        if let Some(at) = self.get_decl_multi_return_ref_on_flow(decl_id, position, flow_id) {
            if let Some(reference) = &at.reference {
                refs.push(reference.clone());
            } else {
                *has_non_reference_origin = true;
            }
            return;
        }

        let Some(flow_node) = self.get_flow_node(flow_id) else {
            *has_non_reference_origin = true;
            return;
        };
        let Some(antecedent) = flow_node.antecedent.as_ref() else {
            *has_non_reference_origin = true;
            return;
        };
        match antecedent {
            FlowAntecedent::Single(next_flow_id) => {
                self.collect_decl_multi_return_refs_at(
                    decl_id,
                    position,
                    *next_flow_id,
                    visited,
                    refs,
                    has_non_reference_origin,
                );
            }
            FlowAntecedent::Multiple(multi_id) => {
                if let Some(multi_antecedents) = self.get_multi_antecedents(*multi_id) {
                    for &next_flow_id in multi_antecedents {
                        self.collect_decl_multi_return_refs_at(
                            decl_id,
                            position,
                            next_flow_id,
                            visited,
                            refs,
                            has_non_reference_origin,
                        );
                    }
                } else {
                    *has_non_reference_origin = true;
                }
            }
        }
    }

    fn get_decl_multi_return_ref_on_flow(
        &self,
        decl_id: &LuaDeclId,
        position: TextSize,
        flow_id: FlowId,
    ) -> Option<&DeclMultiReturnRefAt> {
        self.decl_multi_return_ref
            .get(decl_id)?
            .iter()
            .rev()
            .find(|entry| entry.position <= position && entry.flow_id == flow_id)
    }

    /// Returns whether `decl_id` has a recorded multi-return ref on the linear backward history.
    ///
    /// "Linear history" means repeatedly following only `FlowAntecedent::Single` links from
    /// `start_flow_id`. The search stops as soon as it reaches a merge (`Multiple`) or the start
    /// of flow. In other words, this checks only the current straight-line history and does
    /// not inspect alternate branch predecessors.
    fn has_decl_multi_return_ref_on_linear_history(
        &self,
        decl_id: &LuaDeclId,
        position: TextSize,
        start_flow_id: FlowId,
    ) -> bool {
        let mut current_flow_id = start_flow_id;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(current_flow_id) {
                return false;
            }

            if self
                .get_decl_multi_return_ref_on_flow(decl_id, position, current_flow_id)
                .is_some()
            {
                return true;
            }

            let Some(flow_node) = self.get_flow_node(current_flow_id) else {
                return false;
            };
            match flow_node.antecedent.as_ref() {
                Some(FlowAntecedent::Single(next_flow_id)) => {
                    current_flow_id = *next_flow_id;
                }
                Some(FlowAntecedent::Multiple(_)) | None => return false,
            }
        }
    }

    fn get_nearest_branch_antecedents(&self, start_flow_id: FlowId) -> Vec<FlowId> {
        let mut current_flow_id = start_flow_id;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(current_flow_id) {
                return vec![start_flow_id];
            }

            let Some(flow_node) = self.get_flow_node(current_flow_id) else {
                return vec![start_flow_id];
            };
            match flow_node.antecedent.as_ref() {
                Some(FlowAntecedent::Multiple(multi_id)) => {
                    return self
                        .get_multi_antecedents(*multi_id)
                        .map(|flows| flows.to_vec())
                        .unwrap_or_else(|| vec![start_flow_id]);
                }
                Some(FlowAntecedent::Single(next_flow_id)) => {
                    current_flow_id = *next_flow_id;
                }
                None => return vec![start_flow_id],
            }
        }
    }

    fn collect_antecedents(&self, flow_id: FlowId) -> Vec<FlowId> {
        let mut antecedents = Vec::new();
        let mut pending = VecDeque::from([flow_id]);
        let mut visited = HashSet::new();
        while let Some(flow_id) = pending.pop_front() {
            if !visited.insert(flow_id) {
                continue;
            }

            antecedents.push(flow_id);
            let Some(flow_node) = self.get_flow_node(flow_id) else {
                continue;
            };
            match flow_node.antecedent.as_ref() {
                Some(FlowAntecedent::Single(antecedent_flow_id)) => {
                    pending.push_back(*antecedent_flow_id);
                }
                Some(FlowAntecedent::Multiple(multi_id)) => {
                    if let Some(branch_flow_ids) = self.get_multi_antecedents(*multi_id) {
                        pending.extend(branch_flow_ids.iter().copied());
                    }
                }
                None => {}
            }
        }

        antecedents
    }
}

#[derive(Debug, Clone)]
pub struct DeclMultiReturnRef {
    pub call_expr: LuaAstPtr<LuaCallExpr>,
    pub return_index: usize,
}

#[derive(Debug, Clone)]
pub struct DeclMultiReturnRefAt {
    pub position: TextSize,
    pub flow_id: FlowId,
    pub reference: Option<DeclMultiReturnRef>,
}
