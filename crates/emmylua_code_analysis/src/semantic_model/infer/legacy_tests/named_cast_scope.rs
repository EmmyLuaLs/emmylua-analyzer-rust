use crate::{FileId, LuaType, VirtualWorkspace};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaLocalName};
use googletest::prelude::*;

fn local_type(ws: &VirtualWorkspace, file: FileId, name: &str) -> LuaType {
    let model = ws.analysis.semantic_model(file);
    let chunk = model.chunk().expect("chunk");
    let locals: Vec<_> = chunk
        .descendants::<LuaLocalName>()
        .filter(|local| {
            local
                .get_name_token()
                .is_some_and(|token| token.get_name_text() == name)
        })
        .collect();
    assert_eq!(locals.len(), 1, "expected one local named {name:?}");
    let local = &locals[0];
    let decl = model
        .decl_by_offset(local.get_name_token().expect("name token").get_position())
        .unwrap_or_else(|| panic!("missing declaration for {name:?}"));
    model
        .type_of_decl(&decl)
        .unwrap_or_else(|| panic!("missing type for {name:?}"))
}

#[gtest]
fn named_local_cast_does_not_replace_array_element_type() {
    for strict in [false, true] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut config = ws.get_emmyrc();
        config.strict.array_index = strict;
        ws.update_emmyrc(config);
        let file = ws.def(
            r#"
            ---@param values string[]
            ---@param index integer?
            local function select_value(values, index)
                ---@cast index integer
                local direct = values[index]
                ---@cast index integer
                local checked = assert(values[index])
                return direct, checked
            end
            "#,
        );
        let expected = ws.ty(if strict { "string?" } else { "string" });
        expect_that!(local_type(&ws, file, "direct"), eq(&expected));
        expect_that!(local_type(&ws, file, "checked"), eq(&LuaType::String));
    }
}

#[gtest]
fn named_member_cast_narrows_only_the_array_key() {
    for strict in [false, true] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut config = ws.get_emmyrc();
        config.strict.array_index = strict;
        ws.update_emmyrc(config);
        let file = ws.def(
            r#"
            ---@alias uint32 integer
            ---@class Entry
            ---@field name string
            local Entry = {}
            ---@class Position
            ---@field index uint32?
            local Position = {}

            ---@param values Entry[]
            ---@param position Position
            local function select_entry(values, position)
                ---@cast position.index uint32
                local direct = values[position.index]
                ---@cast position.index uint32
                local checked = assert(values[position.index])
                local later = values[position.index]
                return direct, checked, later
            end
            "#,
        );
        let expected = ws.ty(if strict { "Entry?" } else { "Entry" });
        expect_that!(local_type(&ws, file, "direct"), eq(&expected));
        expect_that!(local_type(&ws, file, "later"), eq(&expected));
        let checked = ws.ty("Entry");
        expect_that!(local_type(&ws, file, "checked"), eq(&checked));
    }
}

#[gtest]
fn named_cast_operators_do_not_change_unrelated_indexed_values() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@param values string[]
        ---@param index integer?
        ---@param unrelated unknown
        local function select_values(values, index, unrelated)
            ---@cast unrelated integer
            local replaced = values[1]
            ---@cast unrelated +string
            local widened = values[1]
            ---@cast index -?
            local narrowed = values[index]
            return replaced, widened, narrowed
        end
        "#,
    );
    let expected = ws.ty("string?");
    for name in ["replaced", "widened", "narrowed"] {
        expect_that!(local_type(&ws, file, name), eq(&expected));
    }
}

#[gtest]
fn member_key_narrowing_reaches_nested_index_and_call_arguments() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    let file = ws.def(
        r#"
        ---@class NestedPosition
        ---@field index integer?
        local NestedPosition = {}

        ---@param values string[][]
        ---@param position NestedPosition
        local function select_value(values, position)
            ---@cast position.index -?
            local selected = assert(assert(values[position.index])[position.index])
            return selected
        end
        "#,
    );
    expect_that!(local_type(&ws, file, "selected"), eq(&LuaType::String));
}

#[gtest]
fn inline_expression_casts_still_apply() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@param values string[]
        local function narrow_value(values)
            local narrowed = values[1] --[[@cast -?]]
            return narrowed
        end
        ---@param values string[]
        local function widen_value(values)
            local widened = values[1] --[[@cast +boolean]]
            return widened
        end
        ---@param values string[]
        local function replace_value(values)
            local replaced = values[1] --[[@as integer]]
            return replaced
        end
        "#,
    );
    expect_that!(local_type(&ws, file, "narrowed"), eq(&LuaType::String));
    let widened = ws.ty("string|nil|boolean");
    expect_that!(local_type(&ws, file, "widened"), eq(&widened));
    expect_that!(local_type(&ws, file, "replaced"), eq(&LuaType::Integer));
}

#[gtest]
fn named_owner_cast_and_keyless_statement_cast_still_apply() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@param values unknown
        local function cast_owner(values)
            ---@cast values string[]
            local owner_cast = values[1]
            return owner_cast
        end
        ---@param values string[]
        local function cast_expression(values)
            ---@cast -?
            local expression_cast = values[1]
            return expression_cast
        end
        "#,
    );
    let optional = ws.ty("string?");
    expect_that!(local_type(&ws, file, "owner_cast"), eq(&optional));
    expect_that!(
        local_type(&ws, file, "expression_cast"),
        eq(&LuaType::String)
    );
}

#[gtest]
fn dynamic_keys_preserve_nil_and_never_casts() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@class KeyPosition
        ---@field index integer
        local KeyPosition = {}
        ---@param values string[]
        ---@param position KeyPosition
        local function nil_key(values, position)
            ---@cast position.index nil
            local nil_index = position.index
            local nil_result = values[position.index]
            return nil_result
        end
        ---@param values string[]
        ---@param position KeyPosition
        local function never_key(values, position)
            ---@cast position.index never
            local never_index = position.index
            local never_result = values[position.index]
            return never_result
        end
        "#,
    );
    expect_that!(local_type(&ws, file, "nil_index"), eq(&LuaType::Nil));
    expect_that!(local_type(&ws, file, "never_index"), eq(&LuaType::Never));
    // A nil/unreachable key cannot produce a useful array element type.
    expect_that!(local_type(&ws, file, "nil_result"), eq(&LuaType::Unknown));
    expect_that!(local_type(&ws, file, "never_result"), eq(&LuaType::Unknown));
}

#[test]
#[ntest::timeout(5000)]
fn nested_dynamic_keys_and_recursive_dependencies_terminate() {
    let mut ws = VirtualWorkspace::new();
    let mut config = ws.get_emmyrc();
    config.strict.array_index = false;
    ws.update_emmyrc(config);
    let mut key = "index".to_owned();
    for _ in 0..64 {
        key = format!("indices[{key}]");
    }
    let file = ws.def(&format!(
        r#"
        ---@param indices integer[]
        ---@param index integer
        local function nested(indices, index)
            local nested_result = indices[{key}]
            return nested_result
        end
        "#,
    ));
    assert_eq!(local_type(&ws, file, "nested_result"), LuaType::Integer);
    let recursive = ws.def(
        r#"
        local values = {}
        local function key()
            return values[key()]
        end
        local recursive_result = values[key()]
        "#,
    );
    assert_eq!(
        local_type(&ws, recursive, "recursive_result"),
        LuaType::Unknown
    );
}

const RECURSIVE_OWNER_TYPES: &str = r#"
---@class WithIndex
---@field next WithIndex|WithoutIndex
---@field start_index integer
---@class WithoutIndex
---@field next WithIndex|WithoutIndex
---@field other boolean
"#;

#[gtest]
fn ambiguous_union_member_cast_matches_only_its_owner_and_path() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(&format!(
        r#"{RECURSIVE_OWNER_TYPES}
        ---@param left WithIndex|WithoutIndex
        ---@param right WithIndex|WithoutIndex
        local function cast_member(left, right)
            ---@cast left.next WithIndex
            local narrowed_owner = left.next
            ---@cast left.next WithIndex
            local bracket_owner = left["next"]
            ---@cast left.next WithIndex
            local projected_field = left.next.start_index
            ---@cast left.next WithIndex
            local other_owner = right.next
            ---@cast left.next WithIndex
            local other_path = left.next.next
            return narrowed_owner, projected_field, other_owner, other_path
        end
        "#
    ));
    let narrowed = ws.ty("WithIndex");
    let union = ws.ty("WithIndex|WithoutIndex");
    assert_that!(local_type(&ws, file, "narrowed_owner"), eq(&narrowed));
    assert_that!(local_type(&ws, file, "bracket_owner"), eq(&narrowed));
    assert_that!(
        local_type(&ws, file, "projected_field"),
        eq(&LuaType::Integer)
    );
    assert_that!(local_type(&ws, file, "other_owner"), eq(&union));
    assert_that!(local_type(&ws, file, "other_path"), eq(&union));
}

#[gtest]
fn ambiguous_member_cast_does_not_survive_owner_reassignment_or_shadowing() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(&format!(
        r#"{RECURSIVE_OWNER_TYPES}
        ---@param node WithIndex|WithoutIndex
        ---@param other WithIndex|WithoutIndex
        local function cast_member(node, other)
            ---@cast node.next WithIndex
            do
                local node = other
                local shadowed_owner = node.next
            end
            ---@cast node.next WithIndex
            node = other
            local reassigned_owner = node.next
            return reassigned_owner
        end
        ---@param entry WithIndex|WithoutIndex
        ---@param other WithIndex|WithoutIndex
        local function replace_member(entry, other)
            ---@cast entry.next WithIndex
            entry.next = other
            local reassigned_member = entry.next
            return reassigned_member
        end
        "#
    ));
    let union = ws.ty("WithIndex|WithoutIndex");
    assert_that!(local_type(&ws, file, "shadowed_owner"), eq(&union));
    assert_that!(local_type(&ws, file, "reassigned_owner"), eq(&union));
    assert_that!(local_type(&ws, file, "reassigned_member"), eq(&union));
}

#[test]
#[ntest::timeout(5000)]
fn recursive_union_member_cast_preserves_the_selected_owner() {
    let mut ws = VirtualWorkspace::new();
    let prefix = format!("node{}", ".next".repeat(16));
    let file = ws.def(&format!(
        r#"{RECURSIVE_OWNER_TYPES}
        ---@param node WithIndex|WithoutIndex
        local function cast_member(node)
            ---@cast {prefix} WithIndex
            local recursive_owner = {prefix}
            ---@cast {prefix} WithIndex
            local recursive_field = {prefix}.start_index
            return recursive_owner, recursive_field
        end
        "#
    ));
    assert_eq!(local_type(&ws, file, "recursive_owner"), ws.ty("WithIndex"));
    assert_eq!(local_type(&ws, file, "recursive_field"), LuaType::Integer);
}
