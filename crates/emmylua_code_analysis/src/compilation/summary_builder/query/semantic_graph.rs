use std::collections::{BTreeMap, BTreeSet};

use rowan::TextSize;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};
use super::{
    SalsaMemberTypeQueryIndex, SalsaSemanticTargetSummary, SalsaSignatureExplainIndex,
    SalsaSignatureReturnQueryIndex, find_call_explain_at, find_member_use_at, find_name_use_at,
    find_signature_return_query_at,
};
use crate::{
    SalsaDeclTreeSummary, SalsaForRangeIterQueryIndex, SalsaNameUseResolutionSummary,
    SalsaSemanticGraphEdgeKindSummary, SalsaSemanticGraphEdgeSummary,
    SalsaSemanticGraphNodeSummary, SalsaSemanticGraphSummary, SalsaSignatureIndexSummary,
    SalsaSingleFileSemanticSummary, SalsaUseSiteIndexSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticGraphQueryIndex {
    pub graph: SalsaSemanticGraphSummary,
    pub scc: SalsaSemanticGraphSccIndex,
    by_from: Vec<SalsaLookupBucket<SalsaSemanticGraphNodeSummary>>,
    by_to: Vec<SalsaLookupBucket<SalsaSemanticGraphNodeSummary>>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticGraphSccComponentSummary {
    pub component_id: usize,
    pub nodes: Vec<SalsaSemanticGraphNodeSummary>,
    pub successor_component_ids: Vec<usize>,
    pub predecessor_component_ids: Vec<usize>,
    pub is_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticGraphSccIndex {
    pub components: Vec<SalsaSemanticGraphSccComponentSummary>,
    pub topo_order: Vec<usize>,
    by_node: Vec<SalsaLookupBucket<SalsaSemanticGraphNodeSummary>>,
}

pub fn build_semantic_graph_summary(
    decl_tree: &SalsaDeclTreeSummary,
    member_types: &SalsaMemberTypeQueryIndex,
    signatures: &SalsaSignatureIndexSummary,
    for_range_iters: &SalsaForRangeIterQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    signature_returns: &SalsaSignatureReturnQueryIndex,
    semantic: &SalsaSingleFileSemanticSummary,
) -> SalsaSemanticGraphSummary {
    let mut nodes = BTreeSet::new();
    let mut edges = BTreeSet::new();

    for decl in &decl_tree.decls {
        nodes.insert(SalsaSemanticGraphNodeSummary::DeclValue(decl.id));
    }

    for member in &member_types.members {
        nodes.insert(SalsaSemanticGraphNodeSummary::MemberValue(
            member.target.clone(),
        ));
    }

    for signature in &signatures.signatures {
        nodes.insert(SalsaSemanticGraphNodeSummary::SignatureReturn(
            signature.syntax_offset,
        ));
    }

    for for_range_iter in &for_range_iters.loops {
        nodes.insert(SalsaSemanticGraphNodeSummary::ForRangeIter(
            for_range_iter.loop_offset,
        ));
    }

    if semantic.module_export.is_some() {
        nodes.insert(SalsaSemanticGraphNodeSummary::ModuleExport);
    }

    if let Some(module_export) = &semantic.module_export
        && let Some(target_node) = semantic_target_node(module_export.semantic_target.clone())
        && nodes.contains(&target_node)
    {
        edges.insert(SalsaSemanticGraphEdgeSummary {
            from: SalsaSemanticGraphNodeSummary::ModuleExport,
            to: target_node,
            kind: SalsaSemanticGraphEdgeKindSummary::ExportTarget,
        });
    }

    build_decl_initializer_edges(
        decl_tree,
        use_sites,
        signature_explain_index,
        &nodes,
        &mut edges,
    );
    build_member_initializer_edges(
        member_types,
        use_sites,
        signature_explain_index,
        &nodes,
        &mut edges,
    );
    build_for_range_iter_edges(
        for_range_iters,
        use_sites,
        signature_explain_index,
        &nodes,
        &mut edges,
    );

    for signature in &signatures.signatures {
        let from = SalsaSemanticGraphNodeSummary::SignatureReturn(signature.syntax_offset);
        let Some(signature_return) =
            find_signature_return_query_at(signature_returns, signature.syntax_offset)
        else {
            continue;
        };

        for value in signature_return.values {
            build_signature_return_edges_for_value_expr(
                &from,
                value.expr_offset,
                use_sites,
                &nodes,
                &mut edges,
            );

            let Some(call) = value.call else {
                continue;
            };

            if let Some(resolved_signature_offset) = call.resolved_signature_offset {
                let to = SalsaSemanticGraphNodeSummary::SignatureReturn(resolved_signature_offset);
                if nodes.contains(&to) {
                    edges.insert(SalsaSemanticGraphEdgeSummary {
                        from: from.clone(),
                        to,
                        kind: SalsaSemanticGraphEdgeKindSummary::ResolvedCallReturn,
                    });
                }
            }

            for candidate_signature_offset in call.candidate_signature_offsets {
                let to = SalsaSemanticGraphNodeSummary::SignatureReturn(candidate_signature_offset);
                if nodes.contains(&to) {
                    edges.insert(SalsaSemanticGraphEdgeSummary {
                        from: from.clone(),
                        to,
                        kind: SalsaSemanticGraphEdgeKindSummary::CandidateCallReturn,
                    });
                }
            }
        }
    }

    SalsaSemanticGraphSummary {
        nodes: nodes.into_iter().collect(),
        edges: edges.into_iter().collect(),
    }
}

fn build_signature_return_edges_for_value_expr(
    from: &SalsaSemanticGraphNodeSummary,
    expr_offset: TextSize,
    use_sites: &SalsaUseSiteIndexSummary,
    nodes: &BTreeSet<SalsaSemanticGraphNodeSummary>,
    edges: &mut BTreeSet<SalsaSemanticGraphEdgeSummary>,
) {
    if let Some(member_use) = find_member_use_at(use_sites, expr_offset) {
        let to = SalsaSemanticGraphNodeSummary::MemberValue(member_use.target);
        if nodes.contains(&to) {
            edges.insert(SalsaSemanticGraphEdgeSummary {
                from: from.clone(),
                to,
                kind: SalsaSemanticGraphEdgeKindSummary::SignatureReturnMember,
            });
            return;
        }
    }

    if let Some(name_use) = find_name_use_at(use_sites, expr_offset)
        && let SalsaNameUseResolutionSummary::LocalDecl(target_decl_id) = name_use.resolution
    {
        let to = SalsaSemanticGraphNodeSummary::DeclValue(target_decl_id);
        if nodes.contains(&to) {
            edges.insert(SalsaSemanticGraphEdgeSummary {
                from: from.clone(),
                to,
                kind: SalsaSemanticGraphEdgeKindSummary::SignatureReturnDecl,
            });
        }
    }
}

pub fn build_semantic_graph_query_index(
    graph: &SalsaSemanticGraphSummary,
) -> SalsaSemanticGraphQueryIndex {
    let mut from_entries = Vec::with_capacity(graph.edges.len());
    let mut to_entries = Vec::with_capacity(graph.edges.len());

    for (index, edge) in graph.edges.iter().enumerate() {
        from_entries.push((edge.from.clone(), index));
        to_entries.push((edge.to.clone(), index));
    }

    SalsaSemanticGraphQueryIndex {
        graph: graph.clone(),
        scc: build_semantic_graph_scc_index(graph),
        by_from: build_lookup_buckets(from_entries),
        by_to: build_lookup_buckets(to_entries),
    }
}

pub fn build_semantic_graph_scc_index(
    graph: &SalsaSemanticGraphSummary,
) -> SalsaSemanticGraphSccIndex {
    let node_indices = graph
        .nodes
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, node)| (node, index))
        .collect::<BTreeMap<_, _>>();
    let mut adjacency = vec![Vec::new(); graph.nodes.len()];

    for edge in &graph.edges {
        let Some(&from_index) = node_indices.get(&edge.from) else {
            continue;
        };
        let Some(&to_index) = node_indices.get(&edge.to) else {
            continue;
        };
        adjacency[from_index].push(to_index);
    }

    let mut tarjan = TarjanSccBuilder::new(&adjacency);
    for node_index in 0..graph.nodes.len() {
        tarjan.visit(node_index);
    }

    let mut node_to_component = vec![0usize; graph.nodes.len()];
    for (component_id, component_node_indices) in tarjan.components.iter().enumerate() {
        for &node_index in component_node_indices {
            node_to_component[node_index] = component_id;
        }
    }

    let mut successor_sets = vec![BTreeSet::new(); tarjan.components.len()];
    let mut predecessor_sets = vec![BTreeSet::new(); tarjan.components.len()];
    let mut self_loops = vec![false; tarjan.components.len()];
    for edge in &graph.edges {
        let Some(&from_index) = node_indices.get(&edge.from) else {
            continue;
        };
        let Some(&to_index) = node_indices.get(&edge.to) else {
            continue;
        };
        let from_component = node_to_component[from_index];
        let to_component = node_to_component[to_index];
        if from_component == to_component {
            if from_index == to_index {
                self_loops[from_component] = true;
            }
            continue;
        }

        successor_sets[from_component].insert(to_component);
        predecessor_sets[to_component].insert(from_component);
    }

    let topo_order = build_component_topo_order(&successor_sets);

    let components = tarjan
        .components
        .into_iter()
        .enumerate()
        .map(|(component_id, component_node_indices)| {
            let mut nodes = component_node_indices
                .into_iter()
                .map(|node_index| graph.nodes[node_index].clone())
                .collect::<Vec<_>>();
            nodes.sort();

            SalsaSemanticGraphSccComponentSummary {
                component_id,
                nodes,
                successor_component_ids: successor_sets[component_id].iter().copied().collect(),
                predecessor_component_ids: predecessor_sets[component_id].iter().copied().collect(),
                is_cycle: self_loops[component_id] || tarjan.component_sizes[component_id] > 1,
            }
        })
        .collect::<Vec<_>>();
    let by_node = build_lookup_buckets(
        graph
            .nodes
            .iter()
            .cloned()
            .enumerate()
            .map(|(node_index, node)| (node, node_to_component[node_index]))
            .collect(),
    );

    SalsaSemanticGraphSccIndex {
        components,
        topo_order,
        by_node,
    }
}

pub fn find_semantic_graph_scc_component_by_id(
    index: &SalsaSemanticGraphSccIndex,
    component_id: usize,
) -> Option<SalsaSemanticGraphSccComponentSummary> {
    index.components.get(component_id).cloned()
}

pub fn find_semantic_graph_scc_component(
    index: &SalsaSemanticGraphSccIndex,
    node: &SalsaSemanticGraphNodeSummary,
) -> Option<SalsaSemanticGraphSccComponentSummary> {
    let component_id = find_bucket_indices(&index.by_node, node)?
        .first()
        .copied()?;
    index.components.get(component_id).cloned()
}

pub fn collect_semantic_graph_scc_successor_components(
    index: &SalsaSemanticGraphSccIndex,
    component_id: usize,
) -> Vec<SalsaSemanticGraphSccComponentSummary> {
    index
        .components
        .get(component_id)
        .into_iter()
        .flat_map(|component| component.successor_component_ids.iter().copied())
        .filter_map(|successor_component_id| {
            find_semantic_graph_scc_component_by_id(index, successor_component_id)
        })
        .collect()
}

pub fn collect_semantic_graph_scc_predecessor_components(
    index: &SalsaSemanticGraphSccIndex,
    component_id: usize,
) -> Vec<SalsaSemanticGraphSccComponentSummary> {
    index
        .components
        .get(component_id)
        .into_iter()
        .flat_map(|component| component.predecessor_component_ids.iter().copied())
        .filter_map(|predecessor_component_id| {
            find_semantic_graph_scc_component_by_id(index, predecessor_component_id)
        })
        .collect()
}

pub fn collect_outgoing_semantic_graph_edges(
    index: &SalsaSemanticGraphQueryIndex,
    from: &SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphEdgeSummary> {
    collect_graph_edges(index, find_bucket_indices(&index.by_from, from))
}

pub fn collect_incoming_semantic_graph_edges(
    index: &SalsaSemanticGraphQueryIndex,
    to: &SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphEdgeSummary> {
    collect_graph_edges(index, find_bucket_indices(&index.by_to, to))
}

pub fn collect_semantic_graph_successor_nodes(
    index: &SalsaSemanticGraphQueryIndex,
    from: &SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphNodeSummary> {
    let mut nodes = collect_outgoing_semantic_graph_edges(index, from)
        .into_iter()
        .map(|edge| edge.to)
        .collect::<Vec<_>>();
    nodes.sort();
    nodes.dedup();
    nodes
}

pub fn collect_semantic_graph_predecessor_nodes(
    index: &SalsaSemanticGraphQueryIndex,
    to: &SalsaSemanticGraphNodeSummary,
) -> Vec<SalsaSemanticGraphNodeSummary> {
    let mut nodes = collect_incoming_semantic_graph_edges(index, to)
        .into_iter()
        .map(|edge| edge.from)
        .collect::<Vec<_>>();
    nodes.sort();
    nodes.dedup();
    nodes
}

fn semantic_target_node(
    target: Option<SalsaSemanticTargetSummary>,
) -> Option<SalsaSemanticGraphNodeSummary> {
    match target? {
        SalsaSemanticTargetSummary::Decl(decl_id) => {
            Some(SalsaSemanticGraphNodeSummary::DeclValue(decl_id))
        }
        SalsaSemanticTargetSummary::Member(member_target) => {
            Some(SalsaSemanticGraphNodeSummary::MemberValue(member_target))
        }
        SalsaSemanticTargetSummary::Signature(signature_offset) => Some(
            SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset),
        ),
    }
}

fn build_decl_initializer_edges(
    decl_tree: &SalsaDeclTreeSummary,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    nodes: &BTreeSet<SalsaSemanticGraphNodeSummary>,
    edges: &mut BTreeSet<SalsaSemanticGraphEdgeSummary>,
) {
    for decl in &decl_tree.decls {
        let Some(value_expr_offset) = decl.value_expr_offset() else {
            continue;
        };

        let from = SalsaSemanticGraphNodeSummary::DeclValue(decl.id);
        build_initializer_edges_for_value_expr(
            &from,
            value_expr_offset,
            use_sites,
            signature_explain_index,
            nodes,
            edges,
        );
    }
}

fn build_member_initializer_edges(
    member_types: &SalsaMemberTypeQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    nodes: &BTreeSet<SalsaSemanticGraphNodeSummary>,
    edges: &mut BTreeSet<SalsaSemanticGraphEdgeSummary>,
) {
    for member_type in &member_types.members {
        let from = SalsaSemanticGraphNodeSummary::MemberValue(member_type.target.clone());
        for candidate in &member_type.candidates {
            let Some(value_expr_offset) = candidate.initializer_offset else {
                continue;
            };

            build_initializer_edges_for_value_expr(
                &from,
                value_expr_offset,
                use_sites,
                signature_explain_index,
                nodes,
                edges,
            );
        }
    }
}

fn build_for_range_iter_edges(
    for_range_iters: &SalsaForRangeIterQueryIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    nodes: &BTreeSet<SalsaSemanticGraphNodeSummary>,
    edges: &mut BTreeSet<SalsaSemanticGraphEdgeSummary>,
) {
    for for_range_iter in &for_range_iters.loops {
        let from = SalsaSemanticGraphNodeSummary::ForRangeIter(for_range_iter.loop_offset);
        for expr_offset in &for_range_iter.iter_expr_offsets {
            build_initializer_edges_for_value_expr(
                &from,
                *expr_offset,
                use_sites,
                signature_explain_index,
                nodes,
                edges,
            );
        }
    }
}

fn build_initializer_edges_for_value_expr(
    from: &SalsaSemanticGraphNodeSummary,
    expr_offset: TextSize,
    use_sites: &SalsaUseSiteIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    nodes: &BTreeSet<SalsaSemanticGraphNodeSummary>,
    edges: &mut BTreeSet<SalsaSemanticGraphEdgeSummary>,
) {
    if let Some(member_use) = find_member_use_at(use_sites, expr_offset) {
        let dependency = SalsaSemanticGraphNodeSummary::MemberValue(member_use.target);
        if nodes.contains(&dependency) {
            edges.insert(SalsaSemanticGraphEdgeSummary {
                from: dependency,
                to: from.clone(),
                kind: SalsaSemanticGraphEdgeKindSummary::InitializerMember,
            });
            return;
        }
    }

    let Some(call) = find_call_explain_at(signature_explain_index, expr_offset) else {
        if let Some(name_use) = find_name_use_at(use_sites, expr_offset) {
            if let SalsaNameUseResolutionSummary::LocalDecl(target_decl_id) = name_use.resolution {
                let dependency = SalsaSemanticGraphNodeSummary::DeclValue(target_decl_id);
                if nodes.contains(&dependency) {
                    edges.insert(SalsaSemanticGraphEdgeSummary {
                        from: dependency,
                        to: from.clone(),
                        kind: SalsaSemanticGraphEdgeKindSummary::InitializerDecl,
                    });
                }
            }
        }
        return;
    };

    if let Some(resolved_signature_offset) = call.resolved_signature_offset {
        let dependency = SalsaSemanticGraphNodeSummary::SignatureReturn(resolved_signature_offset);
        if nodes.contains(&dependency) {
            edges.insert(SalsaSemanticGraphEdgeSummary {
                from: dependency,
                to: from.clone(),
                kind: SalsaSemanticGraphEdgeKindSummary::InitializerResolvedCallReturn,
            });
        }
    }

    for candidate_signature_offset in call.candidate_signature_offsets {
        let dependency = SalsaSemanticGraphNodeSummary::SignatureReturn(candidate_signature_offset);
        if nodes.contains(&dependency) {
            edges.insert(SalsaSemanticGraphEdgeSummary {
                from: dependency,
                to: from.clone(),
                kind: SalsaSemanticGraphEdgeKindSummary::InitializerCandidateCallReturn,
            });
        }
    }

    if let Some(name_use) = find_name_use_at(use_sites, expr_offset) {
        if let SalsaNameUseResolutionSummary::LocalDecl(target_decl_id) = name_use.resolution {
            let dependency = SalsaSemanticGraphNodeSummary::DeclValue(target_decl_id);
            if nodes.contains(&dependency) {
                edges.insert(SalsaSemanticGraphEdgeSummary {
                    from: dependency,
                    to: from.clone(),
                    kind: SalsaSemanticGraphEdgeKindSummary::InitializerDecl,
                });
            }
        }
    }
}

fn collect_graph_edges(
    index: &SalsaSemanticGraphQueryIndex,
    edge_indices: Option<&[usize]>,
) -> Vec<SalsaSemanticGraphEdgeSummary> {
    edge_indices
        .into_iter()
        .flatten()
        .map(|edge_index| index.graph.edges[*edge_index].clone())
        .collect()
}

fn build_component_topo_order(successor_sets: &[BTreeSet<usize>]) -> Vec<usize> {
    let mut indegrees = vec![0usize; successor_sets.len()];
    for successor_set in successor_sets {
        for &successor_component_id in successor_set {
            indegrees[successor_component_id] += 1;
        }
    }

    let mut ready = indegrees
        .iter()
        .enumerate()
        .filter_map(|(component_id, indegree)| (*indegree == 0).then_some(component_id))
        .collect::<Vec<_>>();
    let mut topo_order = Vec::with_capacity(successor_sets.len());
    let mut ready_index = 0;

    while let Some(&component_id) = ready.get(ready_index) {
        ready_index += 1;
        topo_order.push(component_id);
        for &successor_component_id in &successor_sets[component_id] {
            indegrees[successor_component_id] -= 1;
            if indegrees[successor_component_id] == 0 {
                ready.push(successor_component_id);
            }
        }
    }

    topo_order
}

struct TarjanSccBuilder<'a> {
    adjacency: &'a [Vec<usize>],
    next_index: usize,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    indices: Vec<Option<usize>>,
    low_links: Vec<usize>,
    components: Vec<Vec<usize>>,
    component_sizes: Vec<usize>,
}

impl<'a> TarjanSccBuilder<'a> {
    fn new(adjacency: &'a [Vec<usize>]) -> Self {
        let node_count = adjacency.len();
        Self {
            adjacency,
            next_index: 0,
            stack: Vec::new(),
            on_stack: vec![false; node_count],
            indices: vec![None; node_count],
            low_links: vec![0; node_count],
            components: Vec::new(),
            component_sizes: Vec::new(),
        }
    }

    fn visit(&mut self, node_index: usize) {
        if self.indices[node_index].is_some() {
            return;
        }

        self.indices[node_index] = Some(self.next_index);
        self.low_links[node_index] = self.next_index;
        self.next_index += 1;
        self.stack.push(node_index);
        self.on_stack[node_index] = true;

        for &successor_index in &self.adjacency[node_index] {
            if self.indices[successor_index].is_none() {
                self.visit(successor_index);
                self.low_links[node_index] =
                    self.low_links[node_index].min(self.low_links[successor_index]);
            } else if self.on_stack[successor_index]
                && let Some(successor_discovery_index) = self.indices[successor_index]
            {
                self.low_links[node_index] =
                    self.low_links[node_index].min(successor_discovery_index);
            }
        }

        if self.low_links[node_index] != self.indices[node_index].unwrap_or_default() {
            return;
        }

        let mut component = Vec::new();
        loop {
            let stacked_node_index = self.stack.pop().expect("tarjan stack underflow");
            self.on_stack[stacked_node_index] = false;
            component.push(stacked_node_index);
            if stacked_node_index == node_index {
                break;
            }
        }

        self.component_sizes.push(component.len());
        self.components.push(component);
    }
}
