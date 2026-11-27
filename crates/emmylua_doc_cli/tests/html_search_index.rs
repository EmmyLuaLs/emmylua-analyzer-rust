//! Regression tests for member entries in the HTML search index (#1475).
//!
//! The search index used to contain only page-level entries (types, modules
//! and globals), so searching for a documented method or field returned
//! nothing even though the member has a stable anchor on its owner's page.

mod common;

use std::path::Path;

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

const GLOBAL_SOURCE: &str = r#"g = {}

---Return a value.
---@return table
function g.foo() end

g.bar = 1
"#;

const ENUM_SOURCE: &str = r#"---@enum Color
local Color = {
    RED = 1,
    GREEN = 2,
}
"#;

const PRIVATE_SOURCE: &str = r#"---@class Widget
local Widget = {}

---@private
function Widget:secret() end

---Do a thing.
function Widget:run() end

return Widget
"#;

const NAMESPACED_SOURCE: &str = r#"---@class lsp.Client
---@field name string
local Client = {}

---Do a thing.
function Client:run() end

return Client
"#;

const ALIAS_SOURCE: &str = r#"---@alias Handler fun(x: integer)
"#;

fn find_entries<'a>(entries: &'a [serde_json::Value], name: &str) -> Vec<&'a serde_json::Value> {
    entries
        .iter()
        .filter(|entry| entry["name"] == name)
        .collect()
}

fn find_entry<'a>(entries: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    let matches = find_entries(entries, name);
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one search entry {name}: {entries:?}"
    );
    matches[0]
}

fn assert_no_entry(entries: &[serde_json::Value], name: &str) {
    assert!(
        find_entries(entries, name).is_empty(),
        "unexpected search entry {name}: {entries:?}"
    );
}

/// Every member entry must point at an anchor that the generated page defines.
fn assert_member_anchors_exist(out: &Path, entries: &[serde_json::Value]) {
    for entry in entries {
        let kind = entry["kind"].as_str().unwrap_or_default();
        if kind != "method" && kind != "field" {
            continue;
        }
        let href = entry["href"]
            .as_str()
            .unwrap_or_else(|| panic!("member entry without href: {entry}"));
        let (page, anchor) = href
            .split_once('#')
            .unwrap_or_else(|| panic!("member entry without anchor: {entry}"));
        let path = out.join(page);
        let html = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read html page {}: {err}", path.display()));
        let id = format!("id=\"{anchor}\"");
        assert!(
            html.contains(&id),
            "search index points at missing anchor {anchor} on {page}"
        );
    }
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

    assert_member_anchors_exist(&out, &entries);
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

    assert_member_anchors_exist(&out, &entries);
}

#[test]
fn search_index_contains_global_methods_and_fields() {
    let out = common::generate_html_fixture("html_search_index_global", GLOBAL_SOURCE);
    let entries = common::read_search_index(&out);

    let page = find_entry(&entries, "g");
    assert_eq!(page["href"], "global/g.html");
    assert_eq!(page["kind"], "global");

    let method = find_entry(&entries, "g.foo");
    assert_eq!(method["href"], "global/g.html#g.foo");
    assert_eq!(method["kind"], "method");

    let field = find_entry(&entries, "g.bar");
    assert_eq!(field["href"], "global/g.html#g.bar");
    assert_eq!(field["kind"], "field");

    assert_member_anchors_exist(&out, &entries);
}

#[test]
fn search_index_contains_enum_fields() {
    let out = common::generate_html_fixture("html_search_index_enum", ENUM_SOURCE);
    let entries = common::read_search_index(&out);

    let page = find_entry(&entries, "enum Color");
    assert_eq!(page["href"], "enum/Color.html");
    assert_eq!(page["kind"], "enum");

    for name in ["Color.RED", "Color.GREEN"] {
        let field = find_entry(&entries, name);
        assert_eq!(field["href"], format!("enum/Color.html#{name}"));
        assert_eq!(field["kind"], "field");
    }

    assert_member_anchors_exist(&out, &entries);
}

#[test]
fn search_index_skips_private_members() {
    let out = common::generate_html_fixture("html_search_index_private", PRIVATE_SOURCE);
    let entries = common::read_search_index(&out);

    assert_no_entry(&entries, "Widget.secret");

    let method = find_entry(&entries, "Widget.run");
    assert_eq!(method["kind"], "method");

    assert_member_anchors_exist(&out, &entries);
}

#[test]
fn search_index_keeps_namespaced_member_anchors() {
    let out = common::generate_html_fixture("html_search_index_namespaced", NAMESPACED_SOURCE);
    let entries = common::read_search_index(&out);

    let page = find_entry(&entries, "class lsp.Client");
    assert_eq!(page["href"], "class/lsp.Client.html");

    let field = find_entry(&entries, "lsp.Client.name");
    assert_eq!(field["href"], "class/lsp.Client.html#lsp.Client.name");
    assert_eq!(field["kind"], "field");

    let method = find_entry(&entries, "lsp.Client.run");
    assert_eq!(method["href"], "class/lsp.Client.html#lsp.Client.run");
    assert_eq!(method["kind"], "method");

    assert_member_anchors_exist(&out, &entries);
}

#[test]
fn search_index_alias_has_no_members() {
    let out = common::generate_html_fixture("html_search_index_alias", ALIAS_SOURCE);
    let entries = common::read_search_index(&out);

    let page = find_entry(&entries, "alias Handler");
    assert_eq!(page["href"], "alias/Handler.html");
    assert_eq!(page["kind"], "alias");

    assert!(
        entries.iter().all(|entry| !entry["href"]
            .as_str()
            .unwrap_or_default()
            .starts_with("alias/Handler.html#")),
        "alias page must not have member entries: {entries:?}"
    );
}

#[test]
fn search_index_is_empty_without_docs() {
    let out = common::generate_html_fixture("html_search_index_empty", "");
    let entries = common::read_search_index(&out);
    assert!(entries.is_empty(), "unexpected entries: {entries:?}");
}
