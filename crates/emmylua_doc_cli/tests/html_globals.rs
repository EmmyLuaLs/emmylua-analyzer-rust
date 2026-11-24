//! Regression test for global table members.
//!
//! The HTML generator used to project only members attached to the global's
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

---A duplicate declaration of foo must not be rendered twice.
---@return table
g.foo = function() end

---Return another value.
---@return table
g.bar = function() end
"#;

fn read_global_html(fixture_name: &str, source: &str) -> String {
    let out = common::generate_html_fixture(fixture_name, source);
    let page = out.join("global").join("g.html");
    std::fs::read_to_string(&page)
        .unwrap_or_else(|err| panic!("read html page {}: {err}", page.display()))
}

fn assert_two_methods(html: &str) {
    let methods = common::html_section(html, "methods")
        .unwrap_or_else(|| panic!("global methods section was not rendered:\n{html}"));
    assert!(
        methods.contains("<span class=\"section-count\">2</span>"),
        "expected exactly two global methods:\n{methods}"
    );
    for name in ["g.foo", "g.bar"] {
        assert!(
            methods.contains(name),
            "missing global method {name}:\n{methods}"
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
fn html_renders_global_runtime_members() {
    let html = read_global_html("html_globals_runtime", RUNTIME_SOURCE);
    assert_two_methods(&html);
}

#[test]
fn html_renders_global_table_literal_members() {
    let html = read_global_html("html_globals_literal", LITERAL_SOURCE);
    assert_two_methods(&html);
}

#[test]
fn html_dedupes_global_members_declared_twice() {
    let html = read_global_html("html_globals_mixed_dedupe", MIXED_DEDUPE_SOURCE);
    assert_two_methods(&html);
}
