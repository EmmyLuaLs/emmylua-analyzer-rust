use super::*;

#[test]
fn test_summary_builder_type_query_program_point_uses_latest_local_assignment() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local value = init()
value = replace()
print(value)"#;
    set_test_file(&mut compilation, 35, "C:/ws/type_assignment.lua", source);

    let name_offset = source.rfind("value").expect("name use offset") as u32;
    let assignment_offset = source.find("value = replace()").expect("assignment offset") as u32;
    let replace_offset = source.find("replace()").expect("replace offset") as u32;
    let info = compilation
        .types()
        .name_at(
            FileId::new(35),
            TextSize::from(name_offset),
            TextSize::from(name_offset),
        )
        .expect("program point type info");

    assert_eq!(info.candidates.len(), 1);
    assert_eq!(
        info.candidates[0].origin,
        crate::SalsaTypeCandidateOriginSummary::Assignment(TextSize::from(assignment_offset))
    );
    assert_eq!(
        info.candidates[0].initializer_offset,
        Some(TextSize::from(replace_offset))
    );
}

#[test]
fn test_summary_builder_type_query_program_point_applies_truthy_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string|nil
local value = nil
if value then
  print(value)
end"#;
    set_test_file(&mut compilation, 36, "C:/ws/type_truthy_narrow.lua", source);

    let truthy_offset = source.find("value)\nend").expect("truthy use offset") as u32;
    let truthy_info = compilation
        .types()
        .name_at(
            FileId::new(36),
            TextSize::from(truthy_offset),
            TextSize::from(truthy_offset),
        )
        .expect("truthy program point info");

    assert!(
        truthy_info
            .active_narrows
            .contains(&crate::SalsaTypeNarrowSummary::Truthy)
    );
    assert_eq!(truthy_info.candidates.len(), 1);
    assert!(!truthy_info.candidates[0].explicit_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_applies_assert_type_guard_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string|number
local guarded = pick()
assert(type(guarded) == "string")
print(guarded)"#;
    set_test_file(&mut compilation, 37, "C:/ws/type_guard_narrow.lua", source);

    let guarded_offset = source.rfind("guarded").expect("guarded use offset") as u32;
    let guarded_info = compilation
        .types()
        .name_at(
            FileId::new(37),
            TextSize::from(guarded_offset),
            TextSize::from(guarded_offset),
        )
        .expect("guarded program point info");

    assert!(guarded_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::TypeGuard { type_name } if type_name == "string"
    )));
    assert_eq!(guarded_info.candidates.len(), 1);
    assert!(!guarded_info.candidates[0].explicit_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_applies_composite_and_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string|nil
local guarded = pick()
if guarded and type(guarded) == "string" then
  print(guarded)
end"#;
    set_test_file(
        &mut compilation,
        137,
        "C:/ws/type_composite_and_narrow.lua",
        source,
    );

    let guarded_offset = source
        .rfind("guarded)\nend")
        .expect("composite guarded offset") as u32;
    let guarded_info = compilation
        .types()
        .name_at(
            FileId::new(137),
            TextSize::from(guarded_offset),
            TextSize::from(guarded_offset),
        )
        .expect("composite and narrow info");

    assert!(
        guarded_info
            .active_narrows
            .contains(&crate::SalsaTypeNarrowSummary::Truthy)
    );
    assert!(guarded_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::TypeGuard { type_name } if type_name == "string"
    )));
    assert_eq!(guarded_info.candidates.len(), 1);
    assert!(!guarded_info.candidates[0].explicit_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_applies_not_type_guard_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string|number
local guarded = pick()
if not (type(guarded) == "string") then
  print(guarded)
end"#;
    set_test_file(
        &mut compilation,
        138,
        "C:/ws/type_not_type_guard_narrow.lua",
        source,
    );

    let guarded_offset = source.rfind("guarded)\nend").expect("not guarded offset") as u32;
    let guarded_info = compilation
        .types()
        .name_at(
            FileId::new(138),
            TextSize::from(guarded_offset),
            TextSize::from(guarded_offset),
        )
        .expect("not type guard narrow info");

    assert!(guarded_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::ExcludeTypeGuard { type_name } if type_name == "string"
    )));
    assert_eq!(guarded_info.candidates.len(), 1);
    assert!(!guarded_info.candidates[0].explicit_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_applies_not_field_literal_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Foo
---@field kind "foo"
---@class Bar
---@field kind "bar"
---@type Foo|Bar
local value = make_value()
if not (value.kind == "foo") then
  print(value)
end"#;
    set_test_file(
        &mut compilation,
        139,
        "C:/ws/type_not_field_narrow.lua",
        source,
    );

    let value_offset = source.rfind("value)\nend").expect("not field value offset") as u32;
    let value_info = compilation
        .types()
        .name_at(
            FileId::new(139),
            TextSize::from(value_offset),
            TextSize::from(value_offset),
        )
        .expect("not field literal narrow info");

    assert!(value_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::FieldLiteral { member_name, literal_text, positive }
            if member_name == "kind" && literal_text == "\"foo\"" && !positive
    )));
    assert_eq!(value_info.candidates.len(), 1);
    assert_eq!(value_info.candidates[0].explicit_type_offsets.len(), 1);
}

#[test]
fn test_summary_builder_type_query_program_point_applies_falsey_else_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string|nil
local value = maybe()
if value then
  print("ok")
else
  print(value)
end"#;
    set_test_file(&mut compilation, 38, "C:/ws/type_falsey_narrow.lua", source);

    let value_offset = source.rfind("value").expect("else value offset") as u32;
    let value_info = compilation
        .types()
        .name_at(
            FileId::new(38),
            TextSize::from(value_offset),
            TextSize::from(value_offset),
        )
        .expect("falsey else program point info");

    assert!(
        value_info
            .active_narrows
            .contains(&crate::SalsaTypeNarrowSummary::Falsey)
    );
    assert_eq!(value_info.candidates.len(), 1);
    assert!(!value_info.candidates[0].explicit_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_applies_false_branch_type_guard() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string|number
local guarded = pick()
if type(guarded) == "string" then
  print("ok")
else
  print(guarded)
end"#;
    set_test_file(
        &mut compilation,
        39,
        "C:/ws/type_guard_else_narrow.lua",
        source,
    );

    let guarded_offset = source.rfind("guarded").expect("else guarded offset") as u32;
    let guarded_info = compilation
        .types()
        .name_at(
            FileId::new(39),
            TextSize::from(guarded_offset),
            TextSize::from(guarded_offset),
        )
        .expect("false branch type guard info");

    assert!(guarded_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::ExcludeTypeGuard { type_name } if type_name == "string"
    )));
    assert_eq!(guarded_info.candidates.len(), 1);
    assert!(!guarded_info.candidates[0].explicit_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_merges_branch_assignments() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local left = init_left()
---@type number
local right = init_right()
local value = left
if cond then
  value = left
else
  value = right
end
print(value)"#;
    set_test_file(
        &mut compilation,
        40,
        "C:/ws/type_merge_assignments.lua",
        source,
    );

    let value_offset = source.rfind("value").expect("merged value offset") as u32;
    let left_assignment_offset = source
        .find("value = left\nelse")
        .expect("left branch assignment") as u32;
    let right_assignment_offset = source
        .find("value = right")
        .expect("right branch assignment") as u32;
    let value_info = compilation
        .types()
        .name_at(
            FileId::new(40),
            TextSize::from(value_offset),
            TextSize::from(value_offset),
        )
        .expect("merged branch assignment info");

    assert_eq!(value_info.candidates.len(), 2);
    assert!(value_info.candidates.iter().any(|candidate| {
        candidate.origin
            == crate::SalsaTypeCandidateOriginSummary::Assignment(TextSize::from(
                left_assignment_offset,
            ))
    }));
    assert!(value_info.candidates.iter().any(|candidate| {
        candidate.origin
            == crate::SalsaTypeCandidateOriginSummary::Assignment(TextSize::from(
                right_assignment_offset,
            ))
    }));
}

#[test]
fn test_summary_builder_type_query_program_point_keeps_signature_backed_docless_factory_call_candidate()
 {
    let mut compilation = setup_compilation();
    let source = r#"local function iter()
  return "a", 1
end

local function make_iter()
  return iter
end

local iter_alias = make_iter()
print(iter_alias)"#;
    set_test_file(
        &mut compilation,
        412,
        "C:/ws/type_program_point_docless_factory_call.lua",
        source,
    );

    let name_offset = source.rfind("iter_alias)").expect("iter_alias use offset") as u32;
    let info = compilation
        .types()
        .name_at(
            FileId::new(412),
            TextSize::from(name_offset),
            TextSize::from(name_offset),
        )
        .expect("program point type info");

    assert!(
        info.candidates
            .iter()
            .any(|candidate| candidate.signature_offset.is_some())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_keeps_pre_if_assignment_without_else() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local left = init_left()
---@type number
local right = init_right()
local value = left
value = left
if cond then
  value = right
end
print(value)"#;
    set_test_file(
        &mut compilation,
        41,
        "C:/ws/type_optional_merge_assignment.lua",
        source,
    );

    let value_offset = source.rfind("value").expect("optional merged value offset") as u32;
    let initial_assignment_offset = source
        .rfind("value = left\nif cond")
        .expect("initial assignment") as u32;
    let branch_assignment_offset = source.find("value = right").expect("branch assignment") as u32;
    let value_info = compilation
        .types()
        .name_at(
            FileId::new(41),
            TextSize::from(value_offset),
            TextSize::from(value_offset),
        )
        .expect("optional merge assignment info");

    assert_eq!(value_info.candidates.len(), 2);
    assert!(value_info.candidates.iter().any(|candidate| {
        candidate.origin
            == crate::SalsaTypeCandidateOriginSummary::Assignment(TextSize::from(
                initial_assignment_offset,
            ))
    }));
    assert!(value_info.candidates.iter().any(|candidate| {
        candidate.origin
            == crate::SalsaTypeCandidateOriginSummary::Assignment(TextSize::from(
                branch_assignment_offset,
            ))
    }));
}

#[test]
fn test_summary_builder_type_query_program_point_keeps_outer_current_inside_nested_active_block() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local left = init_left()
---@type number
local right = init_right()
local value = left
if cond then
  value = right
  if branch then
    print(value)
  end
end"#;
    set_test_file(
        &mut compilation,
        147,
        "C:/ws/type_nested_active_block_keeps_outer_current.lua",
        source,
    );

    let value_offset = source.rfind("value)\n  end").expect("nested value offset") as u32;
    let branch_assignment_offset = source
        .find("value = right")
        .expect("branch assignment offset") as u32;
    let value_info = compilation
        .types()
        .name_at(
            FileId::new(147),
            TextSize::from(value_offset),
            TextSize::from(value_offset),
        )
        .expect("nested active block value info");

    assert_eq!(value_info.candidates.len(), 1);
    assert_eq!(
        value_info.candidates[0].origin,
        crate::SalsaTypeCandidateOriginSummary::Assignment(TextSize::from(
            branch_assignment_offset
        ))
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_owner_type() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
---@field value integer
---@type Box
local box = make_box()
if box then
  print(box.value)
end"#;
    set_test_file(
        &mut compilation,
        42,
        "C:/ws/type_member_program_point.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(42),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("member program point info");

    assert!(
        member_info
            .active_narrows
            .contains(&crate::SalsaTypeNarrowSummary::Truthy)
    );
    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(!member_info.candidates.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_alias_owner_type() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
---@field value integer
---@alias BoxAlias Box
---@type BoxAlias
local box = make_box()
print(box.value)"#;
    set_test_file(
        &mut compilation,
        143,
        "C:/ws/type_member_alias_owner.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(143),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("alias member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(!member_info.candidates.is_empty());
    assert!(
        member_info
            .candidates
            .iter()
            .any(|candidate| !candidate.explicit_type_offsets.is_empty())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_inline_object_alias_owner_type()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@alias InlineBox { value: integer }
---@type InlineBox
local box = make_box()
print(box.value)"#;
    set_test_file(
        &mut compilation,
        144,
        "C:/ws/type_member_inline_object_alias_owner.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(144),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("inline object alias member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(!member_info.candidates.is_empty());
    assert!(
        member_info
            .candidates
            .iter()
            .any(|candidate| !candidate.explicit_type_offsets.is_empty())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_inline_table_initializer_owner()
 {
    let mut compilation = setup_compilation();
    let source = r#"local box = { value = 1 }
print(box.value)"#;
    set_test_file(
        &mut compilation,
        148,
        "C:/ws/type_member_inline_table_initializer_owner.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let field_value_offset = source.find("1 }").expect("field literal offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(148),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("inline table initializer member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(member_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets.is_empty()
            && candidate.initializer_offset == Some(TextSize::from(field_value_offset))
    }));
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_tuple_index_from_inline_table_initializer_owner()
 {
    let mut compilation = setup_compilation();
    let source = r#"local pair = { 1, "two" }
print(pair[2])"#;
    set_test_file(
        &mut compilation,
        156,
        "C:/ws/type_member_inline_table_tuple_index.lua",
        source,
    );

    let member_offset = source.rfind("pair[2]").expect("tuple member use offset") as u32;
    let slot_offset = source.find("\"two\"").expect("tuple slot offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(156),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("inline table tuple index member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert_eq!(member_info.candidates.len(), 1);
    assert_eq!(
        member_info.candidates[0].initializer_offset,
        Some(TextSize::from(slot_offset))
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_tuple_index_from_tail_call_inline_table_owner()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

local tuple = { pair() }
print(tuple[2])"#;
    set_test_file(
        &mut compilation,
        163,
        "C:/ws/type_member_inline_table_tail_call_tuple_index.lua",
        source,
    );

    let member_offset = source.rfind("tuple[2]").expect("tuple member use offset") as u32;
    let call_offset = source.find("pair() }").expect("tail call slot offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(163),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("tail call inline table tuple member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert_eq!(member_info.candidates.len(), 1);
    assert_eq!(
        member_info.candidates[0].initializer_offset,
        Some(TextSize::from(call_offset))
    );
    assert_eq!(member_info.candidates[0].value_result_index, 1);
    assert!(member_info.candidates[0].source_call_syntax_id.is_some());
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_array_index_from_inline_table_initializer_owner()
 {
    let mut compilation = setup_compilation();
    let source = r#"local list = { 1, 2, 3 }
print(list[1])"#;
    set_test_file(
        &mut compilation,
        157,
        "C:/ws/type_member_inline_table_array_index.lua",
        source,
    );

    let member_offset = source.rfind("list[1]").expect("array member use offset") as u32;
    let first_offset = source.find("1, 2").expect("first array slot offset") as u32;
    let second_offset = source.find("2, 3").expect("second array slot offset") as u32;
    let third_offset = source.find("3 }").expect("third array slot offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(157),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("inline table array index member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert_eq!(member_info.candidates.len(), 3);
    assert!(
        member_info.candidates.iter().any(|candidate| {
            candidate.initializer_offset == Some(TextSize::from(first_offset))
        })
    );
    assert!(
        member_info.candidates.iter().any(|candidate| {
            candidate.initializer_offset == Some(TextSize::from(second_offset))
        })
    );
    assert!(
        member_info.candidates.iter().any(|candidate| {
            candidate.initializer_offset == Some(TextSize::from(third_offset))
        })
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_tuple_index_from_named_type_owner() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Pair
local Pair = { 1, "two" }

---@type Pair
local pair = make_pair()
print(pair[2])"#;
    set_test_file(
        &mut compilation,
        158,
        "C:/ws/type_member_named_type_tuple_index.lua",
        source,
    );

    let member_offset = source
        .rfind("pair[2]")
        .expect("named tuple member use offset") as u32;
    let slot_offset = source.find("\"two\"").expect("named tuple slot offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(158),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("named type tuple index member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert_eq!(member_info.candidates.len(), 1);
    assert_eq!(
        member_info.candidates[0].initializer_offset,
        Some(TextSize::from(slot_offset))
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_tuple_index_from_tail_call_named_type_owner()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

---@class Pair
local Pair = { pair() }

---@type Pair
local tuple = make_pair()
print(tuple[2])"#;
    set_test_file(
        &mut compilation,
        164,
        "C:/ws/type_member_named_type_tail_call_tuple_index.lua",
        source,
    );

    let member_offset = source
        .rfind("tuple[2]")
        .expect("named tuple member use offset") as u32;
    let call_offset = source
        .find("pair() }")
        .expect("named tail call slot offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(164),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("named type tail call tuple member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert_eq!(member_info.candidates.len(), 1);
    assert_eq!(
        member_info.candidates[0].initializer_offset,
        Some(TextSize::from(call_offset))
    );
    assert_eq!(member_info.candidates[0].value_result_index, 1);
    assert!(member_info.candidates[0].source_call_syntax_id.is_some());
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_mixed_numeric_keys_precisely() {
    let mut compilation = setup_compilation();
    let source = r#"---@class MixedBag
local MixedBag = { [1] = "head", "tail", [2] = "override" }

---@type MixedBag
local bag = make_bag()
print(bag[1])
print(bag[2])"#;
    set_test_file(
        &mut compilation,
        159,
        "C:/ws/type_member_named_type_mixed_numeric.lua",
        source,
    );

    let first_member_offset = source
        .find("bag[1]")
        .expect("first mixed member use offset") as u32;
    let second_member_offset = source
        .rfind("bag[2]")
        .expect("second mixed member use offset") as u32;
    let head_offset = source.find("\"head\"").expect("head slot offset") as u32;
    let tail_offset = source.find("\"tail\"").expect("tail slot offset") as u32;
    let override_offset = source.find("\"override\"").expect("override slot offset") as u32;

    let first_member_info = compilation
        .types()
        .member_at(
            FileId::new(159),
            TextSize::from(first_member_offset),
            TextSize::from(first_member_offset),
        )
        .expect("mixed numeric [1] member program point info");
    let second_member_info = compilation
        .types()
        .member_at(
            FileId::new(159),
            TextSize::from(second_member_offset),
            TextSize::from(second_member_offset),
        )
        .expect("mixed numeric [2] member program point info");

    assert_eq!(first_member_info.owner_candidates.len(), 1);
    assert_eq!(first_member_info.candidates.len(), 2);
    assert!(
        first_member_info
            .candidates
            .iter()
            .any(|candidate| { candidate.initializer_offset == Some(TextSize::from(head_offset)) })
    );
    assert!(
        first_member_info
            .candidates
            .iter()
            .any(|candidate| { candidate.initializer_offset == Some(TextSize::from(tail_offset)) })
    );

    assert_eq!(second_member_info.owner_candidates.len(), 1);
    assert_eq!(second_member_info.candidates.len(), 1);
    assert_eq!(
        second_member_info.candidates[0].initializer_offset,
        Some(TextSize::from(override_offset))
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_index_access_owner_type() {
    let mut compilation = setup_compilation();
    let source = r#"---@alias Nested { box: { value: integer } }
---@type Nested["box"]
local box = make_box()
print(box.value)"#;
    set_test_file(
        &mut compilation,
        151,
        "C:/ws/type_member_index_access_owner.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(151),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("index access owner member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(!member_info.candidates.is_empty());
    assert!(
        member_info
            .candidates
            .iter()
            .any(|candidate| !candidate.explicit_type_offsets.is_empty())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_mapped_owner_type() {
    let mut compilation = setup_compilation();
    let source = r#"---@alias Pick<T, K extends keyof T> { [P in K]: T[P]; }
---@type Pick<{ value: integer, name: string }, "value">
local box = make_box()
print(box.value)"#;
    set_test_file(
        &mut compilation,
        152,
        "C:/ws/type_member_mapped_owner.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(152),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("mapped owner member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(!member_info.candidates.is_empty());
    assert!(
        member_info
            .candidates
            .iter()
            .any(|candidate| !candidate.explicit_type_offsets.is_empty())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_does_not_over_bridge_member_from_mapped_owner_type()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@alias Pick<T, K extends keyof T> { [P in K]: T[P]; }
---@type Pick<{ value: integer, name: string }, "value">
local box = make_box()
print(box.name)"#;
    set_test_file(
        &mut compilation,
        165,
        "C:/ws/type_member_mapped_owner_no_overbridge.lua",
        source,
    );

    let member_offset = source.rfind("box.name").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(165),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("mapped owner non-member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(member_info.candidates.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_index_access_alias_key_owner_type()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@alias BoxKey "box"
---@alias Nested { box: { value: integer } }
---@type Nested[BoxKey]
local box = make_box()
print(box.value)"#;
    set_test_file(
        &mut compilation,
        153,
        "C:/ws/type_member_index_access_alias_key_owner.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(153),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("index access alias key owner member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(!member_info.candidates.is_empty());
    assert!(
        member_info
            .candidates
            .iter()
            .any(|candidate| !candidate.explicit_type_offsets.is_empty())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_resolves_member_from_index_access_alias_keyof_owner_type()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class BoxItem
---@field value integer
---@class Nested
---@field box BoxItem
---@alias NestedKey keyof Nested
---@type Nested[NestedKey]
local box = make_box()
print(box.value)"#;
    set_test_file(
        &mut compilation,
        154,
        "C:/ws/type_member_index_access_alias_keyof_owner.lua",
        source,
    );

    let member_offset = source.rfind("box.value").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(154),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("index access alias keyof owner member program point info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(!member_info.candidates.is_empty());
    assert!(
        member_info
            .candidates
            .iter()
            .any(|candidate| !candidate.explicit_type_offsets.is_empty())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_applies_field_literal_correlated_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Foo
---@field kind "foo"
---@field foo_value integer
---@class Bar
---@field kind "bar"
---@field bar_value string
---@type Foo|Bar
local value = make_value()
if value.kind == "foo" then
  print(value)
end"#;
    set_test_file(
        &mut compilation,
        43,
        "C:/ws/type_field_correlated_narrow.lua",
        source,
    );

    let value_offset = source.rfind("value)\nend").expect("narrowed value offset") as u32;
    let value_info = compilation
        .types()
        .name_at(
            FileId::new(43),
            TextSize::from(value_offset),
            TextSize::from(value_offset),
        )
        .expect("field literal correlated narrow info");

    assert!(value_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::FieldLiteral { member_name, literal_text, positive }
            if member_name == "kind" && literal_text == "\"foo\"" && *positive
    )));
    assert_eq!(value_info.candidates.len(), 1);
    assert_eq!(value_info.candidates[0].explicit_type_offsets.len(), 1);
}

#[test]
fn test_summary_builder_type_query_program_point_applies_false_branch_field_literal_correlated_narrow()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class Foo
---@field kind "foo"
---@class Bar
---@field kind "bar"
---@type Foo|Bar
local value = make_value()
if value.kind == "foo" then
  print("foo")
else
  print(value)
end"#;
    set_test_file(
        &mut compilation,
        44,
        "C:/ws/type_field_correlated_else_narrow.lua",
        source,
    );

    let value_offset = source.rfind("value").expect("else narrowed value offset") as u32;
    let value_info = compilation
        .types()
        .name_at(
            FileId::new(44),
            TextSize::from(value_offset),
            TextSize::from(value_offset),
        )
        .expect("false branch field literal correlated narrow info");

    assert!(value_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::FieldLiteral { member_name, literal_text, positive }
            if member_name == "kind" && literal_text == "\"foo\"" && !positive
    )));
    assert_eq!(value_info.candidates.len(), 1);
    assert_eq!(value_info.candidates[0].explicit_type_offsets.len(), 1);
}

#[test]
fn test_summary_builder_type_query_program_point_applies_member_field_literal_correlated_narrow() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Foo
---@field kind "foo"
---@field foo_value integer
---@class Bar
---@field kind "bar"
---@field bar_value string
---@type Foo|Bar
local value = make_value()
if value.kind == "foo" then
  print(value.foo_value)
end"#;
    set_test_file(
        &mut compilation,
        166,
        "C:/ws/type_member_field_correlated_narrow.lua",
        source,
    );

    let member_offset = source.rfind("foo_value").expect("foo_value offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(166),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("member field correlated narrow info");

    assert!(member_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::FieldLiteral { member_name, literal_text, positive }
            if member_name == "kind" && literal_text == "\"foo\"" && *positive
    )));
    assert_eq!(member_info.owner_candidates.len(), 1);
    assert_eq!(member_info.candidates.len(), 1);
    assert_eq!(member_info.candidates[0].explicit_type_offsets.len(), 1);
}

#[test]
fn test_summary_builder_type_query_program_point_rejects_member_from_other_field_literal_branch() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Foo
---@field kind "foo"
---@field foo_value integer
---@class Bar
---@field kind "bar"
---@field bar_value string
---@type Foo|Bar
local value = make_value()
if value.kind == "foo" then
  print(value.bar_value)
end"#;
    set_test_file(
        &mut compilation,
        167,
        "C:/ws/type_member_field_correlated_narrow_negative.lua",
        source,
    );

    let member_offset = source.rfind("bar_value").expect("bar_value offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(167),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("negative member field correlated narrow info");

    assert_eq!(member_info.owner_candidates.len(), 1);
    assert!(member_info.candidates.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_applies_else_member_field_literal_correlated_narrow()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@class Foo
---@field kind "foo"
---@field foo_value integer
---@class Bar
---@field kind "bar"
---@field bar_value string
---@type Foo|Bar
local value = make_value()
if value.kind == "foo" then
  print("foo")
else
  print(value.bar_value)
end"#;
    set_test_file(
        &mut compilation,
        168,
        "C:/ws/type_member_field_correlated_else_narrow.lua",
        source,
    );

    let member_offset = source.rfind("bar_value").expect("else bar_value offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(168),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("else member field correlated narrow info");

    assert!(member_info.active_narrows.iter().any(|narrow| matches!(
        narrow,
        crate::SalsaTypeNarrowSummary::FieldLiteral { member_name, literal_text, positive }
            if member_name == "kind" && literal_text == "\"foo\"" && !positive
    )));
    assert_eq!(member_info.owner_candidates.len(), 1);
    assert_eq!(member_info.candidates.len(), 1);
    assert_eq!(member_info.candidates[0].explicit_type_offsets.len(), 1);
}

#[test]
fn test_summary_builder_type_query_program_point_does_not_over_narrow_positive_or() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string|number
local guarded = pick()
if type(guarded) == "string" or type(guarded) == "number" then
  print(guarded)
end"#;
    set_test_file(
        &mut compilation,
        140,
        "C:/ws/type_positive_or_narrow.lua",
        source,
    );

    let guarded_offset = source.rfind("guarded)\nend").expect("or guarded offset") as u32;
    let guarded_info = compilation
        .types()
        .name_at(
            FileId::new(140),
            TextSize::from(guarded_offset),
            TextSize::from(guarded_offset),
        )
        .expect("positive or narrow info");

    assert!(guarded_info.active_narrows.is_empty());
    assert_eq!(guarded_info.candidates.len(), 1);
    assert!(!guarded_info.candidates[0].explicit_type_offsets.is_empty());
}

#[test]
fn test_summary_builder_type_query_program_point_applies_return_overload_bool_row() {
    let mut compilation = setup_compilation();
    let source = r#"---@param ok boolean
---@return boolean
---@return integer|string
---@return_overload true, integer
---@return_overload false, string
local function pick(ok)
  if ok then
    return true, 1
  end
  return false, "err"
end
---@type boolean
local cond = maybe()
local ok, result = pick(cond)
if ok then
  print(result)
else
  print(result)
end"#;
    set_test_file(
        &mut compilation,
        141,
        "C:/ws/type_return_overload_bool.lua",
        source,
    );

    let then_offset = source.find("print(result)").expect("then result offset") as u32 + 6;
    let else_offset = source.rfind("print(result)").expect("else result offset") as u32 + 6;
    let then_info = compilation
        .types()
        .name_at(
            FileId::new(141),
            TextSize::from(then_offset),
            TextSize::from(then_offset),
        )
        .expect("return overload then info");
    let else_info = compilation
        .types()
        .name_at(
            FileId::new(141),
            TextSize::from(else_offset),
            TextSize::from(else_offset),
        )
        .expect("return overload else info");

    assert_eq!(then_info.candidates.len(), 1);
    assert_eq!(else_info.candidates.len(), 1);
    assert_eq!(then_info.candidates[0].explicit_type_offsets.len(), 1);
    assert_eq!(else_info.candidates[0].explicit_type_offsets.len(), 1);
    assert_ne!(
        then_info.candidates[0].explicit_type_offsets,
        else_info.candidates[0].explicit_type_offsets
    );
}

#[test]
fn test_summary_builder_type_query_program_point_applies_return_overload_literal_row() {
    let mut compilation = setup_compilation();
    let source = r#"---@return "ok"|"err"
---@return integer|string
---@return_overload "ok", integer
---@return_overload "err", string
local function pick(ok)
  if ok then
    return "ok", 1
  end
  return "err", "boom"
end
---@type boolean
local cond = maybe()
local tag, result = pick(cond)
if tag == "ok" then
  print(result)
else
  print(result)
end"#;
    set_test_file(
        &mut compilation,
        142,
        "C:/ws/type_return_overload_literal.lua",
        source,
    );

    let then_offset = source
        .find("print(result)")
        .expect("literal then result offset") as u32
        + 6;
    let else_offset = source
        .rfind("print(result)")
        .expect("literal else result offset") as u32
        + 6;
    let then_info = compilation
        .types()
        .name_at(
            FileId::new(142),
            TextSize::from(then_offset),
            TextSize::from(then_offset),
        )
        .expect("literal overload then info");
    let else_info = compilation
        .types()
        .name_at(
            FileId::new(142),
            TextSize::from(else_offset),
            TextSize::from(else_offset),
        )
        .expect("literal overload else info");

    assert_eq!(then_info.candidates.len(), 1);
    assert_eq!(else_info.candidates.len(), 1);
    assert_eq!(then_info.candidates[0].explicit_type_offsets.len(), 1);
    assert_eq!(else_info.candidates[0].explicit_type_offsets.len(), 1);
    assert_ne!(
        then_info.candidates[0].explicit_type_offsets,
        else_info.candidates[0].explicit_type_offsets
    );
}

#[test]
fn test_summary_builder_type_query_program_point_return_overload_discriminant_rebind_breaks_correlation()
 {
    let mut compilation = setup_compilation();
    let source = r#"---@generic T, E
---@param ok boolean
---@param success T
---@param failure E
---@return boolean
---@return T|E
---@return_overload true, T
---@return_overload false, E
local function pick(ok, success, failure)
  if ok then
    return true, success
  end
  return false, failure
end
local cond = maybe() ---@type boolean
local ok, result = pick(cond, 1, "err")
ok = true
if not ok then
  print(result)
end
final = result"#;
    set_test_file(
        &mut compilation,
        145,
        "C:/ws/type_return_overload_rebind_break.lua",
        source,
    );

    let result_offset = source.rfind("result").expect("final result offset") as u32;
    let result_info = compilation
        .types()
        .name_at(
            FileId::new(145),
            TextSize::from(result_offset),
            TextSize::from(result_offset),
        )
        .expect("result info after discriminant rebind");

    assert_eq!(result_info.candidates.len(), 1);
    assert!(result_info.candidates[0].explicit_type_offsets.len() >= 2);
}

#[test]
fn test_summary_builder_type_query_program_point_keeps_return_overload_narrow_after_error_guard() {
    let mut compilation = setup_compilation();
    let source = r#"---@generic T, E
---@param ok boolean
---@param success T
---@param failure E
---@return boolean
---@return T|E
---@return_overload true, T
---@return_overload false, E
local function pick(ok, success, failure)
  if ok then
    return true, success
  end
  return false, failure
end
local cond = maybe() ---@type boolean
local ok, result = pick(cond, 1, "err")
if not ok then
  error(result)
end
after_ok = ok
final = result"#;
    set_test_file(
        &mut compilation,
        146,
        "C:/ws/type_return_overload_error_guard.lua",
        source,
    );

    let ok_offset = source.rfind("ok").expect("after ok offset") as u32;
    let result_offset = source.rfind("result").expect("final result offset") as u32;
    let ok_info = compilation
        .types()
        .name_at(
            FileId::new(146),
            TextSize::from(ok_offset),
            TextSize::from(ok_offset),
        )
        .expect("ok info after error guard");
    let result_info = compilation
        .types()
        .name_at(
            FileId::new(146),
            TextSize::from(result_offset),
            TextSize::from(result_offset),
        )
        .expect("result info after error guard");

    assert!(
        ok_info
            .active_narrows
            .contains(&crate::SalsaTypeNarrowSummary::Truthy)
    );
    assert_eq!(result_info.candidates.len(), 1);
    assert_eq!(result_info.candidates[0].explicit_type_offsets.len(), 1);
}

#[test]
fn test_summary_builder_type_query_program_point_keeps_noncall_origin_after_error_guard() {
    let mut compilation = setup_compilation();
    let source = r#"---@generic T, E
---@param ok boolean
---@param success T
---@param failure E
---@return boolean
---@return T|E
---@return_overload true, T
---@return_overload false, E
local function pick(ok, success, failure)
  if ok then
    return true, success
  end
  return false, failure
end
local cond = maybe() ---@type boolean
local branch = maybe() ---@type boolean
local ok, result = pick(cond, 1, "err")
if branch then
  result = false
end
if not ok then
  error(result)
end
final = result"#;
    set_test_file(
        &mut compilation,
        147,
        "C:/ws/type_return_overload_noncall_origin.lua",
        source,
    );

    let result_offset = source.rfind("result").expect("final result offset") as u32;
    let false_offset = source
        .find("result = false")
        .map(|offset| offset + "result = ".len())
        .expect("false assignment offset") as u32;
    let result_info = compilation
        .types()
        .name_at(
            FileId::new(147),
            TextSize::from(result_offset),
            TextSize::from(result_offset),
        )
        .expect("result info after mixed origins");

    assert_eq!(result_info.candidates.len(), 2);
    assert!(result_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets.len() == 1
            && candidate.origin
                == crate::SalsaTypeCandidateOriginSummary::Assignment(TextSize::from(
                    source
                        .find("local ok, result = pick")
                        .expect("call assignment offset") as u32,
                ))
    }));
    assert!(result_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets.is_empty()
            && candidate.initializer_offset == Some(TextSize::from(false_offset))
    }));
}

#[test]
fn test_summary_builder_type_query_program_point_keeps_mixed_rhs_call_groups_separate() {
    let mut compilation = setup_compilation();
    let source = r#"---@generic T, E
---@param ok boolean
---@param success T
---@param failure E
---@return boolean
---@return T|E
---@return_overload true, T
---@return_overload false, E
local function pick(ok, success, failure)
  if ok then
    return true, success
  end
  return false, failure
end
local cond = maybe() ---@type boolean
local left_ok, right_ok, right_result = pick(cond, "left-ok", "left-err"), pick(cond, 1, "right-err")
if not left_ok then
  error("left failed")
end
a = right_result
if not right_ok then
  error(right_result)
end
b = right_result"#;
    set_test_file(
        &mut compilation,
        149,
        "C:/ws/type_return_overload_mixed_rhs_calls.lua",
        source,
    );

    let a_offset = source
        .find("right_result\nif not right_ok")
        .expect("a result offset") as u32;
    let b_offset = source.rfind("right_result").expect("b result offset") as u32;
    let a_info = compilation
        .types()
        .name_at(
            FileId::new(149),
            TextSize::from(a_offset),
            TextSize::from(a_offset),
        )
        .expect("a info before right guard");
    let b_info = compilation
        .types()
        .name_at(
            FileId::new(149),
            TextSize::from(b_offset),
            TextSize::from(b_offset),
        )
        .expect("b info after right guard");

    assert_eq!(a_info.candidates.len(), 1);
    assert_eq!(a_info.candidates[0].explicit_type_offsets.len(), 2);
    assert_eq!(b_info.candidates.len(), 1);
    assert_eq!(b_info.candidates[0].explicit_type_offsets.len(), 1);
}

#[test]
fn test_summary_builder_type_query_program_point_branch_reassign_keeps_join_mapping() {
    let mut compilation = setup_compilation();
    let source = r#"---@generic T, E
---@param ok boolean
---@param success T
---@param failure E
---@return boolean
---@return T|E
---@return_overload true, T
---@return_overload false, E
local function pick(ok, success, failure)
  if ok then
    return true, success
  end
  return false, failure
end
local cond = maybe() ---@type boolean
local branch = maybe() ---@type boolean
local ok, result = pick(cond, 1, "left-err")
if branch then
  ok, result = pick(cond, "branch-ok", false)
end
if not ok then
  error(result)
end
a = result"#;
    set_test_file(
        &mut compilation,
        150,
        "C:/ws/type_return_overload_branch_join.lua",
        source,
    );

    let result_offset = source.rfind("result").expect("joined result offset") as u32;
    let result_info = compilation
        .types()
        .name_at(
            FileId::new(150),
            TextSize::from(result_offset),
            TextSize::from(result_offset),
        )
        .expect("result info after branch join and guard");

    assert_eq!(result_info.candidates.len(), 2);
    assert!(
        result_info
            .candidates
            .iter()
            .all(|candidate| candidate.explicit_type_offsets.len() == 1)
    );
}

#[test]
fn test_summary_builder_type_query_program_point_member_uses_multi_return_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

local holder = {}
holder.first, holder.second = pair()
print(holder.second)"#;
    set_test_file(
        &mut compilation,
        155,
        "C:/ws/type_member_multi_return_slot.lua",
        source,
    );

    let signature = compilation
        .doc()
        .signatures(FileId::new(155))
        .expect("signature summary")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");
    let signature_summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(155), signature.syntax_offset)
        .expect("pair signature return summary");

    let member_offset = source.rfind("holder.second").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(155),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("member program point info");

    assert!(member_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets == signature_summary.values[1].doc_return_type_offsets
    }));
    assert!(!member_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets == signature_summary.values[0].doc_return_type_offsets
    }));
}

#[test]
fn test_summary_builder_type_query_program_point_property_candidate_keeps_signature_offset() {
    let mut compilation = setup_compilation();
    let source = r#"local values = {
  run = function()
    return 1
  end,
}
print(values.run)"#;
    set_test_file(
        &mut compilation,
        156,
        "C:/ws/type_property_program_point_signature_offset.lua",
        source,
    );

    let member_offset = source.rfind("values.run").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(156),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("member program point info");

    assert!(
        member_info
            .candidates
            .iter()
            .any(|candidate| candidate.signature_offset.is_some())
    );
}

#[test]
fn test_summary_builder_type_query_program_point_property_uses_multi_return_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

local holder = { pair() }
print(holder[2])"#;
    set_test_file(
        &mut compilation,
        157,
        "C:/ws/type_property_multi_return_slot.lua",
        source,
    );

    let signature = compilation
        .doc()
        .signatures(FileId::new(157))
        .expect("signature summary")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");
    let signature_summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(157), signature.syntax_offset)
        .expect("pair signature return summary");

    let member_offset = source.rfind("holder[2]").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(157),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("member program point info");

    assert!(member_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets == signature_summary.values[1].doc_return_type_offsets
    }));
    assert!(!member_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets == signature_summary.values[0].doc_return_type_offsets
    }));
}

#[test]
fn test_summary_builder_type_query_program_point_named_type_property_uses_multi_return_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

---@class Pair
local Pair = { pair() }

---@type Pair
local holder = make_pair()
print(holder[2])"#;
    set_test_file(
        &mut compilation,
        165,
        "C:/ws/type_property_named_type_multi_return_slot.lua",
        source,
    );

    let signature = compilation
        .doc()
        .signatures(FileId::new(165))
        .expect("signature summary")
        .signatures
        .clone()
        .into_iter()
        .find(|signature| signature.name.as_deref() == Some("pair"))
        .expect("pair signature");
    let signature_summary = compilation
        .semantic()
        .file()
        .signature_return_summary(FileId::new(165), signature.syntax_offset)
        .expect("pair signature return summary");

    let member_offset = source.rfind("holder[2]").expect("member use offset") as u32;
    let member_info = compilation
        .types()
        .member_at(
            FileId::new(165),
            TextSize::from(member_offset),
            TextSize::from(member_offset),
        )
        .expect("named type member program point info");

    assert!(member_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets == signature_summary.values[1].doc_return_type_offsets
    }));
    assert!(!member_info.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets == signature_summary.values[0].doc_return_type_offsets
    }));
}

#[test]
fn test_summary_builder_type_query_program_point_join_with_noncorrelated_origin_keeps_extra_type() {
    let mut compilation = setup_compilation();
    let source = r#"---@param ok boolean
---@return_overload true, integer
---@return_overload false, string
local function pick(ok)
  if ok then
    return true, 1
  end
  return false, "err"
end
---@return false
local function as_false()
  return false
end
local cond = maybe() ---@type boolean
local branch = maybe() ---@type boolean
local ok, result = pick(cond)
if branch then
  ok, result = true, as_false()
end
at_join = result
if not ok then
  in_error_path = result
  error(result)
end
after_guard = result"#;
    set_test_file(
        &mut compilation,
        151,
        "C:/ws/type_return_overload_noncorrelated_join.lua",
        source,
    );

    let after_guard_offset = source.rfind("result").expect("after guard result offset") as u32;
    let result_info = compilation
        .types()
        .name_at(
            FileId::new(151),
            TextSize::from(after_guard_offset),
            TextSize::from(after_guard_offset),
        )
        .expect("result info after noncorrelated join and guard");

    assert_eq!(result_info.candidates.len(), 2);
    assert!(
        result_info
            .candidates
            .iter()
            .all(|candidate| candidate.explicit_type_offsets.len() == 1)
    );
}
