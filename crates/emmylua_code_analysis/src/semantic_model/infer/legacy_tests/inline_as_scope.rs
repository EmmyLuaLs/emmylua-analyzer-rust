use emmylua_parser::{LuaAstNode, LuaExpr};
use googletest::prelude::*;

use crate::{DiagnosticCode, FileId, LuaType, VirtualWorkspace};

fn expression_type(ws: &VirtualWorkspace, file: FileId, text: &str) -> LuaType {
    let model = ws.analysis.semantic_model(file);
    let expressions: Vec<_> = model
        .chunk()
        .expect("chunk")
        .descendants::<LuaExpr>()
        .filter(|expr| expr.get_text() == text)
        .collect();
    assert_eq!(
        expressions.len(),
        1,
        "expected exactly one expression: {text}"
    );
    let expr = &expressions[0];
    model.type_of_expr_at(expr.get_syntax_id(), expr.get_position())
}

const RECORD_TYPES: &str = r#"
---@class Train
---@field id integer
---@class TrainData
---@field train Train
---@field surface string
---@class FreeTrain
---@field train Train
---@field surface string
"#;

#[gtest]
fn inline_as_table_does_not_cast_its_fields() {
    let mut ws = VirtualWorkspace::new();
    ws.def(RECORD_TYPES);
    let file = ws.def(r#"
        ---@param train_data TrainData
        local function create(train_data)
            local result = { train = train_data.train, surface = train_data.surface } --[[@as FreeTrain]]
            return result
        end
    "#);
    assert_that!(
        expression_type(&ws, file, "train_data.train"),
        eq(&ws.ty("Train"))
    );
    assert_that!(
        expression_type(&ws, file, "train_data.surface"),
        eq(&LuaType::String)
    );
    assert_that!(
        expression_type(
            &ws,
            file,
            "{ train = train_data.train, surface = train_data.surface }"
        ),
        eq(&ws.ty("FreeTrain"))
    );
    assert_that!(
        expression_type(&ws, file, "result"),
        eq(&ws.ty("FreeTrain"))
    );
}

#[gtest]
fn inline_as_call_does_not_cast_its_argument() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    let file = ws.def(
        r#"
        ---@param input string
        local function convert(input)
            local result = tostring(input) --[[@as integer]]
            return result
        end
    "#,
    );
    assert_that!(expression_type(&ws, file, "input"), eq(&LuaType::String));
    assert_that!(
        expression_type(&ws, file, "tostring(input)"),
        eq(&LuaType::Integer)
    );
    assert_that!(expression_type(&ws, file, "result"), eq(&LuaType::Integer));
}

#[gtest]
fn inline_as_parenthesized_binary_does_not_cast_its_operands() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@param left number
        ---@param right number
        local function convert(left, right)
            local result = (left + right) --[[@as string]]
            return result
        end
    "#,
    );
    assert_that!(expression_type(&ws, file, "left"), eq(&LuaType::Number));
    assert_that!(expression_type(&ws, file, "right"), eq(&LuaType::Number));
    assert_that!(
        expression_type(&ws, file, "left + right"),
        eq(&LuaType::Number)
    );
    assert_that!(
        expression_type(&ws, file, "(left + right)"),
        eq(&LuaType::String)
    );
    assert_that!(expression_type(&ws, file, "result"), eq(&LuaType::String));
}

#[gtest]
fn inline_as_does_not_cast_unrelated_later_reads() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@class Record
        ---@field text string
        ---@param input string
        ---@param other boolean
        ---@param record Record
        local function convert(input, other, record)
            local adjusted = input --[[@as integer]]
            local untouched = other
            local member = record.text
            return adjusted, untouched, member
        end
    "#,
    );
    assert_that!(expression_type(&ws, file, "other"), eq(&LuaType::Boolean));
    assert_that!(
        expression_type(&ws, file, "record.text"),
        eq(&LuaType::String)
    );
    assert_that!(
        expression_type(&ws, file, "adjusted"),
        eq(&LuaType::Integer)
    );
}

#[gtest]
fn inline_as_table_assignment_checks_actual_field_types() {
    for valid in [true, false] {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(RECORD_TYPES);
        let surface = if valid { "train_data.surface" } else { "123" };
        let no_diagnostic = ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &format!(r#"
            ---@param train_data TrainData
            local function create(train_data)
                ---@type FreeTrain
                local free_train = {{ train = train_data.train, surface = {surface} }} --[[@as FreeTrain]]
                return free_train
            end
        "#));
        assert_that!(no_diagnostic, eq(valid));
    }
}

#[gtest]
fn inline_as_member_assignment_preserves_result_and_source_type() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@param input string
        local function convert(input)
            local holder = {}
            holder.value = input --[[@as integer]]
            local read = holder.value
            local original = input
            return read, original
        end
    "#,
    );
    assert_that!(expression_type(&ws, file, "read"), eq(&LuaType::Integer));
    assert_that!(expression_type(&ws, file, "original"), eq(&LuaType::String));
}

#[gtest]
fn inline_as_multi_value_statement_only_casts_last_expression() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@param input string
        ---@param other boolean
        local function convert(input, other)
            local first, second = input, other --[[@as integer]]
            return first, second
        end
    "#,
    );
    assert_that!(expression_type(&ws, file, "first"), eq(&LuaType::String));
    assert_that!(expression_type(&ws, file, "second"), eq(&LuaType::Integer));
}

#[gtest]
fn inline_as_nested_assertions_keep_their_own_targets() {
    let mut ws = VirtualWorkspace::new();
    ws.def("---@class Container\n---@field value integer\n---@field other boolean");
    let file = ws.def(
        r#"
        ---@param input string
        ---@param other boolean
        local function convert(input, other)
            local result = {
                value = input --[[@as integer]],
                other = other,
            } --[[@as Container]]
            return result
        end
    "#,
    );
    assert_that!(expression_type(&ws, file, "input"), eq(&LuaType::Integer));
    assert_that!(expression_type(&ws, file, "other"), eq(&LuaType::Boolean));
    assert_that!(
        expression_type(&ws, file, "result"),
        eq(&ws.ty("Container"))
    );
}

#[gtest]
fn inline_as_return_and_closure_assertions_still_apply() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@param input string
        function convert(input)
            return input --[[@as integer]]
        end
    "#,
    );
    assert_that!(expression_type(&ws, file, "input"), eq(&LuaType::Integer));
    let file = ws.def(
        r#"
        local fn = function() return 1 end --[[@as string]]
        return fn
    "#,
    );
    assert_that!(expression_type(&ws, file, "fn"), eq(&LuaType::String));
}
