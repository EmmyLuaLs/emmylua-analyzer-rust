//! Regression test for member entries in the HTML search index (#1475).
//!
//! The search index used to contain only page-level entries (types, modules
//! and globals), so searching for a documented method or field returned
//! nothing even though the member has a stable anchor on its owner's page.

mod common;

const MODULE_SOURCE: &str = r#"---@meta

local m = {}

---Return a value.
---@return table
function m.new() end

return m
"#;

const CLASS_SOURCE: &str = r#"---@class Widget
local Widget = {}

---Do a thing.
function Widget:run() end

Widget.size = 1

return Widget
"#;

fn find_entry<'a>(entries: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    entries
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap_or_else(|| panic!("missing search entry {name}: {entries:?}"))
}

#[test]
fn search_index_contains_module_methods() {
    let out = common::generate_html_fixture("html_search_index_module", MODULE_SOURCE);
    let entries = common::read_search_index(&out);

    let page = find_entry(&entries, "mod");
    assert_eq!(page["href"], "module/mod.html");
    assert_eq!(page["kind"], "module");

    let member = find_entry(&entries, "mod.new");
    assert_eq!(member["href"], "module/mod.html#mod.new");
    assert_eq!(member["kind"], "method");
}

#[test]
fn search_index_contains_class_methods_and_fields() {
    let out = common::generate_html_fixture("html_search_index_class", CLASS_SOURCE);
    let entries = common::read_search_index(&out);

    let page = find_entry(&entries, "class Widget");
    assert_eq!(page["href"], "class/Widget.html");
    assert_eq!(page["kind"], "class");

    let method = find_entry(&entries, "Widget.run");
    assert_eq!(method["href"], "class/Widget.html#Widget.run");
    assert_eq!(method["kind"], "method");

    let field = find_entry(&entries, "Widget.size");
    assert_eq!(field["href"], "class/Widget.html#Widget.size");
    assert_eq!(field["kind"], "field");
}
