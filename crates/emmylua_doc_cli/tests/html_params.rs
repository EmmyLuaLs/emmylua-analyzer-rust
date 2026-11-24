//! Regression test for parameters / return values of module functions in the
//! HTML output (#1457).
//!
//! Functions declared outside the module table (`function M.add`) infer as a
//! bare `LuaType::Function`, so the HTML generator used to drop the documented
//! `---@param` / `---@return` data stored in `DocMember.signature`. Rendering
//! now falls back to the signature, matching markdown, and links class types to
//! their pages.

mod common;

const FUNCTION_SOURCE: &str = r#"local M = {}

---@param a number @the first operand
---@param b number @the second operand
---@return number @the sum
function M.add(a, b)
    return a + b
end

return M
"#;

const ASSIGNMENT_SOURCE: &str = r#"local M = {}

---@param a number @the first operand
---@param b number @the second operand
---@return number @the sum
M.add = function(a, b)
    return a + b
end

return M
"#;

const CLASS_RETURN_SOURCE: &str = r#"local M = {}

---@class ulid
local ulid = {}

---@return ulid @a new ulid
function M.new()
    return ulid
end

return M
"#;

const UNDOCUMENTED_SOURCE: &str = r#"local M = {}

function M.identity(value)
    return value
end

return M
"#;

const METHOD_SOURCE: &str = r#"local M = {}

---@param a number @the value
---@return number @the doubled value
function M:double(a)
    return a * 2
end

return M
"#;

const MULTIPLE_RETURNS_SOURCE: &str = r#"local M = {}

---@param a number @the input
---@return number @the first result
---@return string @the second result
function M.split(a)
    return a, tostring(a)
end

return M
"#;

const UNRESOLVED_RETURN_SOURCE: &str = r#"local M = {}

---@return MissingType @an unresolved type
function M.make()
    return nil
end

return M
"#;

const ESCAPING_SOURCE: &str = r#"local M = {}

---@param a string @<script>alert(1)</script>
function M.echo(a)
    return a
end

return M
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

fn assert_documented_add(html: &str) {
    let methods = methods_section(html);
    assert!(
        methods.contains("mod.add"),
        "missing add signature:\n{methods}"
    );
    assert!(
        methods.contains("<span class=\"hl-var\">a</span>"),
        "missing parameter `a` in the signature:\n{methods}"
    );
    assert!(
        methods.contains("<span class=\"hl-var\">b</span>"),
        "missing parameter `b` in the signature:\n{methods}"
    );
    assert!(
        methods.contains("<span class=\"hl-op\">-&gt;</span>"),
        "missing return type in the signature:\n{methods}"
    );
    assert!(
        methods.contains("Parameters"),
        "missing `Parameters` block:\n{methods}"
    );
    assert!(
        methods.contains("Returns"),
        "missing `Returns` block:\n{methods}"
    );
    assert!(
        methods.contains("the first operand"),
        "missing description of parameter `a`:\n{methods}"
    );
    assert!(
        methods.contains("the second operand"),
        "missing description of parameter `b`:\n{methods}"
    );
    assert!(
        methods.contains("the sum"),
        "missing return-value description:\n{methods}"
    );
}

#[test]
fn html_renders_module_function_parameters_and_returns() {
    let html = read_module_html("html_params_function", FUNCTION_SOURCE);
    assert_documented_add(&html);
}

#[test]
fn html_renders_module_assignment_parameters_and_returns() {
    let html = read_module_html("html_params_assignment", ASSIGNMENT_SOURCE);
    assert_documented_add(&html);
}

#[test]
fn html_links_class_return_types() {
    let html = read_module_html("html_params_class_return", CLASS_RETURN_SOURCE);
    let methods = methods_section(&html);
    assert!(methods.contains("href="), "missing type link:\n{methods}");
    assert!(
        methods.contains("class/ulid.html"),
        "return type does not link to the class page:\n{methods}"
    );
    assert!(
        methods.contains("a new ulid"),
        "missing return-value description:\n{methods}"
    );
}

#[test]
fn html_renders_colon_method_without_implicit_self() {
    let html = read_module_html("html_params_colon_method", METHOD_SOURCE);
    let methods = methods_section(&html);
    assert!(methods.contains("mod.double"), "missing method:\n{methods}");
    assert!(
        methods.contains("<span class=\"hl-var\">a</span>: <span class=\"hl-kw\">number</span>"),
        "missing documented parameter:\n{methods}"
    );
    assert!(
        !methods.contains("<span class=\"hl-var\">self</span>"),
        "implicit `self` must not be rendered:\n{methods}"
    );
    assert!(
        methods.contains("the doubled value"),
        "missing return-value description:\n{methods}"
    );
}

#[test]
fn html_renders_multiple_returns() {
    let html = read_module_html("html_params_multiple_returns", MULTIPLE_RETURNS_SOURCE);
    let methods = methods_section(&html);
    assert!(
        methods.contains("the first result") && methods.contains("the second result"),
        "missing return-value descriptions:\n{methods}"
    );
    assert!(
        methods.contains("<span class=\"hl-op\">-&gt;</span>"),
        "missing return arrow:\n{methods}"
    );
    // One parameter row plus two return rows.
    assert_eq!(
        methods.matches("<div class=\"param-row\">").count(),
        3,
        "unexpected number of detail rows:\n{methods}"
    );
}

#[test]
fn html_keeps_unresolved_return_in_signature_and_details() {
    let html = read_module_html("html_params_unresolved_return", UNRESOLVED_RETURN_SOURCE);
    let methods = methods_section(&html);
    assert!(
        methods.contains("an unresolved type"),
        "missing return-value description:\n{methods}"
    );
    // Rendered once in the signature and once in the Returns table: the header
    // must not filter return types that the details table keeps.
    assert_eq!(
        methods.matches("MissingType").count(),
        2,
        "return type missing from the signature or the details table:\n{methods}"
    );
}

#[test]
fn html_escapes_parameter_descriptions() {
    let html = read_module_html("html_params_escaping", ESCAPING_SOURCE);
    let methods = methods_section(&html);
    assert!(
        methods.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "parameter description was not escaped:\n{methods}"
    );
    assert!(
        !methods.contains("<script>alert(1)</script>"),
        "unescaped script tag leaked into the page:\n{methods}"
    );
}

#[test]
fn html_omits_empty_detail_tables() {
    let html = read_module_html("html_params_undocumented", UNDOCUMENTED_SOURCE);
    let methods = methods_section(&html);
    assert!(
        !methods.contains("Parameters"),
        "unexpected empty `Parameters` block:\n{methods}"
    );
    assert!(
        !methods.contains("Returns"),
        "unexpected empty `Returns` block:\n{methods}"
    );
}
