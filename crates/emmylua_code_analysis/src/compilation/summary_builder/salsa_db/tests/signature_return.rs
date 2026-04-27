use super::*;

#[test]
fn test_summary_builder_signature_return_summary_resolves_name_return() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local value = pick()

---@return string
function test()
  return value
end"#;
    set_test_file(
        &mut compilation,
        301,
        "C:/ws/signature_return_name.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(301))
        .expect("signature summary");
    let signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("test"))
        .expect("test signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(301), signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(
        summary.state,
        crate::SalsaSignatureReturnResolveStateSummary::Resolved
    );
    assert_eq!(summary.doc_returns.len(), 1);
    assert_eq!(summary.values.len(), 1);
    assert!(matches!(
        summary.values[0].kind,
        crate::SalsaSignatureReturnExprKindSummary::Name
    ));
    assert!(
        summary.values[0]
            .name_type
            .as_ref()
            .is_some_and(|info| !info.candidates.is_empty())
    );
}

#[test]
fn test_summary_builder_signature_return_summary_resolves_member_return() {
    let mut compilation = setup_compilation();
    let source = r#"local obj = {
  value = 1,
}

local function test()
  return obj.value
end"#;
    set_test_file(
        &mut compilation,
        302,
        "C:/ws/signature_return_member.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(302))
        .expect("signature summary");
    let signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("test"))
        .expect("test signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(302), signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(
        summary.state,
        crate::SalsaSignatureReturnResolveStateSummary::Resolved
    );
    assert_eq!(summary.values.len(), 1);
    assert!(matches!(
        summary.values[0].kind,
        crate::SalsaSignatureReturnExprKindSummary::Member
    ));
    assert!(
        summary.values[0]
            .member_type
            .as_ref()
            .is_some_and(|info| !info.candidates.is_empty())
    );
}

#[test]
fn test_summary_builder_signature_return_summary_resolves_name_return_from_named_type_evidence() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local box = {}

local function test()
  return box
end"#;
    set_test_file(
        &mut compilation,
        311,
        "C:/ws/signature_return_name_named_type.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(311))
        .expect("signature summary");
    let signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("test"))
        .expect("test signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(311), signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(
        summary.state,
        crate::SalsaSignatureReturnResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.values[0].kind,
        crate::SalsaSignatureReturnExprKindSummary::Name
    ));
    assert!(summary.values[0].name_type.as_ref().is_some_and(|info| {
        info.candidates
            .iter()
            .any(|candidate| !candidate.named_type_names.is_empty())
    }));
}

#[test]
fn test_summary_builder_signature_return_summary_keeps_call_backed_name_return_partial() {
    let mut compilation = setup_compilation();
    let source = r#"local value = maybe()

local function test()
  return value
end"#;
    set_test_file(
        &mut compilation,
        312,
        "C:/ws/signature_return_name_partial.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(312))
        .expect("signature summary");
    let signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("test"))
        .expect("test signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(312), signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(
        summary.state,
        crate::SalsaSignatureReturnResolveStateSummary::Partial
    );
    assert!(matches!(
        summary.values[0].kind,
        crate::SalsaSignatureReturnExprKindSummary::Name
    ));
    assert!(summary.values[0].name_type.as_ref().is_some_and(|info| {
        info.candidates
            .iter()
            .any(|candidate| candidate.initializer_offset.is_some())
    }));
}

#[test]
fn test_summary_builder_signature_return_summary_marks_recursive_dependency() {
    let mut compilation = setup_compilation();
    let source = r#"local function test()
  return test()
end"#;
    set_test_file(
        &mut compilation,
        303,
        "C:/ws/signature_return_recursive.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(303))
        .expect("signature summary");
    let signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("test"))
        .expect("test signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(303), signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(
        summary.state,
        crate::SalsaSignatureReturnResolveStateSummary::RecursiveDependency
    );
    assert_eq!(summary.values.len(), 1);
    assert!(
        summary.values[0]
            .call
            .as_ref()
            .is_some_and(|call| call.resolved_signature_offset == Some(signature.syntax_offset))
    );
}

#[test]
fn test_summary_builder_signature_return_summary_resolves_literal_table_and_closure_returns() {
    let mut compilation = setup_compilation();
    let source = r#"local function ret_literal()
  return 1
end

local function ret_table()
  return { value = 1 }
end

local function ret_closure()
  return function()
    return 1
  end
end"#;
    set_test_file(
        &mut compilation,
        304,
        "C:/ws/signature_return_stable_shapes.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(304))
        .expect("signature summary");

    for (name, expected_kind) in [
        (
            "ret_literal",
            crate::SalsaSignatureReturnExprKindSummary::Literal,
        ),
        (
            "ret_table",
            crate::SalsaSignatureReturnExprKindSummary::Table,
        ),
        (
            "ret_closure",
            crate::SalsaSignatureReturnExprKindSummary::Closure,
        ),
    ] {
        let signature = signatures
            .signatures
            .iter()
            .find(|signature| signature.name.as_deref() == Some(name))
            .expect("signature");

        let summary = compilation
            .semantic()
            .file()
            .signature_return_summary(FileId::new(304), signature.syntax_offset)
            .expect("signature return summary");

        assert_eq!(
            summary.state,
            crate::SalsaSignatureReturnResolveStateSummary::Resolved,
            "expected {name} to be treated as stable resolved return",
        );
        assert_eq!(summary.values.len(), 1);
        assert_eq!(summary.values[0].kind, expected_kind);
    }
}

#[test]
fn test_summary_builder_signature_return_summary_tracks_doc_return_offsets_per_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return string first, integer second
local function pair()
  local a = "x"
  local b = 1
  return a, b
end"#;
    set_test_file(
        &mut compilation,
        305,
        "C:/ws/signature_return_slots.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(305))
        .expect("signature summary");
    let signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(305), signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(summary.values.len(), 2);
    assert!(!summary.values[0].doc_return_type_offsets.is_empty());
    assert!(!summary.values[1].doc_return_type_offsets.is_empty());
    assert_ne!(
        summary.values[0].doc_return_type_offsets,
        summary.values[1].doc_return_type_offsets
    );
}

#[test]
fn test_summary_builder_signature_return_summary_resolves_call_via_return_overload_rows() {
    let mut compilation = setup_compilation();
    let source = r#"---@return_overload true, integer
---@return_overload false, string
local function parse()
  return true, 1
end

local function forward()
  return parse()
end"#;
    set_test_file(
        &mut compilation,
        306,
        "C:/ws/signature_return_overload_call.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(306))
        .expect("signature summary");
    let signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("forward"))
        .expect("forward signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(306), signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(
        summary.state,
        crate::SalsaSignatureReturnResolveStateSummary::Resolved
    );
    assert_eq!(summary.values.len(), 1);
    assert!(matches!(
        summary.values[0].kind,
        crate::SalsaSignatureReturnExprKindSummary::Call
    ));
    assert!(summary.values[0].call.as_ref().is_some_and(
        |call| !call.overload_returns.is_empty() && call.returns == call.overload_returns
    ));
}

#[test]
fn test_summary_builder_signature_return_query_state_uses_solver_fixedpoint() {
    let mut compilation = setup_compilation();
    let source = r#"local function pair()
  return 1, missing
end

local first = pair()

local function forward()
  return first
end"#;
    set_test_file(
        &mut compilation,
        307,
        "C:/ws/signature_return_solver_state.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(307))
        .expect("signature summary");
    let forward_signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("forward"))
        .expect("forward signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(307), forward_signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(
        summary.state,
        crate::SalsaSignatureReturnResolveStateSummary::Resolved
    );
    assert_eq!(summary.values.len(), 1);
    assert!(matches!(
        summary.values[0].kind,
        crate::SalsaSignatureReturnExprKindSummary::Name
    ));
}

#[test]
fn test_summary_builder_signature_return_summary_backfills_single_slot_type_offsets_from_shell() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Holder
---@field value string

---@type Holder
local holder

local function forward()
    return holder.value
end"#;
    set_test_file(
        &mut compilation,
        308,
        "C:/ws/signature_return_solver_detail.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(308))
        .expect("signature summary");
    let forward_signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("forward"))
        .expect("forward signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(308), forward_signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(summary.values.len(), 1);
    assert!(matches!(
        summary.values[0].kind,
        crate::SalsaSignatureReturnExprKindSummary::Member
    ));
    assert!(!summary.values[0].doc_return_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_signature_return_summary_backfills_multi_slot_type_offsets_from_shell() {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder

local function pair()
  return holder.first, holder.second
end"#;
    set_test_file(
        &mut compilation,
        309,
        "C:/ws/signature_return_solver_multi_slot.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(309))
        .expect("signature summary");
    let pair_signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");

    let summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(309), pair_signature.syntax_offset)
        .expect("signature return summary");

    assert_eq!(summary.values.len(), 2);
    assert!(summary.values.iter().all(|value| matches!(
        value.kind,
        crate::SalsaSignatureReturnExprKindSummary::Member
    )));
    assert!(!summary.values[0].doc_return_type_offsets.is_empty());
    assert!(!summary.values[1].doc_return_type_offsets.is_empty());
    assert_ne!(
        summary.values[0].doc_return_type_offsets,
        summary.values[1].doc_return_type_offsets
    );
}

#[test]
fn test_summary_builder_signature_return_summary_aligns_with_query_and_component_result() {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder

local function pair()
  return holder.first, holder.second
end"#;
    set_test_file(
        &mut compilation,
        310,
        "C:/ws/signature_return_semantic_summary.lua",
        source,
    );

    let signatures = compilation
        .doc()
        .signatures(FileId::new(310))
        .expect("signature summary");
    let pair_signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");

    let semantic_summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(310), pair_signature.syntax_offset)
        .expect("semantic signature return summary");
    let query = compilation
        .doc()
        .signature_return(FileId::new(310), pair_signature.syntax_offset)
        .expect("signature return query");
    let component_result = compilation
        .semantic()
        .file()
        .signature_return_component_result_summary(FileId::new(310), pair_signature.syntax_offset)
        .expect("signature return component result");

    assert_eq!(
        semantic_summary.signature_offset,
        pair_signature.syntax_offset
    );
    assert_eq!(semantic_summary.state, query.state);
    assert_eq!(semantic_summary.doc_returns, query.doc_returns);
    assert_eq!(semantic_summary.values, query.values);
    assert_eq!(
        semantic_summary.propagated_value_shell,
        component_result.propagated_value_shell
    );
    assert_eq!(
        semantic_summary.local_value_shell,
        component_result.local_value_shell
    );
    assert_eq!(
        semantic_summary.fixedpoint_value_shell,
        component_result.fixedpoint_value_shell
    );
    assert_eq!(
        semantic_summary.value_shell,
        component_result.fixedpoint_value_shell
    );
    assert_eq!(semantic_summary.values.len(), 2);
    assert!(
        !semantic_summary.values[0]
            .doc_return_type_offsets
            .is_empty()
    );
    assert!(
        !semantic_summary.values[1]
            .doc_return_type_offsets
            .is_empty()
    );
}
