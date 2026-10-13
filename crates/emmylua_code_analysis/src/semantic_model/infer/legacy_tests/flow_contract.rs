use crate::{DiagnosticCode, test_lib::VirtualWorkspace};
use lsp_types::NumberOrString;
use tokio_util::sync::CancellationToken;

/// The specification's examples exercise the public diagnostic path, including
/// the absence of errors on accepted reads, writes, arguments, and returns.
#[test]
fn flow_typing_contract() {
    let document = include_str!("../../../../../../docs/emmylua_code_analysis/flow-typing.md");
    let mut failures = Vec::new();
    for (index, section) in document.split("```lua\n").skip(1).enumerate() {
        let code = section.split("```").next().unwrap();
        failures.extend(
            check_contract(code)
                .into_iter()
                .map(|failure| format!("example {}: {failure}", index + 1)),
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn check_contract(code: &str) -> Vec<String> {
    check_contract_in(&mut VirtualWorkspace::new_with_init_std_lib(), code)
}

fn check_contract_in(ws: &mut VirtualWorkspace, code: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let file = ws.def(code);
    let diagnostics = ws
        .analysis
        .diagnose_file(file, CancellationToken::new())
        .unwrap_or_default();
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            [
                DiagnosticCode::AssignTypeMismatch,
                DiagnosticCode::ParamTypeMismatch,
                DiagnosticCode::ReturnTypeMismatch,
                DiagnosticCode::UndefinedField,
                DiagnosticCode::MissingFields,
            ]
            .iter()
            .any(|code| diagnostic.code == Some(NumberOrString::String(code.get_name().to_owned())))
        })
        .collect();
    for (line, text) in code.lines().enumerate() {
        let expected = text.contains("-- Error:");
        let actual: Vec<_> = errors
            .iter()
            .filter(|diagnostic| diagnostic.range.start.line as usize == line)
            .collect();
        if expected != !actual.is_empty() {
            failures.push(format!(
                "line {}: {text}\n  expected error: {expected}; {actual:?}",
                line + 1
            ));
        }
    }
    failures
}

fn assert_contract(code: &str) {
    let failures = check_contract(code);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn construction_seals_at_uses_of_the_whole_value() {
    assert_contract(
        r#"
        local alias_source = {}
        alias_source.text = 'hello'
        local alias = alias_source
        alias_source.extra = 1 -- Error: aliasing seals the source.
        alias.extra = 1        -- Error: an alias cannot add fields.

        local passed = {}
        passed.text = 'hello'
        local function consume(value) end
        consume(passed)
        passed.extra = 1       -- Error: passing the table seals it.

        local stored = {}
        stored.text = 'hello'
        local holder = {value = stored}
        stored.extra = 1       -- Error: storage elsewhere seals the table.

        local nested = {child = {text = 'hello'}}
        nested.child.text = 'updated'
        nested.child.extra = 1 -- Error: nested writes use the existing child's type.
        nested.extra = 1       -- Error: a nested write seals its owner.

        local parenthesized = ({})
        parenthesized.text = 'hello'
        ---@type {text: string}
        local record = parenthesized
        parenthesized.extra = 1 -- Error: parentheses preserve construction and sealing.

        local replaced = {text = 'hello'}
        replaced = {text = 'updated'}
        replaced.extra = 1     -- Error: reassignment does not restart construction.
    "#,
    );
}

#[test]
fn conditional_construction_uses_only_paths_reaching_the_use() {
    assert_contract(
        r#"
        ---@param flag boolean
        local function build(flag)
            local value = {}
            if flag then return end
            value.text = 'hello'
            ---@type {text: string}
            local record = value
            return record
        end

        ---@param flag boolean
        local function established(flag)
            local value = {text = 'hello'}
            if flag then value.text = 1 end -- Error: the stable field type is string.
            return value
        end

        ---@param flag boolean
        local function loop(flag)
            local value = {}
            while flag do
                value.text = 'hello'
                if flag then break end
            end
            ---@type {text?: string}
            local optional = value
            ---@type {text: string}
            local required = value -- Error: the loop can execute zero times.
            return optional, required
        end
    "#,
    );
}

#[test]
fn error_paths_do_not_make_constructed_fields_optional() {
    assert_contract(
        r#"
        ---@param stop boolean
        local function build(stop)
            local value = {}
            if stop then error('stop') else value.text = 'hello' end
            ---@type {text: string}
            local record = value
            return record
        end
        "#,
    );
}

#[test]
fn declared_never_calls_do_not_reach_construction_joins() {
    assert_contract(
        r#"
        ---@return never
        local function stop() error('stop') end
        ---@param fail boolean
        local function build(fail)
            local value = {}
            if fail then stop() else value.text = 'hello' end
            ---@type {text: string}
            local record = value
            return record
        end

        ---@alias Stop fun(): never
        ---@param stop Stop
        ---@param fail boolean
        local function aliased_stop(stop, fail)
            local value = {}
            if fail then stop() else value.text = 'hello' end
            ---@type {text: string}
            local record = value
            return record
        end

        ---@param stop (fun(value: boolean): never) | (fun(value: string): nil)
        ---@param fail boolean
        local function may_return(stop, fail)
            local value = {}
            if fail then stop('continue') else value.text = 'hello' end
            ---@type {text: string}
            local record = value -- Error: one callable alternative can return.
            return record
        end

        ---@param stop (fun(value: boolean): never) | (fun(value: string): never)
        ---@param fail boolean
        local function always_stops(stop, fail)
            local value = {}
            if fail then stop('stop') else value.text = 'hello' end
            ---@type {text: string}
            local record = value
            return record
        end
        "#,
    );
}

#[test]
fn literal_true_loops_use_only_break_exits() {
    assert_contract(
        r#"
        local value = {}
        while true do
            value.text = 'hello'
            break
        end
        ---@type {text: string}
        local record = value

        ---@param done boolean
        local function build(done)
            local value = {}
            while (true) do
                if done then
                    value.text = 'hello'
                    break
                end
            end
            ---@type {text: string}
            local record = value
            return record
        end
        "#,
    );
}

#[test]
fn field_assertions_do_not_disable_table_construction() {
    assert_contract(
        r#"
        local value = {count = 1 --[[@as integer]]}
        ---@type {text: string}
        local record = value -- Error: a field assertion does not provide text.
        value.text = 'late' -- Error: the whole-table use seals value before this write.

        local built = {text = 'hello' --[[@as string]]}
        built.count = 1
        ---@type {text: string, count: integer}
        local record = built
        built.extra = 1 -- Error: field assertions preserve normal sealing.
        "#,
    );
}

#[test]
fn overlapping_index_writes_respect_existing_contracts() {
    assert_contract(
        r#"
        ---@alias TextOrCount string | integer
        ---@alias TextOrFlag string | boolean
        ---@param text_key string
        ---@param count_key integer
        ---@param mixed_key TextOrCount
        ---@param other_key TextOrFlag
        local function build(text_key, count_key, mixed_key, other_key)
            local value = {}
            value[text_key] = 'hello'
            value[mixed_key] = 1 -- Error: mixed_key can select the string index.

            local partial = {}
            partial[mixed_key] = 'hello'
            partial[other_key] = 1 -- Error: the key unions overlap on string.

            local disjoint = {}
            disjoint[text_key] = 'hello'
            disjoint[count_key] = 1
            disjoint[mixed_key] = 'updated' -- Error: the integer index still requires integers.

            local compatible = {}
            compatible[mixed_key] = 'hello'
            compatible[other_key] = 'updated'

            local sealed = {}
            sealed[text_key] = 'hello'
            local record = sealed
            sealed[mixed_key] = 'updated' -- Error: the sealed string index cannot add integer keys.
            return compatible
        end
        "#,
    );
}

#[test]
fn sealed_writes_require_coverage_of_the_whole_key() {
    assert_contract(
        r#"
        ---@param key 'text' | 'extra'
        local function named(key)
            local value = {text = 'hello'}
            local sealed = value
            value[key] = 'updated' -- Error: extra is outside the sealed shape.
            return value
        end

        ---@param text_key string
        ---@param count_key integer
        ---@param key string | integer
        local function indexed(text_key, count_key, key)
            local value = {}
            value[text_key] = 'hello'
            value[count_key] = 'hello'
            local sealed = value
            value[key] = 'updated'
            return value
        end
        "#,
    );
}

#[test]
fn function_fields_use_the_sealed_structure() {
    assert_contract(
        r#"
        local module = {}
        function module.read()
            return module.text
        end
        module.copy = function() return module.text end
        function module:get() return self.text end
        module.text = 'hello'
        ---@type string
        local first = module.read()
        ---@type string
        local second = module.copy()
        ---@type string
        local third = module:get()
        module.extra = 1 -- Error: a call seals the owner.

        local replaced = {}
        ---@return string | integer
        function replaced.read() return 'hello' end
        ---@return integer
        function replaced.read() return 1 end
        ---@type fun(): integer
        local callback = replaced.read
        ---@type integer
        local number = replaced.read()

        do
            local unused = {}
            function unused.read()
                return unused.missing -- Error: unused bodies are checked at scope end.
            end
        end
    "#,
    );
}

#[test]
fn computed_function_reads_seal_construction() {
    assert_contract(
        r#"
        local value = {}
        function value.read()
            return value.text -- Error: the function is retrieved before text exists.
        end
        local key = 'read'
        local read = value[key]
        local text = read()
        value.text = 'late' -- Error: the computed function read seals value.

        ---@param key string
        local function dynamic(key)
            local value = {text = 'hello'}
            function value.read() return value.text end
            local member = value[key]
            value.extra = 1 -- Error: the dynamic key can retrieve read.
        end

        ---@param key string
        local function dictionary(key)
            local value = {}
            value[key] = function() return 'hello' end
            local read = value[key]
            value[1] = 'extra' -- Error: a function index entry also seals construction.
        end

        local reassigned = {text = 'hello', read = function() return 'hello' end}
        local key = 'text'
        key = 'read'
        local copied_key = key
        local read = reassigned[copied_key]
        reassigned.extra = 1 -- Error: a reassigned initializer cannot hide a function key.
        "#,
    );
}

#[test]
fn known_computed_data_reads_keep_construction_open() {
    assert_contract(
        r#"
        local value = {text = 'hello'}
        function value.read() return value.text end
        local first_key = 'text'
        local key = (first_key)
        local text = value[key]
        value.extra = 1

        ---@param key integer
        local function dictionary(key)
            local value = {text = 'hello'}
            value[key] = function() return 'hello' end
            local field_key = 'text'
            local text = value[field_key]
            value.extra = 1
        end
        "#,
    );
}

#[test]
fn computed_reads_select_only_function_keys_in_their_domain() {
    assert_contract(
        r#"
        ---@alias CountKey integer
        ---@param key CountKey
        local function numeric_data(key)
            local value = {[1] = 'hello'}
            function value.read() return 'hello' end
            local text = value[key]
            value.extra = 1
            return value
        end

        ---@param key string
        local function string_data(key)
            local value = {text = 'hello', [1] = function() return 'hello' end}
            local copied_key = key
            local text = value[copied_key]
            value.extra = 1
            return value
        end

        ---@param text_key string
        ---@param count_key integer
        local function function_index(text_key, count_key)
            local value = {text = 'hello'}
            value[count_key] = function() return 'hello' end
            local text = value[text_key]
            value.extra = 1
            return value
        end

        ---@param key string | integer
        local function possible_function(key)
            local value = {text = 'hello', [1] = function() return 'hello' end}
            local read = value[key]
            value.extra = 1 -- Error: an integer alternative can retrieve the function.
        end
        "#,
    );
}

#[test]
fn array_writes_use_the_declared_element_type() {
    assert_contract(
        r#"
        ---@type string[]
        local values = {'hello'}
        values[1] = 1 -- Error: the stable element type is string.
        values[1] = 'updated'
        values[1] = nil

        ---@param index integer
        local function fill(index)
            ---@type string[]
            local values = {}
            values[index] = 1 -- Error: an empty initializer keeps the declared element type.
            values[index] = 'hello'
            values[index] = nil
        end
        "#,
    );
}

#[test]
fn computed_reads_combine_selected_fields_and_keep_missing_entries_nullable() {
    assert_contract(
        r#"
        ---@alias DataKey 'count' | 'text'
        ---@param key DataKey
        ---@param missing_key 'count' | 'missing'
        ---@param broad_key string
        local function read(key, missing_key, broad_key)
            local value = {count = 1, text = 'hello'}
            ---@type integer | string
            local result = value[key]
            ---@type {selected: 1 | 'hello'}
            local precise = {selected = value[key]}
            ---@type boolean
            local wrong = value[key] -- Error: both selected fields contain other types.
            ---@type integer | nil
            local optional = value[missing_key]
            ---@type integer
            local required = value[missing_key] -- Error: the missing alternative can return nil.
            ---@type integer | string | nil
            local broad = value[broad_key]
            ---@type nil
            local empty = value[broad_key] -- Error: broad_key can also select either data field.
            ---@type integer | string
            local present = value[broad_key] -- Error: a general string can select missing fields.
            value.count = 2
            ---@type {selected: 1 | 'hello'}
            local stale = {selected = value[key]} -- Error: count's initializer is no longer current.
            value.extra = 1

            local dynamic = {count = 1, text = 2}
            local sealed = dynamic
            dynamic[key] = 3
            ---@type {selected: 1 | 2}
            local stale = {selected = dynamic[key]} -- Error: a computed write can change either field.
            return result
        end

        ---@param key 1 | 'text'
        local function mixed_keys(key)
            local value = {[1] = true, text = 'hello'}
            ---@type boolean | string
            local result = value[key]
            return result
        end
        "#,
    );
}

#[test]
fn reassigned_computed_keys_keep_only_safe_declared_domains() {
    assert_contract(
        r#"
        ---@param key integer
        local function numeric_data(key)
            local value = {[1] = 'hello'}
            function value.read() return 'hello' end
            key = key + 1
            local text = value[key]
            value.extra = 1
            return value
        end

        ---@param key integer
        local function cast_key(key)
            local value = {[1] = 'hello'}
            function value.read() return 'hello' end
            ---@cast key 'read'
            local read = value[key]
            value.extra = 1 -- Error: the cast key can retrieve the function.
        end

        local value = {text = 'hello', read = function() return 'hello' end}
        ---@type string
        local key = 'text'
        key = 'read'
        local read = value[key]
        value.extra = 1 -- Error: assignment invalidates the exact initializer key.
        "#,
    );
}

#[test]
fn dynamic_construction_keeps_the_first_index_contract() {
    assert_contract(
        r#"
        ---@param key string
        local function build(key)
            local value = {}
            value[key] = 'hello'
            value[key] = 'updated'
            value[key] = 1 -- Error: the index signature is string.
            ---@type table<string, string>
            local record = value
            return record
        end
    "#,
    );
}

#[test]
fn construction_uses_function_fields_and_existing_index_contracts() {
    assert_contract(
        r#"
        local function read() return 'hello' end
        local value = {read = read}
        local callback = value.read
        value.extra = 1 -- Error: retrieving a function seals the table.

        ---@param callback fun(): string
        local function build(callback)
            local value = {read = callback}
            local read = value.read
            value.extra = 1 -- Error: annotated function values also seal.
            return value
        end

        ---@alias Callback fun(): string
        ---@param callback Callback
        local function build_alias(callback)
            local value = {read = callback}
            local read = value.read
            value.extra = 1 -- Error: a function alias has the same sealing rule.
            return value
        end

        local nil_field = {text = nil}
        local text = nil_field.text
        nil_field.text = 'hello' -- Error: nil is the established stable field type.

        local counted = {}
        local count = #counted
        counted.extra = 1 -- Error: a whole-table operation seals construction.

        ---@param key string
        local function dictionary(key)
            local value = {text = 'hello'}
            value[key] = 1 -- Error: the key can select the existing string field.
            return value
        end
    "#,
    );
}

#[test]
fn construction_keeps_generic_alias_arguments_in_the_callers_file() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    ws.def_file(
        "aliases.lua",
        r#"
        ---@alias (file) Callback string
        ---@alias Identity<T> T
    "#,
    );
    let failures = check_contract_in(
        &mut ws,
        r#"
        ---@alias (file) Callback fun(): string
        ---@param callback Identity<Callback>
        local function build(callback)
            local value = {read = callback}
            local read = value.read
            value.extra = 1 -- Error: Callback names the function type in this file.
            return value
        end
    "#,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn construction_preserves_lua_assignment_results_and_dynamic_presence() {
    assert_contract(
        r#"
        local function pair() return 'hello', 1 end
        local value = {}
        value.text, value.count = pair()
        ---@type string
        local text = value.text
        ---@type integer
        local count = value.count
        ---@type {text: string, count: integer}
        local record = value
        value.count = 'one' -- Error: count's first value was an integer.

        ---@param key string
        ---@param flag boolean
        local function build(key, flag)
            local value = {}
            if flag then value[key] = 'hello' else value[key] = 1 end
            value[key] = 'updated'
            value[key] = 2
            ---@type table<string, string | integer>
            local record = value
            ---@type string | integer | nil
            local maybe = value.other
            ---@type string | integer
            local required = value.other -- Error: an index signature does not guarantee presence.
            return record
        end

        ---@param key string
        local function read(key)
            local value = {text = 'hello'}
            ---@type string | nil
            local text = value[key]
            value.count = 1 -- Reading a data field keeps construction open.
            return text
        end
    "#,
    );
}

#[test]
fn unused_early_exits_preserve_the_returned_table_shape() {
    assert_contract(
        r#"
        ---@param stop boolean
        local function build(stop)
            local value = {}
            if stop then
                local alias = value
                return nil
            end
            value.text = 'hello'
            return value
        end
        local value = build(false)
        if value then
            ---@type {text: string}
            local record = value
        end
        "#,
    );
}

#[test]
fn returning_a_local_alias_preserves_the_returned_table_shape() {
    assert_contract(
        r#"
        ---@param stop boolean
        local function build(stop)
            local value = {}
            if stop then
                local unused = value
                return nil
            end
            value.text = 'hello'
            local alias = value
            local result = (alias)
            return result
        end
        local value = build(false)
        if value then
            ---@type {text: string}
            local record = value
        end
        "#,
    );
}

#[test]
fn returned_table_shapes_keep_other_table_returns_and_deferred_bodies() {
    assert_contract(
        r#"
        ---@param stop boolean
        local function build(stop)
            local value = {}
            function value.read()
                ---@type string
                local text = value.text -- Error: an earlier sealing path has no text.
                return text
            end
            if stop then
                local alias = value
                return nil
            end
            value.text = 'hello'
            return value
        end
        local value = build(false)
        if value then
            ---@type {text: string}
            local record = value
        end

        ---@param stop boolean
        local function maybe_text(stop)
            local value = {}
            if stop then return value end
            value.text = 'hello'
            return value
        end
        ---@type {text?: string}
        local optional = maybe_text(false)
        ---@type {text: string}
        local required = maybe_text(false) -- Error: another path returns the empty table.
        "#,
    );
}

#[test]
fn tuple_writes_check_the_declared_slot_type() {
    assert_contract(
        r#"
        ---@type [integer, string]
        local value = {1, 'hello'}
        value[1] = 2
        value[2] = 'updated'
        value[1] = 'bad' -- Error: the first slot accepts integers.
        value[2] = 3 -- Error: the second slot accepts strings.
        value[1] = nil
        "#,
    );
}

#[test]
fn field_reads_follow_writes_and_rebinding() {
    assert_contract(
        r#"
        ---@type {text?: string}
        local value = {}
        value.text = 'hello'
        ---@type 'hello'
        local first = value.text
        value.text = 'updated'
        ---@type 'updated'
        local second = value.text
        value = {}
        ---@type string
        local stale = value.text -- Error: rebinding discards the old field value.
        "#,
    );
}

#[test]
fn assignment_rhs_reads_fields_before_any_target_is_written() {
    assert_contract(
        r#"
        local value = {a = 1, b = 0}
        value.a, value.b = 2, value.a
        ---@type 1
        local before = value.b
        ---@type 2
        local after = value.a
        ---@type 2
        local wrong = value.b -- Error: the RHS reads a before it becomes 2.

        value.a = 3
        value.a, value.b = 4, value.a
        ---@type 3
        local previous = value.b
        ---@type 4
        local current = value.a
        "#,
    );
}

#[test]
fn uncalled_functions_do_not_change_field_flow() {
    assert_contract(
        r#"
        local value = {text = 'hello'}
        value.text = 'current'
        function value.change()
            value.text = 'updated'
        end
        ---@type 'current'
        local current = value.text
        ---@type 'updated'
        local wrong = value.text -- Error: change has not run.
        value.text = 'after'
        ---@type 'after'
        local after = value.text
        "#,
    );
}

#[test]
fn field_writes_use_stable_types_and_casts() {
    assert_contract(
        r#"
        ---@class MixedRecord
        ---@field value string | integer
        ---@type MixedRecord
        local record
        if record.value == 'ok' then
            record.value = 1
            ---@type 1
            local current = record.value
        end

        ---@class Box<T>
        ---@field value T
        ---@type Box<string>
        local box
        box.value = 'hello'
        box.value = 1 -- Error: the stable field contract includes generic substitution.

        ---@type {text?: string}
        local casted = {}
        ---@cast casted.text 'ok'
        casted.text = 'ok'
        casted.text = 'bad' -- Error: the cast governs later writes too.

        local constructed = {}
        ---@type 'ok'
        constructed.text = 'bad' -- Error: the first write must fit its field annotation.
        "#,
    );
}
