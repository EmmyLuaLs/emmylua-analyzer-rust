//! Regression test for parameter / return-value documentation in the
//! markdown output.
//!
//! The HTML generator renders `@param` / `@return` descriptions as a dedicated
//! "Parameters" / "Returns" table, but the markdown generator used to either
//! drop them or dump raw `@param \`name\` - ...` lines into the page.  This test
//! checks that a function with documented parameters produces a proper
//! `**Parameters**` / `**Returns**` block and no raw `@param` / `@return` lines.

mod common;

const SOURCE: &str = r#"local M = {}

---@param a number @the first operand
---@param b number @the second operand
---@return number @the sum
function M.add(a, b)
    return a + b
end

return M
"#;

const TYPE_ONLY_SOURCE: &str = r#"local M = {}

---@param a number
---@param b string
---@return boolean
function M.check(a, b)
    return true
end

return M
"#;

const TYPE_SOURCE: &str = r#"---@class demo
local M = {}

---@param a number @the first operand
---@return number @the sum
function M.add(a)
    return a
end

return M
"#;

const DOC_FUNCTION_SOURCE: &str = r#"---@class demo
---@field add fun(a: number, b: number): number
local exports = {}

return exports
"#;

const UNDOCUMENTED_SOURCE: &str = r#"local M = {}

function M.identity(value)
    return value
end

return M
"#;

#[test]
fn markdown_renders_parameter_and_return_descriptions() {
    let md = common::generate_module_markdown("markdown_params", SOURCE);

    assert!(
        md.contains("**Parameters**"),
        "missing `**Parameters**` section:\n{md}"
    );
    assert!(
        md.contains("**Returns**"),
        "missing `**Returns**` section:\n{md}"
    );
    assert!(
        md.contains("the first operand"),
        "missing description of parameter `a`:\n{md}"
    );
    assert!(
        md.contains("the second operand"),
        "missing description of parameter `b`:\n{md}"
    );
    assert!(
        md.contains("the sum"),
        "missing return-value description:\n{md}"
    );

    // The old implementation wrote raw `@param` / `@return` lines into the
    // page; make sure they are gone now that a dedicated block is rendered.
    assert!(
        !md.contains("@param `"),
        "raw `@param` line leaked into the markdown:\n{md}"
    );
    assert!(
        !md.contains("@return "),
        "raw `@return` line leaked into the markdown:\n{md}"
    );
}

#[test]
fn markdown_renders_type_only_parameters() {
    let md = common::generate_module_markdown("markdown_params_type_only", TYPE_ONLY_SOURCE);

    assert!(
        md.contains("**Parameters**"),
        "type-only parameters were dropped:\n{md}"
    );
    assert!(
        md.contains("- **a** (*number*)"),
        "missing type of parameter `a`:\n{md}"
    );
    assert!(
        md.contains("- **b** (*string*)"),
        "missing type of parameter `b`:\n{md}"
    );
    assert!(
        md.contains("**Returns**"),
        "type-only return value was dropped:\n{md}"
    );
    assert!(md.contains("(*boolean*)"), "missing return type:\n{md}");
}

#[test]
fn markdown_renders_class_type_member_details() {
    let md = common::generate_type_markdown("markdown_params_type_page", TYPE_SOURCE, "demo");

    assert!(
        md.contains("**Parameters**"),
        "missing `**Parameters**` section on the type page:\n{md}"
    );
    assert!(
        md.contains("**Returns**"),
        "missing `**Returns**` section on the type page:\n{md}"
    );
    assert!(
        md.contains("the first operand"),
        "missing parameter description on the type page:\n{md}"
    );
    assert!(
        md.contains("the sum"),
        "missing return-value description on the type page:\n{md}"
    );
}

#[test]
fn markdown_renders_doc_function_details() {
    let md = common::generate_module_markdown("markdown_params_doc_function", DOC_FUNCTION_SOURCE);

    assert!(
        md.contains("**Parameters**"),
        "`DocFunction` parameters were dropped:\n{md}"
    );
    assert!(
        md.contains("- **a** (*number*)"),
        "missing type of parameter `a`:\n{md}"
    );
    assert!(
        md.contains("- **b** (*number*)"),
        "missing type of parameter `b`:\n{md}"
    );
    assert!(
        md.contains("**Returns**"),
        "`DocFunction` return value was dropped:\n{md}"
    );
    assert!(md.contains("(*number*)"), "missing return type:\n{md}");
}

#[test]
fn markdown_omits_empty_detail_tables() {
    let md = common::generate_module_markdown("markdown_params_undocumented", UNDOCUMENTED_SOURCE);

    assert!(
        !md.contains("**Parameters**"),
        "unexpected empty `**Parameters**` section:\n{md}"
    );
    assert!(
        !md.contains("**Returns**"),
        "unexpected empty `**Returns**` section:\n{md}"
    );
}
