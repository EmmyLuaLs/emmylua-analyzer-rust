#![cfg(test)]
use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

fn check_inherited_write_projection(
    declarations: &str,
    receiver: &str,
    read_type: &str,
    write_type: &str,
    valid_value: &str,
) {
    let mut ws = VirtualWorkspace::new();
    ws.def(&format!(
        "{declarations}\n---@type {receiver}\nchild = {{}}"
    ));
    ws.def(
        "read_value = child.value\nchild.callback = function(value)\ncallback_value = value\nend",
    );
    assert_eq!(ws.expr_ty("read_value"), ws.ty(read_type));
    assert_eq!(ws.expr_ty("callback_value"), ws.ty(write_type));
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        &format!("child.value = {valid_value}")
    ));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "child.value = true"));
}

#[test]
fn asymmetric_fields_inherited_generic_ref() {
    check_inherited_write_projection(
        r#"
---@class Base<T>
---@field (read) value T
---@field (write) value T
---@field (read) callback fun(value: T)
---@field (write) callback fun(value: T)
---@class Child: Base<string>
"#,
        "Child",
        "string",
        "string",
        "'valid'",
    );
}

#[test]
fn asymmetric_fields_inherited_generic_mapped() {
    check_inherited_write_projection(
        r#"
---@class Base<T>
---@field (read) value T
---@field (write) value T
---@field (read) callback fun(value: T)
---@field (write) callback fun(value: T)
---@class Child<U>: Base<U[]>
"#,
        "Child<number>",
        "number[]",
        "number[]",
        "{1, 2}",
    );
}

#[test]
fn asymmetric_fields_inherited_generic_distinct_directions() {
    check_inherited_write_projection(
        r#"
---@class Base<R, W>
---@field (read) value R
---@field (write) value W
---@field (read) callback boolean
---@field (write) callback fun(value: W)
---@class Child<U>: Base<number, U[]>
"#,
        "Child<string>",
        "number",
        "string[]",
        "{'valid'}",
    );
}

const DEFS: &str = r#"
---@meta
---@class Fuel
---@field name string
---@class Burner
---@field (read) fuel? Fuel
---@field (write) fuel? string|Fuel
---@field (read) force Fuel
---@field (write) force string|Fuel
---@field heat number
---@class Child: Burner
---@class Box<T, W>
---@field (read) value T
---@field (write) value W
---@type Burner
burner = {}
---@type Child
child = {}
---@type Box<Fuel, string>
box = {}
"#;

#[test]
fn asymmetric_fields_partial_override_replaces_both_directions() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
---@class Parent
---@field (read) value number
---@field (write) value string
---@class Child: Parent
---@field (read) value boolean
---@type Parent
parent = {}
---@type Child
child = {}
"#,
    );
    assert_eq!(ws.expr_ty("parent.value"), LuaType::Number);
    assert_eq!(ws.expr_ty("child.value"), LuaType::Boolean);
    assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "parent.value = 'valid'"));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "parent.value = true"));
    assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "child.value = true"));
    assert!(!ws.has_no_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        "child.value = 'invalid'"
    ));
}

#[test]
fn asymmetric_fields_read_and_write() {
    let mut ws = VirtualWorkspace::new();
    ws.def(DEFS);
    assert_eq!(ws.expr_ty("burner.force"), ws.ty("Fuel"));
    assert_eq!(ws.expr_ty("child.force"), ws.ty("Fuel"));
    assert_eq!(ws.expr_ty("box.value"), ws.ty("Fuel"));
    for source in [
        "burner.fuel = 'coal'",
        "burner['fuel'] = 'coal'",
        "burner.fuel = nil",
        "child.force = 'player'",
        "box.value = 'coal'",
        "burner.fuel, burner.heat = 'coal', 10",
    ] {
        assert!(
            ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, source),
            "{source}"
        );
    }
    for source in [
        "burner.fuel = true",
        "burner.force = nil",
        "burner.force = true",
        "child.force = false",
        "box.value = 42",
        "burner.heat = 'bad'",
    ] {
        assert!(
            !ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, source),
            "{source}"
        );
    }
}

#[test]
fn asymmetric_fields_flow_and_nested_writes() {
    let mut ws = VirtualWorkspace::new();
    ws.def(DEFS);
    ws.def(
        r#"
        burner.fuel = 'coal'
        after_write = burner.fuel
        if burner.fuel then
            guarded = burner.fuel.name
            burner.fuel = nil
            after_nil = burner.fuel
        end
        box.value = 'coal'
        generic_after = box.value
    "#,
    );
    assert_eq!(ws.expr_ty("after_write"), ws.ty("Fuel?"));
    assert_eq!(ws.expr_ty("guarded"), LuaType::String);
    assert_eq!(ws.expr_ty("after_nil"), ws.ty("Fuel?"));
    assert_eq!(ws.expr_ty("generic_after"), ws.ty("Fuel"));
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        "burner.force.name = 'ok'"
    ));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "burner.force.name = 3"));
}

#[test]
fn asymmetric_fields_duplicate_rules() {
    let mut ws = VirtualWorkspace::new();
    assert!(ws.has_no_diagnostic(DiagnosticCode::DuplicateDocField, DEFS));
    for fields in [
        "---@field (read) value string\n---@field (read) value number",
        "---@field (write) value string\n---@field (write) value number",
        "---@field value string\n---@field (write) value number",
    ] {
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::DuplicateDocField,
            &format!("---@class Duplicate\n{fields}\nlocal t = {{}}")
        ));
    }
}

#[test]
fn asymmetric_fields_reindex_and_incomplete_directions() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file("defs.lua", DEFS);
    assert_eq!(ws.expr_ty("burner.force"), ws.ty("Fuel"));
    ws.def_file(
        "defs.lua",
        &DEFS.replace("(read) force Fuel", "(read) force number"),
    );
    assert_eq!(ws.expr_ty("burner.force"), LuaType::Number);
    ws.def("---@class Only\n---@field (write) value string\n---@type Only\nonly = {}");
    assert_eq!(ws.expr_ty("only.value"), LuaType::String);
    assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "only.value = 'yes'"));
}

#[test]
fn asymmetric_fields_contextual_table_assignment() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
---@class Input
---@field name string
---@class Output
---@field result number
---@class Container
---@field (read) value Output
---@field (write) value Input
---@type Container
container = {}
"#,
    );
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        "container.value = { name = 'coal' }"
    ));
    assert!(!ws.has_no_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        "container.value = { name = 3 }"
    ));
}

#[test]
fn asymmetric_fields_keys_and_independent_optionality() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
---@meta
---@class Keyed
---@field (read) ['odd-key'] number
---@field (write) ['odd-key']? string
---@field (read) [1] number
---@field (write) [1] string
---@type Keyed
keyed = {}
"#,
    );
    assert_eq!(ws.expr_ty("keyed['odd-key']"), LuaType::Number);
    assert_eq!(ws.expr_ty("keyed[1]"), LuaType::Number);
    assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "keyed['odd-key'] = nil"));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "keyed[1] = nil"));
    assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "keyed[1] = 'ok'"));
    for fields in [
        "---@field (read,write) value number",
        "---@field (read) [string] number",
    ] {
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AnnotationUsageError,
            &format!("---@class Bad\n{fields}")
        ));
    }
}

#[test]
fn asymmetric_fields_callback_context() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
---@class CallbackHolder
---@field (read) callback fun(value: number)
---@field (write) callback fun(value: string)
---@type CallbackHolder
holder = {}
holder.callback = function(value)
    callback_value = value
end
"#,
    );
    assert_eq!(ws.expr_ty("callback_value"), LuaType::String);
}

#[test]
fn asymmetric_fields_write_first_and_branch_invalidation() {
    let mut ws = VirtualWorkspace::new();
    ws.def("---@class Converted\n---@field (write) value string\n---@field (read) value number?\n---@type Converted\na = {}\n---@type Converted\nb = {}\n");
    assert_eq!(ws.expr_ty("a.value"), ws.ty("number?"));
    ws.def(
        r#"
        if a.value then
            before = a.value
            a.value = 'reset'
            after = a.value
        end
        if b.value then
            independent = a.value
        end
        if a.value then
            if ... then a.value = 'reset' end
            merged = a.value
        end
    "#,
    );
    assert_eq!(ws.expr_ty("before"), LuaType::Number);
    assert_eq!(ws.expr_ty("after"), ws.ty("number?"));
    assert_eq!(ws.expr_ty("independent"), ws.ty("number?"));
    assert_eq!(ws.expr_ty("merged"), ws.ty("number?"));
}

// Directional member flow must retain upstream expression-local @as behavior.
#[googletest::gtest]
fn asymmetric_fields_keep_inline_assertions_expression_local() {
    use googletest::prelude::*;
    let mut ws = VirtualWorkspace::new();
    ws.def(DEFS);
    ws.def(
        r#"
        local unrelated = 123 --[[@as string]]
        before_assertion = burner.force
        explicit_assertion = burner.force --[[@as integer]]
        after_assertion = burner.force
        optional_after_assertion = burner.fuel
    "#,
    );
    let fuel = ws.ty("Fuel");
    let optional_fuel = ws.ty("Fuel?");
    assert_that!(ws.expr_ty("before_assertion"), eq(&fuel));
    assert_that!(ws.expr_ty("explicit_assertion"), eq(&LuaType::Integer));
    assert_that!(ws.expr_ty("after_assertion"), eq(&fuel));
    assert_that!(ws.expr_ty("optional_after_assertion"), eq(&optional_fuel));
}

#[test]
fn asymmetric_fields_review_repeated_direction_flags() {
    for flags in ["read,read", "write,write", "read,write", "write,read"] {
        let mut ws = VirtualWorkspace::new();
        assert!(
            !ws.has_no_diagnostic(
                DiagnosticCode::AnnotationUsageError,
                &format!("---@class Invalid\n---@field ({flags}) value number"),
            ),
            "{flags}"
        );
    }
}

#[test]
fn asymmetric_fields_review_nullable_contracts() {
    for typ in ["Fuel", "Fuel[]", "fun(value: string)"] {
        let mut ws = VirtualWorkspace::new();
        ws.def(&format!(
            "---@class Fuel\n---@field name string\n---@class Property\n---@field (read) value? {typ}\n---@field (write) value? {typ}\n---@type Property\nproperty = {{}}",
        ));
        assert_eq!(
            ws.expr_ty("property.value"),
            ws.ty(&format!("({typ})?")),
            "{typ}"
        );
        assert!(
            ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "property.value = nil"),
            "{typ}"
        );
        assert!(
            !ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "property.value = true"),
            "{typ}"
        );
    }
}

#[test]
fn asymmetric_fields_review_branch_guard_merge() {
    let mut ws = VirtualWorkspace::new();
    ws.def(DEFS);
    ws.def(
        r#"
        if burner.fuel then
            if ... then
                branch = 1
            else
                branch = 2
            end
            shared_predecessor = burner.fuel
            if ... then burner.fuel = 'coal' end
            after_possible_write = burner.fuel
        end
    "#,
    );
    assert_eq!(ws.expr_ty("shared_predecessor"), ws.ty("Fuel"));
    assert_eq!(ws.expr_ty("after_possible_write"), ws.ty("Fuel?"));
}

#[test]
fn asymmetric_fields_review_repeated_callback_writer_context() {
    for fields in [
        "---@field callback fun(value: string)\n---@field callback fun(value: number)",
        "---@field (read) callback fun(value: boolean)\n---@field (write) callback fun(value: string)\n---@field (write) callback fun(value: number)",
        "---@field (write) callback fun(value: string)\n---@field (read) callback fun(value: boolean)\n---@field (write) callback fun(value: number)",
        "---@field (write) callback fun(value: string)\n---@field (write) callback fun(value: number)\n---@field (read) callback fun(value: boolean)",
        "---@field (write) callback fun(value: string)\n---@field (write) callback fun(value: number)",
    ] {
        let mut ws = VirtualWorkspace::new();
        ws.def(&format!(
            "---@class Holder\n{fields}\n---@type Holder\nholder = {{}}\nholder.callback = function(value)\n    callback_value = value\nend",
        ));
        assert_eq!(ws.expr_ty("callback_value"), LuaType::Number, "{fields}");
    }
}

#[test]
fn asymmetric_fields_review_repeated_callback_writer_generics() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
        ---@class Holder<T>
        ---@field (read) callback fun(value: boolean)
        ---@field (write) callback fun(value: string)
        ---@field (write) callback fun(value: T)
        ---@class Unrelated
        ---@field (write) callback fun(value: table)
        ---@type Holder<number>
        holder = {}
        holder.callback = function(value)
            callback_value = value
        end
    "#,
    );
    assert_eq!(ws.expr_ty("callback_value"), LuaType::Number);
    assert_eq!(ws.expr_ty("holder.callback"), ws.ty("fun(value: boolean)"));
}

#[test]
fn asymmetric_fields_review_member_collection_preserves_real_overloads() {
    for fields in [
        "---@field (read) value number\n---@field (write) value string",
        "---@field (write) value string\n---@field (read) value number",
    ] {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def_file("pair.lua", &format!("---@class Pair\n{fields}"));
        let model = ws.analysis.semantic_model(file);
        let ty = LuaType::Ref(crate::LuaTypeDeclId::global("Pair"));
        let infos =
            model.member_infos_with_key_all(&ty, &crate::LuaMemberKey::Name("value".into()));
        assert_eq!(infos.len(), 1, "{fields}: {infos:?}");
        assert_eq!(infos[0].typ, LuaType::Number);
    }
    for flag in ["", "(write) "] {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def_file("overloads.lua", &format!(
            "---@class Overloads\n---@field {flag}callback fun(value: string)\n---@field {flag}callback fun(value: number)"
        ));
        let model = ws.analysis.semantic_model(file);
        let ty = LuaType::Ref(crate::LuaTypeDeclId::global("Overloads"));
        let infos =
            model.member_infos_with_key_all(&ty, &crate::LuaMemberKey::Name("callback".into()));
        assert_eq!(infos.len(), 2, "{flag}: {infos:?}");
        assert_ne!(infos[0].id, infos[1].id);
    }
}

#[test]
fn asymmetric_fields_review_repeated_direction_messages() {
    for (flags, expected) in [
        ("read,read", "Duplicate (read) field modifier"),
        ("write,write", "Duplicate (write) field modifier"),
        (
            "read,write",
            "A field may specify only one of (read) or (write)",
        ),
        (
            "write,read",
            "A field may specify only one of (read) or (write)",
        ),
    ] {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def_file(
            "invalid.lua",
            &format!("---@class Invalid\n---@field ({flags}) value number"),
        );
        let model = ws.analysis.semantic_model(file);
        let errors = &model.file_facts().unwrap().annotation_errors;
        assert_eq!(errors.len(), 1, "{flags}: {errors:?}");
        assert_eq!(errors[0].message, expected, "{flags}");
        let document = model.document(file).unwrap();
        assert_eq!(
            document.get_text_slice(errors[0].range),
            flags.split(',').nth(1).unwrap(),
            "{flags}"
        );
    }
}

fn check_directional_guard_after_write(setup: &str, access: &str, action: &str, invalidates: bool) {
    let mut ws = VirtualWorkspace::new();
    ws.analysis
        .diagnostic
        .enable_only(DiagnosticCode::ParamTypeMismatch);
    let file = ws.def(&format!(
        r#"
---@class Converted
---@field (read) value number?
---@field (write) value string?
---@class Root
---@field left Converted
---@field right Converted
---@param value number
local function accept_number(value) end
local function harmless() end
---@param a Converted
---@param other Converted
---@param root Root
---@param other_root Root
---@param unknown fun(value: Converted)
local function check(a, other, root, other_root, unknown)
    {setup}
    if {access} then
        before_write = {access}
        {action}
        after_write = {access}
        accept_number({access})
    end
end
"#,
    ));
    assert_eq!(ws.expr_ty("before_write"), LuaType::Number, "{action}");
    let expected = if invalidates {
        ws.ty("number?")
    } else {
        LuaType::Number
    };
    assert_eq!(ws.expr_ty("after_write"), expected, "{action}");
    let diagnostics = ws
        .analysis
        .diagnose_file(file, tokio_util::sync::CancellationToken::new())
        .unwrap();
    assert_eq!(diagnostics.len(), usize::from(invalidates), "{action}");
}

#[test]
fn asymmetric_fields_review_receiver_replacement_resets_guard() {
    check_directional_guard_after_write("", "a.value", "a = other", true);
}

#[test]
fn asymmetric_fields_review_ancestor_replacement_resets_guard() {
    check_directional_guard_after_write("", "root.left.value", "root.left = other", true);
}

#[test]
fn asymmetric_fields_review_root_replacement_resets_guard() {
    check_directional_guard_after_write("", "root.left.value", "root = other_root", true);
}

#[test]
fn asymmetric_fields_review_alias_write_resets_guard() {
    check_directional_guard_after_write(
        "local alias = a",
        "a.value",
        "alias.value = 'changed'",
        true,
    );
    check_directional_guard_after_write(
        "local alias = root",
        "root.left.value",
        "alias.left = other",
        true,
    );
}

#[test]
fn asymmetric_fields_review_exact_write_restores_getter() {
    for action in ["a.value = nil", "a.value = 'changed'"] {
        check_directional_guard_after_write("", "a.value", action, true);
    }
}

#[test]
fn asymmetric_fields_review_unrelated_writes_and_calls_keep_guard() {
    for action in ["", "harmless()", "unknown(a)", "other.value = 'changed'"] {
        check_directional_guard_after_write("", "a.value", action, false);
    }
    check_directional_guard_after_write("", "root.left.value", "root.right = other", false);
    check_directional_guard_after_write("local alias = a", "a.value", "alias = other", false);
    check_directional_guard_after_write("local alias = a", "alias.value", "a = other", false);
}

#[test]
fn asymmetric_fields_review_new_guard_after_replacement() {
    check_directional_guard_after_write(
        "",
        "a.value",
        "a = other\nif a.value == nil then return end",
        false,
    );
}

#[test]
fn asymmetric_fields_review_new_guard_after_setter_write() {
    check_directional_guard_after_write(
        "",
        "a.value",
        "a.value = 'changed'\nif a.value == nil then return end",
        false,
    );
}

#[test]
fn asymmetric_fields_review_nullable_discriminant_keeps_receiver() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
---@class Absent
---@field (read) value nil
---@field (write) value string?
---@class Maybe
---@field (read) value number?
---@field (write) value string?
---@param a Absent|Maybe
local function check(a)
    if a.value == nil then
        matches_nil = a
    else
        may_have_number = a
        number_value = a.value
    end
end
"#,
    );
    assert_eq!(ws.expr_ty("matches_nil"), ws.ty("Absent|Maybe"));
    assert_eq!(ws.expr_ty("may_have_number"), ws.ty("Maybe"));
    assert_eq!(ws.expr_ty("number_value"), LuaType::Number);
}

#[test]
fn asymmetric_fields_review_ordinary_nullable_discriminant_keeps_receiver() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
---@class Absent
---@field value nil
---@class Maybe
---@field value number?
---@param a Absent|Maybe
local function check(a)
    if a.value == nil then
        matches_nil = a
    else
        may_have_number = a
    end
end
"#,
    );
    assert_eq!(ws.expr_ty("matches_nil"), ws.ty("Absent|Maybe"));
    assert_eq!(ws.expr_ty("may_have_number"), ws.ty("Maybe"));
}

#[test]
fn asymmetric_fields_review_alias_guard_tracks_receiver() {
    for (before_guard, after_guard, narrowed) in [
        ("", "", true),
        ("a = other", "", false),
        ("alias = other", "", false),
        ("", "a = other", false),
    ] {
        let mut ws = VirtualWorkspace::new();
        ws.def(&format!(
            r#"
---@class Converted
---@field (read) value number?
---@field (write) value string?
---@param a Converted
---@param other Converted
local function check(a, other)
    local alias = a
    {before_guard}
    if alias.value then
        {after_guard}
        guarded = a.value
    end
end
"#,
        ));
        let expected = if narrowed { "number" } else { "number?" };
        assert_eq!(
            ws.expr_ty("guarded"),
            ws.ty(expected),
            "{before_guard}; {after_guard}"
        );
    }
}

const UNION_DEFS: &str = r#"
---@class A
---@field kind 'a'
---@field (read) value boolean
---@field (write) value string
---@class B
---@field kind 'b'
---@field (read) value number
---@field (write) value number
---@class Generic<T>
---@field (read) value boolean
---@field (write) value T
---@class Ordinary
---@field value number
---@alias Choice A|B
---@alias GenericChoice<T> Generic<T>|B
"#;

#[test]
fn asymmetric_fields_review_union_setter_alternatives() {
    for receiver in ["A|B", "B|A", "Choice", "GenericChoice<string>"] {
        let mut ws = VirtualWorkspace::new();
        ws.def(&format!(
            "{UNION_DEFS}\n---@type {receiver}\nproperty = {{}}"
        ));
        assert_eq!(ws.expr_ty("property.value"), ws.ty("boolean|number"));
        for value in ["'accepted'", "42"] {
            assert!(
                ws.has_no_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    &format!("property.value = {value}"),
                ),
                "{receiver}: {value}"
            );
        }
        assert!(
            !ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "property.value = true"),
            "{receiver}"
        );
    }
}

#[test]
fn asymmetric_fields_review_union_keeps_ordinary_writer_alternatives() {
    for receiver in [
        "A|Ordinary",
        "Ordinary|A",
        "A|{value:number}",
        "{value:number}|A",
    ] {
        let mut ws = VirtualWorkspace::new();
        ws.def(&format!(
            "{UNION_DEFS}\n---@type {receiver}\nproperty = {{}}"
        ));
        assert_eq!(ws.expr_ty("property.value"), ws.ty("boolean|number"));
        for value in ["'accepted'", "42"] {
            assert!(
                ws.has_no_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    &format!("property.value = {value}"),
                ),
                "{receiver}: {value}"
            );
        }
        assert!(
            !ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "property.value = true"),
            "{receiver}"
        );
    }
}

#[test]
fn asymmetric_fields_review_union_keeps_distinct_generic_writers() {
    let mut ws = VirtualWorkspace::new();
    ws.def(&format!(
        "{UNION_DEFS}\n---@type Generic<string>|Generic<number>\nproperty = {{}}"
    ));
    assert_eq!(ws.expr_ty("property.value"), LuaType::Boolean);
    for value in ["'accepted'", "42"] {
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            &format!("property.value = {value}"),
        ));
    }
    assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "property.value = true"));
}

#[test]
fn asymmetric_fields_review_union_keeps_ordinary_lookup_policy() {
    for receiver in ["A|nil", "A|Missing", "Recursive"] {
        let mut ws = VirtualWorkspace::new();
        ws.def(&format!(
            "{UNION_DEFS}\n---@class Missing\n---@alias Recursive A|Recursive\n---@type {receiver}\nproperty = {{}}"
        ));
        assert_eq!(ws.expr_ty("property.value"), LuaType::Boolean);
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            "property.value = 'accepted'"
        ));
        assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "property.value = true"));
    }
}

#[test]
fn asymmetric_fields_review_union_narrowing_selects_setter() {
    for (kind, valid, invalid) in [("a", "'accepted'", "42"), ("b", "42", "'rejected'")] {
        let mut ws = VirtualWorkspace::new();
        ws.def(UNION_DEFS);
        let assignment = |value| {
            format!(
                "---@param property A|B\nlocal function check(property)\nif property.kind == '{kind}' then\nproperty.value = {value}\nend\nend"
            )
        };
        assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &assignment(valid)));
        assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &assignment(invalid)));
    }
}

#[test]
fn asymmetric_fields_review_invalid_annotation_retains_member() {
    for field in [
        "(read,read) value",
        "(write,write) value",
        "(read,write) value",
        "(write,read) value",
        "(read) [string]",
    ] {
        let mut ws = VirtualWorkspace::new();
        ws.analysis
            .diagnostic
            .enable_only(DiagnosticCode::UndefinedField);
        let source = |field: &str| {
            format!(
                "---@class Invalid\n---@field {field} number\n---@param target Invalid\nlocal function check(target)\nread_value = target.value\nend"
            )
        };
        let file = ws.def(&source(field));
        assert_eq!(
            ws.analysis
                .semantic_model(file)
                .file_facts()
                .unwrap()
                .annotation_errors
                .len(),
            1,
            "{field}"
        );
        let diagnostics = ws
            .analysis
            .diagnose_file(file, tokio_util::sync::CancellationToken::new())
            .unwrap();
        assert!(diagnostics.is_empty(), "{field}: {diagnostics:?}");
        let ordinary_field = field.split_once(' ').unwrap().1;
        let mut ordinary = VirtualWorkspace::new();
        ordinary.def(&source(ordinary_field));
        assert_eq!(
            ws.expr_ty("read_value"),
            ordinary.expr_ty("read_value"),
            "{field}"
        );
        if ordinary_field == "value" {
            assert!(!ws.has_no_diagnostic(
                DiagnosticCode::UndefinedField,
                "---@param target Invalid\nlocal function check(target)\nreturn target.missing\nend"
            ));
        }
    }
}

#[test]
fn asymmetric_fields_review_overloads_are_checked_per_direction() {
    for (single, repeated) in [("read", "write"), ("write", "read")] {
        let fields = [
            format!("---@field ({single}) value number"),
            format!("---@field ({repeated}) value fun(value: string)"),
            format!("---@field ({repeated}) value fun(value: boolean)"),
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut ws = VirtualWorkspace::new();
            let declarations = order.map(|i| fields[i].as_str()).join("\n");
            assert!(
                ws.has_no_diagnostic(
                    DiagnosticCode::DuplicateDocField,
                    &format!("---@class Converted\n{declarations}"),
                ),
                "{declarations}"
            );
        }
    }
}

#[test]
fn asymmetric_fields_review_invalid_duplicate_directions_still_report() {
    for declarations in [
        "---@field (read) value number\n---@field (read) value number\n---@field (write) value fun(value: string)",
        "---@field (write) value string\n---@field (write) value string\n---@field (read) value fun(value: number)",
        "---@field (read) value number\n---@field (write) value fun(value: string)\n---@field value fun(value: boolean)",
        "---@field value number\n---@field value number",
    ] {
        let mut ws = VirtualWorkspace::new();
        assert!(
            !ws.has_no_diagnostic(
                DiagnosticCode::DuplicateDocField,
                &format!("---@class Converted\n{declarations}"),
            ),
            "{declarations}"
        );
    }
}

#[test]
fn asymmetric_fields_review_callback_assignment_accepts_each_writer() {
    for (first, last) in [("string", "number"), ("number", "string")] {
        for read in ["---@field (read) callback fun(value: boolean)\n", ""] {
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                "---@class Holder\n{read}---@field (write) callback fun(value: {first})\n---@field (write) callback fun(value: {last})\n---@type Holder\nholder = {{}}"
            ));
            ws.def("holder.callback = function(value) callback_context = value end");
            assert_eq!(ws.expr_ty("callback_context"), ws.ty(last));
            assert!(
                ws.has_no_diagnostic(
                    DiagnosticCode::AssignTypeMismatch,
                    "holder.callback = function(value) callback_value = value end"
                ),
                "{first}/{last}: inferred callback"
            );
            for typ in [first, last] {
                assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch,
                    &format!("---@param value {typ}\nlocal function callback(value) end\nholder.callback = callback")), "{first}/{last}: {typ}");
            }
            assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch,
                "---@param value boolean\nlocal function callback(value) end\nholder.callback = callback"));
        }
    }
}

#[test]
fn asymmetric_fields_review_ordinary_callback_assignment_is_unchanged() {
    for (first, last) in [("string", "number"), ("number", "string")] {
        let mut ws = VirtualWorkspace::new();
        ws.def(&format!(
            "---@class Holder\n---@field callback fun(value: {first})\n---@field callback fun(value: {last})\n---@type Holder\nholder = {{}}"
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            "holder.callback = function(value) end"
        ));
        assert!(ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch,
            &format!("---@param value {first}\nlocal function callback(value) end\nholder.callback = callback")));
        assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch,
            &format!("---@param value {last}\nlocal function callback(value) end\nholder.callback = callback")));
    }
}

#[test]
fn asymmetric_fields_review_budget_preserves_completed_cast_paths() {
    for op in ["", "+"] {
        for length in [0, 600] {
            let long = (0..length)
                .map(|i| format!("noise = {i}\n"))
                .collect::<String>();
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                "---@class Box\n---@field (read) value number\n---@field (write) value number\n---@type Box\nlocal box = {{}}\nlocal noise = 0\nif flag then\n{long}else\n---@cast box.value {op}string\ncast_seen = box.value\nend\nresult = box.value"
            ));
            assert_eq!(
                ws.expr_ty("result"),
                ws.ty("number|string"),
                "{op}: {length}"
            );
        }
    }
}

#[test]
fn asymmetric_fields_review_lhs_assertion_keeps_setter_contract() {
    let mut ws = VirtualWorkspace::new();
    ws.def("---@class Box\n---@field (read) value number\n---@field (write) value string\n---@type Box\nbox = {}");
    for (value, accepted) in [("'valid'", true), ("false", false)] {
        let source = format!("box.value --[[@as boolean]] = {value}");
        assert!(ws.has_no_diagnostic(DiagnosticCode::SyntaxError, &source));
        assert_eq!(
            ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, &source),
            accepted
        );
    }
}

#[test]
fn asymmetric_fields_review_budget_preserves_partial_cast_paths() {
    for (op, expected) in [("", "string"), ("+", "number|string")] {
        for length in [0, 600] {
            let long = (0..length)
                .map(|i| format!("noise = {i}\n"))
                .collect::<String>();
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                "---@class Box\n---@field (read) value number\n---@field (write) value number\n---@type Box\nlocal box = {{}}\nlocal noise = 0\nif flag then\nnoise = 1\nelse\n{long}end\n---@cast box.value {op}string\nresult = box.value"
            ));
            assert_eq!(ws.expr_ty("result"), ws.ty(expected), "{op}: {length}");
        }
    }
}

#[test]
fn asymmetric_fields_index_signature_domains_remain_separate() {
    let fields = r#"
---@class Indexed<T>
---@field [integer] T
---@field [string] boolean
---@field (read) integer number
---@field (write) integer string
"#;
    let mut ws = VirtualWorkspace::new();
    assert!(ws.has_no_diagnostic(DiagnosticCode::DuplicateDocField, fields));
    assert!(!ws.has_no_diagnostic(
        DiagnosticCode::DuplicateDocField,
        &format!("{fields}\n---@field [integer] boolean")
    ));
    let mut ws = VirtualWorkspace::new();
    let file_id = ws.def(&format!(
        "{fields}\n---@type Indexed<string>\nindexed = {{}}"
    ));
    let indexed_type = ws.ty("Indexed<string>");
    let index_infos = ws
        .analysis
        .semantic_model(file_id)
        .index_infos(&indexed_type);
    assert_eq!(index_infos.len(), 2);
    assert!(
        index_infos
            .iter()
            .any(|info| info.key_type == LuaType::Integer && info.value_type == LuaType::String)
    );
    assert!(
        index_infos
            .iter()
            .any(|info| info.key_type == LuaType::String && info.value_type == LuaType::Boolean)
    );
    assert_eq!(ws.expr_ty("indexed.integer"), LuaType::Number);
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        "indexed.integer = 'accepted'"
    ));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "indexed.integer = 42"));
}

#[test]
fn asymmetric_fields_array_members_belong_to_elements() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
---@class Element
---@field (read) value number
---@field (write) value string
---@type Element[]
items = {}
"#,
    );
    assert_eq!(ws.expr_ty("items[1].value"), LuaType::Number);
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::UndefinedField,
        "local value = items[1].value"
    ));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::UndefinedField, "local value = items.value"));
    assert!(ws.has_no_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        "items[1].value = 'accepted'"
    ));
    assert!(!ws.has_no_diagnostic(DiagnosticCode::AssignTypeMismatch, "items[1].value = true"));
    ws.def("items[1].value = 'accepted'\nread_after = items[1].value");
    assert_eq!(ws.expr_ty("read_after"), LuaType::Number);
}
