use crate::{DiagnosticCode, LuaType, test_lib::VirtualWorkspace};
use lsp_types::NumberOrString;
use tokio_util::sync::CancellationToken;

fn check_contract(code: &str) -> Vec<String> {
    check_contract_in(&mut VirtualWorkspace::new_with_init_std_lib(), code)
}

fn check_contract_in(ws: &mut VirtualWorkspace, code: &str) -> Vec<String> {
    let mut failures = Vec::new();
    ws.enable_check(DiagnosticCode::NeedCheckNil);
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
                DiagnosticCode::NeedCheckNil,
                DiagnosticCode::InjectField,
            ]
            .iter()
            .any(|code| diagnostic.code == Some(NumberOrString::String(code.get_name().to_owned())))
        })
        .collect();
    for (line, text) in code.lines().enumerate() {
        let expected = text.contains("-- Error:") || text.contains("-- inject-field:");
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
    assert!(failures.is_empty(), "{code}\n{}", failures.join("\n"));
}

#[test]
#[ignore = "Known bug before ef507415: function assignments lose declared call contracts"]
fn declared_function_contracts_survive_all_call_forms() {
    for annotation in [
        "fun(value: number): number",
        "Callback",
        "Callback|nil",
        "TypedCallback<number>",
    ] {
        for definition in [
            "local accept = function(value) return value end",
            "local accept; accept = function(value) return value end",
            "local accept; function accept(value) return value end",
        ] {
            assert_contract(&format!(
                r#"
                ---@alias Callback fun(value: number): number
                ---@alias TypedCallback<T> fun(value: T): T
                ---@type {annotation}
                {definition}
                local alias = accept
                accept(1);
                (accept)(1);
                alias(1)
                pcall(accept, 1)
                accept('text'); -- Error: the declared parameter is a number.
                (accept)('text'); -- Error: parentheses preserve the contract.
                alias('text') -- Error: aliases preserve the contract.
                pcall(accept, 'text') -- Error: protected calls preserve the contract.
                ---@type string
                local text = alias(1) -- Error: the declared return is a number.
                "#,
            ));
        }
    }
}

#[test]
#[ignore = "Known bug before ef507415: function assignments lose declared call contracts"]
fn implementation_docs_do_not_replace_declared_call_returns() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    ws.def(
        r#"
        ---@type fun(value: number): number
        ---@param value number
        local make = function(value) return 'text' end
        DirectResult = make(1)
        local alias = make
        AliasResult = alias(1)
        local _, result = pcall(make, 1)
        ProtectedResult = result
        "#,
    );
    for result in ["DirectResult", "AliasResult"] {
        assert_eq!(ws.expr_ty(result), LuaType::Number, "{result}");
    }
    // A protected call can also return the error message.
    let expected = ws.ty("number|string");
    assert_eq!(ws.expr_ty("ProtectedResult"), expected);
}

#[test]
fn function_assignments_keep_all_declared_overloads() {
    assert_contract(
        r#"
        ---@type (fun(value: number): number) | (fun(value: string): string)
        local accept
        function accept(value) return value end
        local alias = accept
        alias(1)
        alias('text')
        pcall(accept, 'text')
        alias(false) -- Error: neither overload accepts a boolean.
        pcall(accept, false) -- Error: neither overload accepts a boolean.
        ---@type string
        local text = alias(1) -- Error: this overload returns a number.
        "#,
    );
}

#[test]
#[ignore = "Known bug before ef507415: function assignments lose declared call contracts"]
fn nullable_function_contracts_still_follow_nil_assignments() {
    assert_contract(
        r#"
        ---@type (fun(value: number): number)?
        local accept
        function accept(value) return value end
        local alias = accept
        accept = nil
        alias(1)
        alias('text') -- Error: a saved function keeps its contract.
        accept(1) -- Error: the current value is nil.
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
fn field_assertions_do_not_disable_inferred_table() {
    assert_contract(
        r#"
        local value = {count = 1 --[[@as integer]]}
        ---@type {text: string}
        local record = value -- The completed interface includes the later definition.
        value.text = 'late' -- An earlier use does not restrict definitions.

        local built = {text = 'hello' --[[@as string]]}
        built.count = 1
        ---@type {text: string, count: integer}
        local record = built
        built.extra = 1
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

/// Run the specification examples through the public diagnostic path.
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

#[test]
fn branches_and_loops_preserve_stable_field_types() {
    assert_contract(
        r#"
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
fn declared_never_calls_do_not_reach_field_joins() {
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

            local shared = {}
            shared[text_key] = 'hello'
            local record = shared
            shared[mixed_key] = 'updated' -- The original binding can define integer keys.
            return compatible
        end
        "#,
    );
}

#[test]
fn aliases_require_coverage_of_the_whole_key() {
    assert_contract(
        r#"
        ---@param key 'text' | 'extra'
        local function named(key)
            local value = {text = 'hello'}
            local shared = value
            shared[key] = 'updated' -- inject-field: an alias cannot add extra.
            return value
        end

        ---@param text_key string
        ---@param count_key integer
        ---@param key string | integer
        local function indexed(text_key, count_key, key)
            local value = {}
            value[text_key] = 'hello'
            value[count_key] = 'hello'
            local shared = value
            shared[key] = 'updated'
            return value
        end
        "#,
    );
}

#[test]
fn function_fields_use_the_completed_interface() {
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
        module.extra = 1

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
        ---@param index integer
        local function read(key, missing_key, broad_key, index)
            local value = {count = 1, text = 'hello'}
            value[index] = function() return 'hello' end
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
            local shared = dynamic
            dynamic[key] = 3
            ---@type {selected: 1 | 2}
            local stale = {selected = dynamic[key]} -- Error: a computed write can change either field.
            return result
        end

        ---@param key 1 | 'text'
        local function mixed_keys(key)
            local value = {[1] = true, text = 'hello'}
            function value.read() return value.text end
            local first_key = 'text'
            local field_key = (first_key)
            local text = value[field_key]
            ---@type boolean | string
            local result = value[key]
            value.extra = 1
            return result
        end
        "#,
    );
}

#[test]
fn dynamic_definitions_keep_the_first_index_contract() {
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
fn field_definitions_preserve_assignment_results_and_dynamic_presence() {
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
    "#,
    );
}

#[test]
fn returned_tables_and_aliases_share_completed_interfaces() {
    assert_contract(
        r#"
        ---@param stop boolean
        local function build(stop)
            local value = {}
            function value.read()
                ---@type string
                local text = value.text -- Error: another normal exit has no text.
                return text
            end
            if stop then
                local alias = value
                return
            end
            value.text = 'hello'
            ---@type {text: string}
            local required = value -- Error: the completed interface includes the early exit.
            local alias = value
            local result = (alias)
            return result
        end
        local value = build(false)
        if value then
            ---@type {text: string}
            local record = value -- Error: the completed interface includes the early exit.
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

#[test]
fn nil_placeholders_establish_one_contract_on_each_path() {
    assert_contract(
        r#"
        local unrelated = 'hello'
        ---@cast unrelated string

        ---@return integer, string
        local function pair() return 1, 'hello' end
        local number, text
        number, text = pair()
        number = 'one' -- Error: the first result established integer | nil.
        text = 2       -- Error: the second result established string | nil.
        text = 'world'

        local first, second
        first = 1
        second = 'hello'
        first = 'one' -- Error: first has stable type integer | nil.
        second = 2    -- Error: second has stable type string | nil.

        local value
        if flag then
            value = 1
            value = 'one' -- Error: a later branch cannot widen this path.
        else
            value = 'hello'
        end
        value = 2
        value = 'world'
        value = nil
        value = true -- Error: nil did not restart inference.

        local record = {text = nil}
        record.text = 'hello'
        record.text = nil
        record.text = 1 -- Error: text has stable type string | nil.
        ---@type nil
        local empty
        empty = 1 -- Error: an explicit nil type is not a placeholder.
    "#,
    );
}

#[test]
fn field_refinements_do_not_change_whole_table_checks() {
    for (setup, reference, write) in [
        (
            "local value = {}; if flag then value.text = 'hello' end",
            "value",
            "if not value.text then value.text = 'hello' end",
        ),
        (
            "local value = {text = nil}; value.text = 'hello'",
            "alias",
            "alias.text = 'world'",
        ),
    ] {
        assert_contract(&format!(
            r#"
            {setup}
            local alias = value
            ---@type {{text: string}}
            local early = {reference} -- Error: the stable field type includes nil.
            if {reference}.text then
                ---@type string
                local text = {reference}.text
                ---@type {{text: string}}
                local guarded = {reference} -- Error: the stable field type still includes nil.
            end
            {write}
            ---@type {{text: string}}
            local assigned = {reference} -- Error: initialization does not change the stable type.
            ---@cast alias.text string | nil
            value.text:upper() -- The alias cast does not affect value's flow facts.
            "#
        ));
    }
}

#[test]
fn only_original_bindings_define_inferred_members() {
    assert_contract(
        r#"
        local value = {child = {text = 'hello'}}
        local alias = value
        alias.only_alias = 1 -- inject-field: aliases cannot define members.
        value.count = 1 -- The original binding can define count later.
        alias.count = 2
        value.child.extra = 1 -- inject-field: a nested path cannot define fields.
        function value.read()
            value.captured = 1 -- inject-field: a captured body cannot define fields.
            return value.count
        end
        value.extra = true -- Reads and function fields do not close the interface.
    "#,
    );
}

#[test]
fn rebinding_does_not_add_replacement_members_to_old_aliases() {
    assert_contract(
        r#"
        local value = {text = 'hello'}
        local alias = value
        value.count = 1
        value = {text = 'world', count = 2}
        value.extra = true
        alias.text = 'updated'
        alias.count = 3
        local extra = alias.extra -- Error: extra belongs to the replacement table.
    "#,
    );
}

#[test]
fn labels_resume_flow_and_join_only_reaching_definitions() {
    assert_contract(
        r#"
        local value = {}
        goto define
        value.text = 1 -- Unreachable: this does not establish a contract.
        ::define::
        value.text = 'hello'
        ::again::
        if retry then
            value.text = 'world'
            goto again
        end
        if false then value.text = true end -- Unreachable first definitions do not join.
        ---@type {text: string}
        local record = value
        value.text = 1 -- Error: the stable type is string.
    "#,
    );
}

#[test]
fn captured_reads_and_aliases_share_the_stable_contract() {
    assert_contract(
        r#"
        local module = {text = nil}
        if flag then module.text = 'early' end
        local function reset()
            module = {text = 'reset', read = module.read}
        end
        module.text = 'hello'
        function module.read()
            return module.text:upper() -- Error: the stable text type includes nil.
        end
        local alias = module
        alias.text:upper() -- Error: alias creation does not copy presence.
        module.read()
    "#,
    );
}

#[test]
fn whole_table_checks_allow_later_field_definitions() {
    assert_contract(
        r#"
        ---@param value {text: string}
        local function accept(value) return value end
        ---@return {text: string}
        local function build()
            local value = {}
            ---@type {text: string}
            local alias = value
            accept(value)
            pcall(accept, value)
            ---@type string
            local early = value.text -- Error: this field read precedes initialization.
            value.text = 'hello'
            return alias
        end
        "#,
    );
}

#[test]
fn whole_table_checks_include_other_definition_branches() {
    assert_contract(
        r#"
        ---@param flag boolean
        local function build(flag)
            local value = {}
            if flag then
                value.result = 1
                ---@type integer
                local number = value.result
                ---@type {result: integer}
                local alias = value -- Error: the completed field type includes string.
            else
                value.result = 'hello'
            end
            return value
        end
        "#,
    );
}
