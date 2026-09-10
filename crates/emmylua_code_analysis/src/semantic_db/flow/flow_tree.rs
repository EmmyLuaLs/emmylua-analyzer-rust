use std::collections::VecDeque;

use hashbrown::{HashMap, HashSet};

use emmylua_parser::{LuaAstPtr, LuaCallExpr, LuaExpr, LuaSyntaxId};
use rowan::{TextRange, TextSize};

use super::super::def::SemanticId;
use super::{FlowAntecedent, FlowEffect, FlowId, FlowNode, FlowNodeKind};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FlowTree {
    decl_bind_expr_ref: HashMap<SemanticId, LuaAstPtr<LuaExpr>>,
    decl_multi_return_ref: HashMap<SemanticId, Vec<DeclMultiReturnRefAt>>,
    flow_nodes: Vec<FlowNode>,
    node_effects: HashMap<FlowId, Vec<FlowEffect>>,
    has_tag_cast: bool,
    multiple_antecedents: Vec<Vec<FlowId>>,
    // labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
    bindings: HashMap<LuaSyntaxId, FlowId>,
    /// Sorted binding ranges; used as a fallback for offsets that are not exactly
    /// the start of a bound syntax node.
    binding_ranges: Vec<(TextRange, FlowId)>,
    /// For the common case where `offset` is exactly the start of a bound node,
    /// maps start -> flow id of the smallest range starting there.
    binding_starts: HashMap<TextSize, FlowId>,
    /// Sorted `(assignment statement start, flow id)` entries per assigned declaration.
    decl_assignments: HashMap<SemanticId, Vec<(TextSize, FlowId)>>,
    /// Sorted `(assignment statement start, flow id)` entries per assigned member.
    member_assignments: HashMap<SemanticId, Vec<(TextSize, FlowId)>>,
    /// Sorted `(position, flow id)` for `DeclPosition` nodes.
    decl_positions: Vec<(TextSize, FlowId)>,
    /// Sorted `(condition position, flow id)` for true/false condition nodes.
    condition_nodes: Vec<(TextSize, FlowId)>,
    /// Sorted `(cast position, flow id)` for `---@cast` / `--[[@as]]` nodes.
    cast_nodes: Vec<(TextSize, FlowId)>,
    /// Sorted `(position, flow id)` for nodes with a merge (`Multiple`) antecedent.
    branch_nodes: Vec<(TextSize, FlowId)>,
    /// True when a merge node has no AST range to index safely.
    unpositioned_branch: bool,
    /// True when the tree has loops / labels / break / continue. These can create
    /// backward edges whose source positions are not monotonic, so the member
    /// assignment-position fast path is conservatively disabled.
    has_complex_control: bool,
}

impl FlowTree {
    pub fn new(
        decl_bind_expr_ref: HashMap<SemanticId, LuaAstPtr<LuaExpr>>,
        decl_multi_return_ref: HashMap<SemanticId, Vec<DeclMultiReturnRefAt>>,
        flow_nodes: Vec<FlowNode>,
        node_effects: HashMap<FlowId, Vec<FlowEffect>>,
        multiple_antecedents: Vec<Vec<FlowId>>,
        // labels: HashMap<LuaClosureId, HashMap<SmolStr, FlowId>>,
        bindings: HashMap<LuaSyntaxId, FlowId>,
    ) -> Self {
        let mut decl_assignments: HashMap<SemanticId, Vec<(TextSize, FlowId)>> = HashMap::new();
        let mut member_assignments: HashMap<SemanticId, Vec<(TextSize, FlowId)>> = HashMap::new();
        let mut decl_positions: Vec<(TextSize, FlowId)> = Vec::new();
        let mut condition_nodes: Vec<(TextSize, FlowId)> = Vec::new();
        let mut cast_nodes: Vec<(TextSize, FlowId)> = Vec::new();
        let mut branch_nodes: Vec<(TextSize, FlowId)> = Vec::new();
        let mut unpositioned_branch = false;
        let has_complex_control = flow_nodes.iter().any(|node| {
            matches!(
                node.kind,
                FlowNodeKind::LoopLabel
                    | FlowNodeKind::NamedLabel(_)
                    | FlowNodeKind::ForIStat(_)
                    | FlowNodeKind::Break
                    | FlowNodeKind::Continue
            )
        });
        for node in &flow_nodes {
            let flow_id = node.id;
            if matches!(node.antecedent, Some(FlowAntecedent::Multiple(_))) {
                match flow_node_position(&node.kind) {
                    Some(pos) => branch_nodes.push((pos, flow_id)),
                    None => unpositioned_branch = true,
                }
            }
            match &node.kind {
                FlowNodeKind::Assignment(assign_ptr) => {
                    let assign_pos = assign_ptr.get_syntax_id().get_range().start();
                    for effect in node_effects
                        .get(&flow_id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[])
                    {
                        match effect {
                            FlowEffect::AssignDecl { decl, .. } => {
                                decl_assignments
                                    .entry(decl.clone())
                                    .or_default()
                                    .push((assign_pos, flow_id));
                            }
                            FlowEffect::AssignMember { member, .. } => {
                                member_assignments
                                    .entry(member.clone())
                                    .or_default()
                                    .push((assign_pos, flow_id));
                            }
                            _ => {}
                        }
                    }
                }
                FlowNodeKind::DeclPosition(pos) => decl_positions.push((*pos, flow_id)),
                FlowNodeKind::TrueCondition(cond) | FlowNodeKind::FalseCondition(cond) => {
                    condition_nodes.push((cond.get_syntax_id().get_range().start(), flow_id));
                }
                FlowNodeKind::TagCast(cast) => {
                    cast_nodes.push((cast.get_syntax_id().get_range().start(), flow_id));
                }
                FlowNodeKind::AsCast(as_cast) => {
                    cast_nodes.push((as_cast.get_syntax_id().get_range().start(), flow_id));
                }
                _ => {}
            }
        }
        for entries in decl_assignments.values_mut() {
            entries.sort_by_key(|(pos, _)| *pos);
        }
        for entries in member_assignments.values_mut() {
            entries.sort_by_key(|(pos, _)| *pos);
        }
        decl_positions.sort_by_key(|(pos, _)| *pos);
        condition_nodes.sort_by_key(|(pos, _)| *pos);
        cast_nodes.sort_by_key(|(pos, _)| *pos);
        branch_nodes.sort_by_key(|(pos, _)| *pos);
        let has_tag_cast = !cast_nodes.is_empty();
        let mut binding_ranges: Vec<(TextRange, FlowId)> = bindings
            .iter()
            .map(|(syntax_id, flow_id)| (syntax_id.get_range(), *flow_id))
            .collect();
        binding_ranges.sort_by_key(|(range, _)| (range.start(), range.end()));
        let mut binding_starts = HashMap::new();
        for (range, flow_id) in &binding_ranges {
            binding_starts.entry(range.start()).or_insert(*flow_id);
        }
        Self {
            decl_bind_expr_ref,
            decl_multi_return_ref,
            flow_nodes,
            node_effects,
            has_tag_cast,
            multiple_antecedents,
            bindings,
            binding_ranges,
            binding_starts,
            decl_assignments,
            member_assignments,
            decl_positions,
            condition_nodes,
            cast_nodes,
            branch_nodes,
            unpositioned_branch,
            has_complex_control,
        }
    }

    pub fn get_flow_id(&self, syntax_id: LuaSyntaxId) -> Option<FlowId> {
        self.bindings.get(&syntax_id).cloned()
    }

    /// The flow node for the deepest statement (a bindings key) containing `offset`.
    pub fn get_flow_id_at(&self, offset: TextSize) -> Option<FlowId> {
        if let Some(flow_id) = self.binding_starts.get(&offset) {
            return Some(*flow_id);
        }
        let idx = self
            .binding_ranges
            .partition_point(|(range, _)| range.start() <= offset);
        self.binding_ranges[..idx]
            .iter()
            .filter(|(range, _)| range.contains(offset))
            .min_by_key(|(range, _)| range.len())
            .map(|(_, flow_id)| *flow_id)
    }

    pub fn get_flow_node(&self, flow_id: FlowId) -> Option<&FlowNode> {
        self.flow_nodes.get(flow_id.0 as usize)
    }

    /// Effect summaries on flow nodes (declaration assignment / guard / tag cast).
    pub fn get_flow_effects(&self, flow_id: FlowId) -> &[FlowEffect] {
        self.node_effects
            .get(&flow_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Total number of CFG nodes.
    pub fn node_count(&self) -> u32 {
        self.flow_nodes.len() as u32
    }

    pub fn has_tag_cast(&self) -> bool {
        self.has_tag_cast
    }

    /// Whether the flow tree contains any narrowing condition node.
    pub fn has_condition(&self) -> bool {
        !self.condition_nodes.is_empty()
    }

    /// Latest `(assignment position, flow id)` for `decl` at or before `offset`.
    pub fn latest_decl_assignment(
        &self,
        decl: &SemanticId,
        offset: TextSize,
    ) -> Option<(TextSize, FlowId)> {
        let entries = self.decl_assignments.get(decl)?;
        let idx = entries.partition_point(|(pos, _)| *pos <= offset);
        idx.checked_sub(1).map(|idx| entries[idx])
    }

    /// Latest `(assignment position, flow id)` for `member` at or before `offset`.
    pub fn latest_member_assignment(
        &self,
        member: &SemanticId,
        offset: TextSize,
    ) -> Option<(TextSize, FlowId)> {
        let entries = self.member_assignments.get(member)?;
        let idx = entries.partition_point(|(pos, _)| *pos <= offset);
        idx.checked_sub(1).map(|idx| entries[idx])
    }

    /// Exact flow id for a `DeclPosition` at `position`, if any.
    pub fn decl_position_flow(&self, position: TextSize) -> Option<FlowId> {
        let idx = self
            .decl_positions
            .partition_point(|(pos, _)| *pos < position);
        self.decl_positions
            .get(idx)
            .filter(|(pos, _)| *pos == position)
            .map(|(_, flow_id)| *flow_id)
    }

    /// Whether any flow event that can change the queried value lies in the
    /// position range: conditions, casts, or a branch merge.
    ///
    /// `include_start` selects `start` inclusion; `end` is always inclusive.
    /// When a merge node has no indexable position, every range conservatively
    /// counts as blocked, so correctness is preserved.
    pub fn has_flow_event_between(
        &self,
        start: TextSize,
        end: TextSize,
        include_start: bool,
    ) -> bool {
        if self.unpositioned_branch {
            return true;
        }
        has_node_between(&self.condition_nodes, start, end, include_start)
            || has_node_between(&self.cast_nodes, start, end, include_start)
            || has_node_between(&self.branch_nodes, start, end, include_start)
    }

    /// Whether any assignment to `decl` occurs in the flow tree.
    pub fn has_decl_assignment(&self, decl: &SemanticId) -> bool {
        self.decl_assignments.contains_key(decl)
    }

    /// Whether any assignment to `member` occurs in the flow tree.
    pub fn has_member_assignment(&self, member: &SemanticId) -> bool {
        self.member_assignments.contains_key(member)
    }

    /// Whether loops / labels / break / continue may create non-monotonic flow edges.
    pub fn has_complex_control(&self) -> bool {
        self.has_complex_control
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

    pub fn get_decl_ref_expr(&self, decl_id: &SemanticId) -> Option<LuaAstPtr<LuaExpr>> {
        self.decl_bind_expr_ref.get(decl_id).cloned()
    }

    pub fn has_shared_multi_return_refs(
        &self,
        left_decl_id: &SemanticId,
        right_decl_id: &SemanticId,
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
        discriminant_decl_id: &SemanticId,
        target_decl_id: &SemanticId,
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
        decl_id: &SemanticId,
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
        decl_id: &SemanticId,
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

    pub fn get_decl_multi_return_ref_on_flow(
        &self,
        decl_id: &SemanticId,
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
        decl_id: &SemanticId,
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

/// Best-effort AST position for a flow node kind; `None` for label/terminal nodes.
fn flow_node_position(kind: &FlowNodeKind) -> Option<TextSize> {
    match kind {
        FlowNodeKind::Start
        | FlowNodeKind::Unreachable
        | FlowNodeKind::BranchLabel
        | FlowNodeKind::LoopLabel
        | FlowNodeKind::NamedLabel(_)
        | FlowNodeKind::Break
        | FlowNodeKind::Continue
        | FlowNodeKind::Return => None,
        FlowNodeKind::DeclPosition(pos) => Some(*pos),
        FlowNodeKind::Assignment(ptr) => Some(ptr.get_syntax_id().get_range().start()),
        FlowNodeKind::CallExprStat(ptr) => Some(ptr.get_syntax_id().get_range().start()),
        FlowNodeKind::TrueCondition(ptr) | FlowNodeKind::FalseCondition(ptr) => {
            Some(ptr.get_syntax_id().get_range().start())
        }
        FlowNodeKind::ImplFunc(ptr) => Some(ptr.get_syntax_id().get_range().start()),
        FlowNodeKind::ForIStat(ptr) => Some(ptr.get_syntax_id().get_range().start()),
        FlowNodeKind::TagCast(ptr) => Some(ptr.get_syntax_id().get_range().start()),
        FlowNodeKind::AsCast(ptr) => Some(ptr.get_syntax_id().get_range().start()),
    }
}

/// Whether a sorted `(position, flow id)` list contains any position in range.
///
/// `include_start` selects `start` inclusion; `end` is always inclusive.
fn has_node_between(
    nodes: &[(TextSize, FlowId)],
    start: TextSize,
    end: TextSize,
    include_start: bool,
) -> bool {
    if start > end {
        return false;
    }
    let right = nodes.partition_point(|(pos, _)| *pos <= end);
    let left = if include_start {
        nodes[..right].partition_point(|(pos, _)| *pos < start)
    } else {
        nodes[..right].partition_point(|(pos, _)| *pos <= start)
    };
    left < right
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclMultiReturnRef {
    pub call_expr: LuaAstPtr<LuaCallExpr>,
    pub return_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclMultiReturnRefAt {
    pub position: TextSize,
    pub flow_id: FlowId,
    pub reference: Option<DeclMultiReturnRef>,
}
