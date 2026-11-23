//! Regression test for module members declared outside the module table (#1420).
//!
//! The JSON generator now uses the shared module member projection, so runtime
//! members (`function m.foo`, `m.bar = ...`) are exported alongside members of
//! the exported table.

mod common;

const RUNTIME_SOURCE: &str = r#"---@meta

local m = {}

---Return a value.
---@return table
function m.foo() end

---Return another value.
---@return table
m.bar = function() end

return m
"#;

const LITERAL_SOURCE: &str = r#"---@meta

local m = {
    ---Return a value.
    ---@return table
    foo = function() end,

    ---Return another value.
    ---@return table
    bar = function() end,
}

return m
"#;

const INLINE_LITERAL_SOURCE: &str = r#"---@meta

return {
    ---Return a value.
    ---@return table
    foo = function() end,

    ---Return another value.
    ---@return table
    bar = function() end,
}
"#;

const MIXED_DEDUPE_SOURCE: &str = r#"---@meta

local m = {}

---Return a value.
---@return table
function m.foo() end

---A duplicate declaration of foo must not be exported twice.
---@return table
m.foo = function() end

---Return another value.
---@return table
m.bar = function() end

return m
"#;

/// A class export keeps its rendered `typ` while also exporting members.
const CLASS_SOURCE: &str = r#"---@class ModClass
local m = {}

---Return a value.
---@return table
function m.foo() end

return m
"#;

fn module(fixture_name: &str, source: &str) -> serde_json::Value {
    let json = common::generate_json_fixture(fixture_name, source);
    assert_eq!(json["modules"].as_array().map(Vec::len), Some(1), "{json}");
    json["modules"][0].clone()
}

fn assert_module_methods(module: &serde_json::Value) {
    assert_eq!(module["name"], "mod");
    assert!(module["typ"].is_null(), "unexpected module type: {module}");
    assert_eq!(
        common::json_member_names_sorted(module),
        ["bar", "foo"],
        "unexpected module members: {module}"
    );
}

#[test]
fn json_exports_module_members_declared_after_table() {
    let module = module("json_modules_after_table", RUNTIME_SOURCE);
    assert_module_methods(&module);
}

#[test]
fn json_exports_module_table_literal_members() {
    let module = module("json_modules_literal", LITERAL_SOURCE);
    assert_module_methods(&module);
}

#[test]
fn json_exports_inline_literal_module_members() {
    let module = module("json_modules_inline_literal", INLINE_LITERAL_SOURCE);
    assert_module_methods(&module);
}

#[test]
fn json_dedupes_module_members_declared_twice() {
    let module = module("json_modules_mixed_dedupe", MIXED_DEDUPE_SOURCE);
    assert_module_methods(&module);
}

#[test]
fn json_exports_type_and_members_for_class_module() {
    let module = module("json_modules_class_export", CLASS_SOURCE);

    // `typ` and `members` are independent: a class export renders its type and
    // still exports runtime members declared after the module table.
    assert_eq!(
        module["typ"].as_str(),
        Some("ModClass"),
        "unexpected module type: {module}"
    );
    assert_eq!(
        common::json_member_names_sorted(&module),
        ["foo"],
        "unexpected module members: {module}"
    );
}
