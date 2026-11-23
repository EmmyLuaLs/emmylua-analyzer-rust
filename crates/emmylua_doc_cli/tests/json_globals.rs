//! Regression test for global table members.
//!
//! The JSON generator used to project only members attached to the global's
//! value type, so runtime members declared after the table
//! (`function g.foo()`, `g.bar = ...`) were dropped. It now uses the shared
//! global member projection.

mod common;

const RUNTIME_SOURCE: &str = r#"g = {}

---Return a value.
---@return table
function g.foo() end

---Return another value.
---@return table
g.bar = function() end
"#;

const LITERAL_SOURCE: &str = r#"g = {
    ---Return a value.
    ---@return table
    foo = function() end,

    ---Return another value.
    ---@return table
    bar = function() end,
}
"#;

const MIXED_DEDUPE_SOURCE: &str = r#"g = {}

---Return a value.
---@return table
function g.foo() end

---A duplicate declaration of foo must not be exported twice.
---@return table
g.foo = function() end

---Return another value.
---@return table
g.bar = function() end
"#;

fn global(fixture_name: &str, source: &str) -> serde_json::Value {
    let json = common::generate_json_fixture(fixture_name, source);
    let globals = json["globals"]
        .as_array()
        .unwrap_or_else(|| panic!("missing globals array: {json}"));
    let global = globals
        .iter()
        .find(|global| global["name"] == "g")
        .unwrap_or_else(|| panic!("global `g` was not exported: {json}"));
    // `type` is the serde tag of `Global::Table`
    // (`#[serde(tag = "type", rename_all = "snake_case")]`).
    assert_eq!(global["type"], "table", "unexpected global: {global}");
    global.clone()
}

fn assert_global_methods(global: &serde_json::Value) {
    assert_eq!(
        common::json_member_names_sorted(global),
        ["bar", "foo"],
        "unexpected global members: {global}"
    );
}

#[test]
fn json_exports_global_runtime_members() {
    let global = global("json_globals_runtime", RUNTIME_SOURCE);
    assert_global_methods(&global);
}

#[test]
fn json_exports_global_table_literal_members() {
    let global = global("json_globals_literal", LITERAL_SOURCE);
    assert_global_methods(&global);
}

#[test]
fn json_dedupes_global_members_declared_twice() {
    let global = global("json_globals_mixed_dedupe", MIXED_DEDUPE_SOURCE);
    assert_global_methods(&global);
}
