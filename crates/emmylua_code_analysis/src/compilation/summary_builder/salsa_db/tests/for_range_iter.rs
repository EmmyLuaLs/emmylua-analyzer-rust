use super::*;

fn first_for_range_loop_offset(compilation: &SalsaSummaryHost, file_id: FileId) -> rowan::TextSize {
    compilation
        .flow()
        .summary(file_id)
        .expect("flow summary")
        .loops
        .iter()
        .find(|loop_summary| matches!(loop_summary.kind, crate::SalsaFlowLoopKindSummary::ForRange))
        .map(|loop_summary| loop_summary.syntax_offset)
        .expect("for range loop")
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_resolves_name_source() {
    let mut compilation = setup_compilation();
    let source = r#"---@type fun(): string, integer
local iter

for key, value in iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        401,
        "C:/ws/for_range_iter_name.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(401));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(401), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Name,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert_eq!(summary.iter_vars[0].name.as_str(), "key");
    assert_eq!(summary.iter_vars[1].name.as_str(), "value");
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_resolves_member_source() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Holder
---@field iter fun(): string, integer

---@type Holder
local holder

for key, value in holder.iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        402,
        "C:/ws/for_range_iter_member.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(402));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(402), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Member,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_marks_recursive_call_dependency() {
    let mut compilation = setup_compilation();
    let source = r#"local function factory()
  return factory()
end

for value in factory() do
  print(value)
end"#;
    set_test_file(
        &mut compilation,
        403,
        "C:/ws/for_range_iter_recursive.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(403));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(403), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::RecursiveDependency
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Call,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 1);
    assert!(summary.iter_vars[0].type_offsets.is_empty());
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_backfills_call_source_factory_slot_offsets()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder = {}

local function iter()
  return holder.first, holder.second
end

local function make_iter()
  return iter
end

for key, value in make_iter() do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        410,
        "C:/ws/for_range_iter_call_source_factory_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(410));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(410), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Call,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
    assert_ne!(
        summary.iter_vars[0].type_offsets,
        summary.iter_vars[1].type_offsets
    );
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_backfills_name_source_factory_call_slot_offsets()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder = {}

local function iter()
  return holder.first, holder.second
end

local function make_iter()
  return iter
end

local iter_alias = make_iter()

for key, value in iter_alias do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        411,
        "C:/ws/for_range_iter_name_source_factory_call_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(411));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(411), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Name,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
    assert_ne!(
        summary.iter_vars[0].type_offsets,
        summary.iter_vars[1].type_offsets
    );
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_backfills_member_source_factory_call_slot_offsets()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer
---@field iter fun(): string, integer

---@type PairHolder
local holder = {}

local function iter()
  return holder.first, holder.second
end

local function make_iter()
  return iter
end

holder.iter = make_iter()

for key, value in holder.iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        415,
        "C:/ws/for_range_iter_member_source_factory_call_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(415));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(415), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Member,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
    assert_ne!(
        summary.iter_vars[0].type_offsets,
        summary.iter_vars[1].type_offsets
    );
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_state_uses_solver_fixedpoint() {
    let mut compilation = setup_compilation();
    let source = r#"---@return fun(): string
local function make()
    return function()
        return "x"
    end
end

local iter = make()

for key in iter do
  print(key)
end"#;
    set_test_file(
        &mut compilation,
        404,
        "C:/ws/for_range_iter_solver_state.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(404));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(404), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Name,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 1);
}

#[test]
fn test_summary_builder_for_range_iter_value_shell_tracks_source_domain() {
    let mut compilation = setup_compilation();
    let source = r#"---@type fun(): string, integer
local iter

for key, value in iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        405,
        "C:/ws/for_range_iter_source_domain.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(405));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(405), loop_offset)
        .expect("for range iter component summary");
    let value_shell = compilation
        .semantic()
        .file()
        .for_range_iter_value_shell(FileId::new(405), loop_offset)
        .expect("for range value shell");

    assert_eq!(
        value_shell.state,
        crate::SalsaSemanticResolveStateSummary::Resolved
    );
    assert!(!value_shell.candidate_type_offsets.is_empty());
    assert_eq!(summary.iter_vars.len(), 2);
    assert_ne!(
        value_shell.candidate_type_offsets,
        summary.iter_vars[0].type_offsets
    );
    assert_ne!(
        value_shell.candidate_type_offsets,
        summary.iter_vars[1].type_offsets
    );
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_backfills_member_slot_offsets_from_signature_return_values()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder = {}

function holder.iter()
  return holder.first, holder.second
end

for key, value in holder.iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        406,
        "C:/ws/for_range_iter_member_signature_return_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(406));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(406), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Member,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
    assert_ne!(
        summary.iter_vars[0].type_offsets,
        summary.iter_vars[1].type_offsets
    );
}

#[test]
fn test_summary_builder_for_range_iter_query_aligns_with_semantic_component_summary() {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder = {}

function holder.iter()
  return holder.first, holder.second
end

for key, value in holder.iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        414,
        "C:/ws/for_range_iter_query_aligns_with_semantic_component_summary.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(414));
    let query = compilation
        .flow()
        .for_range_iter(FileId::new(414), loop_offset)
        .expect("for range iter query");
    let semantic_summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(414), loop_offset)
        .expect("semantic for range iter component summary");
    let component_result = compilation
        .semantic()
        .file()
        .for_range_iter_component_result_summary(FileId::new(414), loop_offset)
        .expect("for range iter component result");

    assert_eq!(query.loop_offset, semantic_summary.loop_offset);
    assert_eq!(query.iter_expr_offsets, semantic_summary.iter_expr_offsets);
    assert_eq!(query.state, semantic_summary.state);
    assert_eq!(query.source, semantic_summary.source);
    assert_eq!(query.iter_vars, semantic_summary.iter_vars);
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
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_backfills_name_alias_slot_offsets_from_signature_return_values()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder = {}

local function pair()
  return holder.first, holder.second
end

local iter = pair

for key, value in iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        407,
        "C:/ws/for_range_iter_name_alias_signature_return_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(407));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(407), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Name,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
    assert_ne!(
        summary.iter_vars[0].type_offsets,
        summary.iter_vars[1].type_offsets
    );
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_backfills_multi_hop_name_alias_slot_offsets()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class PairHolder
---@field first string
---@field second integer

---@type PairHolder
local holder = {}

local function pair()
  return holder.first, holder.second
end

local iter0 = pair
local iter = iter0

for key, value in iter do
  print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        408,
        "C:/ws/for_range_iter_multi_hop_name_alias_signature_return_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(408));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(408), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
    assert_ne!(
        summary.iter_vars[0].type_offsets,
        summary.iter_vars[1].type_offsets
    );
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_merges_branch_alias_slot_offsets() {
    let mut compilation = setup_compilation();
    let source = r#"---@class LeftPair
---@field first string
---@field second integer

---@class RightPair
---@field first boolean
---@field second number

---@type LeftPair
local left_holder = {}
---@type RightPair
local right_holder = {}

local function left_pair()
    return left_holder.first, left_holder.second
end

local function right_pair()
    return right_holder.first, right_holder.second
end

local iter = left_pair
if cond then
    iter = left_pair
else
    iter = right_pair
end

for key, value in iter do
    print(key, value)
end"#;
    set_test_file(
        &mut compilation,
        409,
        "C:/ws/for_range_iter_branch_alias_signature_return_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(409));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(409), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Name,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
}

#[test]
fn test_summary_builder_for_range_iter_component_summary_merges_mixed_branch_alias_and_factory_candidates()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class LeftPair
    ---@field first string
    ---@field second integer

    ---@class RightPair
    ---@field first boolean
    ---@field second number

    ---@type LeftPair
    local left_holder = {}
    ---@type RightPair
    local right_holder = {}

    local function left_pair()
        return left_holder.first, left_holder.second
    end

    local function right_pair()
        return right_holder.first, right_holder.second
    end

    local function make_right_iter()
        return right_pair
    end

    local iter = left_pair
    if cond then
        iter = left_pair
    else
        iter = make_right_iter()
    end

    for key, value in iter do
        print(key, value)
    end"#;
    set_test_file(
        &mut compilation,
        413,
        "C:/ws/for_range_iter_mixed_branch_alias_and_factory_slot_backfill.lua",
        source,
    );

    let loop_offset = first_for_range_loop_offset(&compilation, FileId::new(413));
    let summary = compilation
        .semantic()
        .file()
        .for_range_iter_component_summary(FileId::new(413), loop_offset)
        .expect("for range iter component summary");

    assert_eq!(
        summary.state,
        crate::SalsaForRangeIterResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.source,
        Some(crate::SalsaForRangeIterSourceSummary {
            kind: crate::SalsaForRangeIterSourceKindSummary::Name,
            ..
        })
    ));
    assert_eq!(summary.iter_vars.len(), 2);
    assert!(!summary.iter_vars[0].type_offsets.is_empty());
    assert!(!summary.iter_vars[1].type_offsets.is_empty());
    assert_ne!(
        summary.iter_vars[0].type_offsets,
        summary.iter_vars[1].type_offsets
    );
}
