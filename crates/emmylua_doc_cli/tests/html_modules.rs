//! Regression test for module members declared outside the module table (#1420).
//!
//! The HTML generator used to collect only members attached to the exported
//! value type, so members declared after the module table
//! (`function m.foo()`, `m.bar = ...`) were dropped. It now uses the same
//! module member projection as the markdown generator and names members after
//! the module, matching markdown output.

mod common;

const FUNCTIONS_SOURCE: &str = r#"---@meta

local m = {}

---Return a value.
---@return table
function m.foo() end

---Return another value.
---@return table
function m.bar() end

return m
"#;

const ASSIGNMENTS_SOURCE: &str = r#"---@meta

local m = {}

---Return a value.
---@return table
m.foo = function() end

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

---A duplicate declaration of foo must not be rendered twice.
---@return table
m.foo = function() end

---Return another value.
---@return table
m.bar = function() end

return m
"#;

const CLASS_SOURCE: &str = r#"---@class ModClass
local m = {}

---Return a value.
---@return table
function m.foo() end

return m
"#;

fn read_module_html(fixture_name: &str, source: &str) -> String {
    let out = common::generate_html_fixture(fixture_name, source);
    let page = out.join("module").join("mod.html");
    std::fs::read_to_string(&page)
        .unwrap_or_else(|err| panic!("read html page {}: {err}", page.display()))
}

fn methods_section(html: &str) -> &str {
    common::html_section(html, "methods")
        .unwrap_or_else(|| panic!("module methods section was not rendered:\n{html}"))
}

fn assert_two_methods(html: &str) {
    let methods = methods_section(html);
    assert!(
        methods.contains("<span class=\"section-count\">2</span>"),
        "expected exactly two module methods:\n{methods}"
    );
    for name in ["mod.foo", "mod.bar"] {
        assert!(
            methods.contains(name),
            "missing module method {name}:\n{methods}"
        );
    }
    assert!(
        methods.contains("Return a value."),
        "missing foo description:\n{methods}"
    );
    assert!(
        methods.contains("Return another value."),
        "missing bar description:\n{methods}"
    );
    assert!(
        common::html_section(html, "fields").is_none(),
        "methods must not be rendered in the fields section:\n{html}"
    );
}

#[test]
fn html_renders_module_functions_declared_after_table() {
    let html = read_module_html("html_modules_after_table", FUNCTIONS_SOURCE);
    assert_two_methods(&html);
}

#[test]
fn html_renders_module_assignments_declared_after_table() {
    let html = read_module_html("html_modules_assignments", ASSIGNMENTS_SOURCE);
    assert_two_methods(&html);
}

#[test]
fn html_renders_module_table_literal_members() {
    let html = read_module_html("html_modules_literal", LITERAL_SOURCE);
    assert_two_methods(&html);
}

#[test]
fn html_renders_inline_literal_module_members() {
    let html = read_module_html("html_modules_inline_literal", INLINE_LITERAL_SOURCE);
    assert_two_methods(&html);
}

#[test]
fn html_dedupes_module_members_declared_twice() {
    let html = read_module_html("html_modules_mixed_dedupe", MIXED_DEDUPE_SOURCE);
    assert_two_methods(&html);
}

#[test]
fn html_renders_module_members_for_class_export() {
    let html = read_module_html("html_modules_class_export", CLASS_SOURCE);
    let methods = methods_section(&html);
    assert!(
        methods.contains("<span class=\"section-count\">1</span>"),
        "expected exactly one module method:\n{methods}"
    );
    assert!(
        methods.contains("mod.foo"),
        "missing module method foo:\n{methods}"
    );
    assert!(
        methods.contains("Return a value."),
        "missing foo description:\n{methods}"
    );
}
