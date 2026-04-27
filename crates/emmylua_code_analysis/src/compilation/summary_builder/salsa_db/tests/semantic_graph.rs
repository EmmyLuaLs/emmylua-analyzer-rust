use super::*;

#[test]
fn test_summary_builder_semantic_graph_tracks_module_export_target() {
    let mut compilation = setup_compilation();
    let source = r#"local function factory()
  return 1
end

return factory
"#;
    set_test_file(
        &mut compilation,
        501,
        "C:/ws/semantic_graph_module.lua",
        source,
    );

    let graph = compilation
        .semantic()
        .file()
        .graph(FileId::new(501))
        .expect("semantic graph");

    assert!(
        graph
            .nodes
            .iter()
            .any(|node| matches!(node, crate::SalsaSemanticGraphNodeSummary::ModuleExport))
    );
    assert!(graph.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            from: crate::SalsaSemanticGraphNodeSummary::ModuleExport,
            kind: crate::SalsaSemanticGraphEdgeKindSummary::ExportTarget,
            ..
        }
    )));
}

#[test]
fn test_summary_builder_semantic_graph_tracks_nested_property_member_initializer_edges() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local source

local box = { value = source }
local holder = {
  nested = {
    enabled = box.value,
  },
}
"#;
    set_test_file(
        &mut compilation,
        510,
        "C:/ws/semantic_graph_nested_property_member_initializer.lua",
        source,
    );

    let decls = compilation
        .file()
        .decl_tree(FileId::new(510))
        .expect("decl tree")
        .decls
        .clone();
    let box_decl = decls
        .iter()
        .find(|decl| decl.name == "box" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .expect("box decl");
    let holder_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "holder" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("holder decl");
    let source_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "box".into(),
            decl_id: box_decl.id,
        },
        owner_segments: Vec::new().into(),
        member_name: "value".into(),
    });
    let nested_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: vec!["nested".into()].into(),
        member_name: "enabled".into(),
    });

    let outgoing = compilation
        .semantic()
        .file()
        .graph_outgoing_edges(
            FileId::new(510),
            crate::SalsaSemanticGraphNodeSummary::MemberValue(source_member.clone()),
        )
        .expect("source member outgoing edges");

    assert!(outgoing.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            to: crate::SalsaSemanticGraphNodeSummary::MemberValue(member_target),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::InitializerMember,
            ..
        } if *member_target == nested_member
    )));
}

#[test]
fn test_summary_builder_semantic_graph_tracks_nested_property_call_and_multihop_member_initializer_edges()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local source

---@return string
local function make()
  return source
end

local outer = {
  nested = {
    value = source,
  },
}

local holder = {
  nested = {
    made = make(),
    forwarded = outer.nested.value,
  },
}
"#;
    set_test_file(
        &mut compilation,
        511,
        "C:/ws/semantic_graph_nested_property_call_multihop.lua",
        source,
    );

    let decls = compilation
        .file()
        .decl_tree(FileId::new(511))
        .expect("decl tree")
        .decls
        .clone();
    let outer_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "outer" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("outer decl");
    let holder_decl = decls
        .iter()
        .find(|decl| {
            decl.name == "holder" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("holder decl");
    let multihop_source_member =
        crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
            root: crate::SalsaMemberRootSummary::LocalDecl {
                name: "outer".into(),
                decl_id: outer_decl.id,
            },
            owner_segments: vec!["nested".into()].into(),
            member_name: "value".into(),
        });
    let forwarded_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: vec!["nested".into()].into(),
        member_name: "forwarded".into(),
    });
    let made_member = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: vec!["nested".into()].into(),
        member_name: "made".into(),
    });
    let make_signature = compilation
        .doc()
        .signatures(FileId::new(511))
        .expect("signature summary")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("make"))
        .expect("make signature");

    let multihop_outgoing = compilation
        .semantic()
        .file()
        .graph_outgoing_edges(
            FileId::new(511),
            crate::SalsaSemanticGraphNodeSummary::MemberValue(multihop_source_member.clone()),
        )
        .expect("multihop source member outgoing edges");
    assert!(multihop_outgoing.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            to: crate::SalsaSemanticGraphNodeSummary::MemberValue(member_target),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::InitializerMember,
            ..
        } if *member_target == forwarded_member
    )));

    let signature_outgoing = compilation
        .semantic()
        .file()
        .graph_outgoing_edges(
            FileId::new(511),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(make_signature.syntax_offset),
        )
        .expect("make signature outgoing edges");
    assert!(signature_outgoing.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            to: crate::SalsaSemanticGraphNodeSummary::MemberValue(member_target),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::InitializerResolvedCallReturn,
            ..
        } if *member_target == made_member
    )));
}

#[test]
fn test_summary_builder_semantic_graph_tracks_signature_call_edges() {
    let mut compilation = setup_compilation();
    let source = r#"local function callee()
  return 1
end

local function caller()
  return callee()
end
"#;
    set_test_file(
        &mut compilation,
        502,
        "C:/ws/semantic_graph_signature.lua",
        source,
    );

    let graph = compilation
        .semantic()
        .file()
        .graph(FileId::new(502))
        .expect("semantic graph");

    assert!(graph.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            from: crate::SalsaSemanticGraphNodeSummary::SignatureReturn(_),
            to: crate::SalsaSemanticGraphNodeSummary::SignatureReturn(_),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::ResolvedCallReturn,
        }
    )));
}

#[test]
fn test_summary_builder_semantic_graph_tracks_signature_name_and_member_edges() {
    let mut compilation = setup_compilation();
    let source = r#"local value = 1
local holder = {}
holder.alias = 1

local function from_name()
  return value
end

local function from_member()
  return holder.alias
end
"#;
    set_test_file(
        &mut compilation,
        508,
        "C:/ws/semantic_graph_signature_targets.lua",
        source,
    );

    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(508))
        .expect("decl tree");
    let value_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name.as_str() == "value")
        .expect("value decl");
    let alias_member = compilation
        .file()
        .members(FileId::new(508))
        .expect("member summary")
        .members
        .clone()
        .into_iter()
        .find(|member| member.target.member_name.as_str() == "alias")
        .expect("alias member");
    let signatures = compilation
        .doc()
        .signatures(FileId::new(508))
        .expect("signature summary")
        .signatures
        .clone();
    let from_name_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("from_name"))
        .expect("from_name signature");
    let from_member_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("from_member"))
        .expect("from_member signature");

    let graph = compilation
        .semantic()
        .file()
        .graph(FileId::new(508))
        .expect("semantic graph");

    assert!(graph.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            from: crate::SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset),
            to: crate::SalsaSemanticGraphNodeSummary::DeclValue(decl_id),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::SignatureReturnDecl,
        } if *signature_offset == from_name_signature.syntax_offset && *decl_id == value_decl.id
    )));
    assert!(graph.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            from: crate::SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset),
            to: crate::SalsaSemanticGraphNodeSummary::MemberValue(member_target),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::SignatureReturnMember,
        } if *signature_offset == from_member_signature.syntax_offset && *member_target == alias_member.target
    )));
}

#[test]
fn test_summary_builder_semantic_graph_includes_property_driven_member_nodes() {
    let mut compilation = setup_compilation();
    let source = r#"local holder = {
  run = function()
    return 1
  end,
}

local function from_member()
  return holder.run
end
"#;
    set_test_file(
        &mut compilation,
        509,
        "C:/ws/semantic_graph_property_member.lua",
        source,
    );

    let holder_decl = compilation
        .file()
        .decl_tree(FileId::new(509))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| decl.name.as_str() == "holder")
        .expect("holder decl");
    let run_member_target = crate::SalsaMemberTargetId::from(&crate::SalsaMemberTargetSummary {
        root: crate::SalsaMemberRootSummary::LocalDecl {
            name: "holder".into(),
            decl_id: holder_decl.id,
        },
        owner_segments: Vec::new().into(),
        member_name: "run".into(),
    });
    let from_member_signature = compilation
        .doc()
        .signatures(FileId::new(509))
        .expect("signature summary")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("from_member"))
        .expect("from_member signature");

    let graph = compilation
        .semantic()
        .file()
        .graph(FileId::new(509))
        .expect("semantic graph");

    assert!(graph.nodes.iter().any(|node| matches!(
        node,
        crate::SalsaSemanticGraphNodeSummary::MemberValue(member_target)
            if *member_target == run_member_target
    )));
    assert!(graph.edges.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            from: crate::SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset),
            to: crate::SalsaSemanticGraphNodeSummary::MemberValue(member_target),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::SignatureReturnMember,
        } if *signature_offset == from_member_signature.syntax_offset && *member_target == run_member_target
    )));
}

#[test]
fn test_summary_builder_semantic_graph_tracks_for_range_initializer_edges() {
    let mut compilation = setup_compilation();
    let source = r#"---@type fun(): string, integer
local iter

for key, extra in iter do
  print(key, extra)
end
"#;
    set_test_file(
        &mut compilation,
        507,
        "C:/ws/semantic_graph_for_range.lua",
        source,
    );

    let loop_offset = compilation
        .flow()
        .summary(FileId::new(507))
        .expect("flow summary")
        .loops
        .iter()
        .find(|loop_summary| matches!(loop_summary.kind, crate::SalsaFlowLoopKindSummary::ForRange))
        .map(|loop_summary| loop_summary.syntax_offset)
        .expect("for range loop");
    let iter_decl = compilation
        .file()
        .decl_tree(FileId::new(507))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| decl.name.as_str() == "iter")
        .expect("iter decl");

    let graph = compilation
        .semantic()
        .file()
        .graph(FileId::new(507))
        .expect("semantic graph");
    assert!(graph.nodes.iter().any(|node| matches!(
        node,
        crate::SalsaSemanticGraphNodeSummary::ForRangeIter(offset) if *offset == loop_offset
    )));

    let loop_successors = compilation
        .semantic()
        .file()
        .graph_predecessors(
            FileId::new(507),
            crate::SalsaSemanticGraphNodeSummary::ForRangeIter(loop_offset),
        )
        .expect("for range predecessors");
    assert!(loop_successors.iter().any(|node| matches!(
        node,
        crate::SalsaSemanticGraphNodeSummary::DeclValue(decl_id) if *decl_id == iter_decl.id
    )));
}

#[test]
fn test_summary_builder_semantic_graph_tracks_decl_initializer_edges() {
    let mut compilation = setup_compilation();
    let source = r#"local source = 1
local alias = source
local caller = callee()

local function callee()
  return source
end
"#;
    set_test_file(
        &mut compilation,
        503,
        "C:/ws/semantic_graph_decl_initializer.lua",
        source,
    );

    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(503))
        .expect("decl tree");
    let alias_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name.as_str() == "alias")
        .expect("alias decl");
    let caller_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name.as_str() == "caller")
        .expect("caller decl");
    let callee_signature = compilation
        .doc()
        .signatures(FileId::new(503))
        .expect("signatures")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("callee"))
        .expect("callee signature");

    let alias_predecessors = compilation
        .semantic()
        .file()
        .graph_predecessors(
            FileId::new(503),
            crate::SalsaSemanticGraphNodeSummary::DeclValue(alias_decl.id),
        )
        .expect("alias predecessors");
    assert!(alias_predecessors.iter().any(|node| matches!(
        node,
        crate::SalsaSemanticGraphNodeSummary::DeclValue(target_decl_id)
            if *target_decl_id != alias_decl.id
    )));

    let callee_outgoing = compilation
        .semantic()
        .file()
        .graph_outgoing_edges(
            FileId::new(503),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(callee_signature.syntax_offset),
        )
        .expect("callee outgoing edges");
    assert!(callee_outgoing.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            to: crate::SalsaSemanticGraphNodeSummary::DeclValue(decl_id),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::InitializerResolvedCallReturn,
            ..
        } if *decl_id == caller_decl.id
    )));

    let caller_predecessors = compilation
        .semantic()
        .file()
        .graph_predecessors(
            FileId::new(503),
            crate::SalsaSemanticGraphNodeSummary::DeclValue(caller_decl.id),
        )
        .expect("caller predecessors");
    assert!(caller_predecessors.iter().any(|node| matches!(
        node,
        crate::SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset)
            if *signature_offset == callee_signature.syntax_offset
    )));
}

#[test]
fn test_summary_builder_semantic_graph_tracks_member_initializer_edges() {
    let mut compilation = setup_compilation();
    let source = r#"local source = 1
local holder = {}
holder.alias = source
holder.forwarded = callee()

local function callee()
  return source
end
"#;
    set_test_file(
        &mut compilation,
        504,
        "C:/ws/semantic_graph_member_initializer.lua",
        source,
    );

    let members = compilation
        .file()
        .members(FileId::new(504))
        .expect("member summary");
    let alias_member = members
        .members
        .iter()
        .find(|member| member.target.member_name.as_str() == "alias")
        .expect("alias member");
    let forwarded_member = members
        .members
        .iter()
        .find(|member| member.target.member_name.as_str() == "forwarded")
        .expect("forwarded member");
    let callee_signature = compilation
        .doc()
        .signatures(FileId::new(504))
        .expect("signatures")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("callee"))
        .expect("callee signature");

    let alias_predecessors = compilation
        .semantic()
        .file()
        .graph_predecessors(
            FileId::new(504),
            crate::SalsaSemanticGraphNodeSummary::MemberValue(alias_member.target.clone()),
        )
        .expect("alias member predecessors");
    assert!(
        alias_predecessors
            .iter()
            .any(|node| matches!(node, crate::SalsaSemanticGraphNodeSummary::DeclValue(_)))
    );

    let callee_outgoing = compilation
        .semantic()
        .file()
        .graph_outgoing_edges(
            FileId::new(504),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(callee_signature.syntax_offset),
        )
        .expect("callee outgoing edges");
    assert!(callee_outgoing.iter().any(|edge| matches!(
        edge,
        crate::SalsaSemanticGraphEdgeSummary {
            to: crate::SalsaSemanticGraphNodeSummary::MemberValue(member_target),
            kind: crate::SalsaSemanticGraphEdgeKindSummary::InitializerResolvedCallReturn,
            ..
        } if *member_target == forwarded_member.target
    )));
}

#[test]
fn test_summary_builder_semantic_graph_builds_scc_components() {
    let mut compilation = setup_compilation();
    let source = r#"local function a()
  return b()
end

local function b()
  return a()
end
"#;
    set_test_file(
        &mut compilation,
        505,
        "C:/ws/semantic_graph_scc.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(505))
        .expect("signature summary")
        .signatures
        .clone();
    let a_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("a"))
        .expect("a signature");
    let b_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("b"))
        .expect("b signature");

    let a_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(505),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(a_signature.syntax_offset),
        )
        .expect("a scc component");
    let b_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(505),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(b_signature.syntax_offset),
        )
        .expect("b scc component");

    assert_eq!(a_component.component_id, b_component.component_id);
    assert!(a_component.is_cycle);
    assert_eq!(a_component.nodes.len(), 2);
    assert!(a_component.nodes.iter().any(|node| matches!(
        node,
        crate::SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset)
            if *signature_offset == a_signature.syntax_offset
    )));
    assert!(a_component.nodes.iter().any(|node| matches!(
        node,
        crate::SalsaSemanticGraphNodeSummary::SignatureReturn(signature_offset)
            if *signature_offset == b_signature.syntax_offset
    )));

    let scc_index = compilation
        .semantic()
        .file()
        .graph_scc_index(FileId::new(505))
        .expect("graph scc index");
    assert!(
        scc_index
            .components
            .iter()
            .any(|component| component.is_cycle && component.nodes.len() == 2)
    );
    assert_eq!(scc_index.topo_order.len(), scc_index.components.len());
}

#[test]
fn test_summary_builder_semantic_graph_scc_component_graph_queries() {
    let mut compilation = setup_compilation();
    let source = r#"local function leaf()
  return 1
end

local function mid()
  return leaf()
end

local function a()
  return b()
end

local function b()
  return a()
end

local result = mid()
"#;
    set_test_file(
        &mut compilation,
        506,
        "C:/ws/semantic_graph_scc_component_queries.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(506))
        .expect("signature summary")
        .signatures
        .clone();
    let leaf_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("leaf"))
        .expect("leaf signature");
    let mid_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("mid"))
        .expect("mid signature");
    let a_signature = signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("a"))
        .expect("a signature");

    let leaf_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(506),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(leaf_signature.syntax_offset),
        )
        .expect("leaf component");
    let mid_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(506),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(mid_signature.syntax_offset),
        )
        .expect("mid component");
    let cycle_component = compilation
        .semantic()
        .file()
        .graph_scc_component(
            FileId::new(506),
            crate::SalsaSemanticGraphNodeSummary::SignatureReturn(a_signature.syntax_offset),
        )
        .expect("cycle component");

    assert!(!leaf_component.is_cycle);
    assert!(!mid_component.is_cycle);
    assert!(cycle_component.is_cycle);
    assert!(
        mid_component
            .successor_component_ids
            .contains(&leaf_component.component_id)
    );
    assert!(
        leaf_component
            .predecessor_component_ids
            .contains(&mid_component.component_id)
    );

    let mid_successors = compilation
        .semantic()
        .file()
        .graph_scc_successors(FileId::new(506), mid_component.component_id)
        .expect("mid component successors");
    assert!(
        mid_successors
            .iter()
            .any(|component| component.component_id == leaf_component.component_id)
    );

    let leaf_predecessors = compilation
        .semantic()
        .file()
        .graph_scc_predecessors(FileId::new(506), leaf_component.component_id)
        .expect("leaf component predecessors");
    assert!(
        leaf_predecessors
            .iter()
            .any(|component| component.component_id == mid_component.component_id)
    );

    let fetched_cycle_component = compilation
        .semantic()
        .file()
        .graph_scc_component_by_id(FileId::new(506), cycle_component.component_id)
        .expect("cycle component by id");
    assert_eq!(
        fetched_cycle_component.component_id,
        cycle_component.component_id
    );

    let scc_index = compilation
        .semantic()
        .file()
        .graph_scc_index(FileId::new(506))
        .expect("graph scc index");
    let leaf_topo_position = scc_index
        .topo_order
        .iter()
        .position(|component_id| *component_id == leaf_component.component_id)
        .expect("leaf topo position");
    let mid_topo_position = scc_index
        .topo_order
        .iter()
        .position(|component_id| *component_id == mid_component.component_id)
        .expect("mid topo position");
    assert!(mid_topo_position < leaf_topo_position);
}
