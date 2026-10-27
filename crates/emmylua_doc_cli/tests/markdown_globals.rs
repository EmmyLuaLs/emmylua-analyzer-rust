//! Regression test for parameter / return-value documentation of global
//! functions in the markdown output.
//!
//! Removing the raw `@param` / `@return` text dump from the rendered signature
//! must not silently drop the documentation of simple global functions.

mod common;

const SOURCE: &str = r#"---@param name string @who to greet
---@return string @the greeting
---@overload fun(name: string, punctuation: string): string
function greet(name)
    return "hello " .. name
end
"#;

#[test]
fn markdown_renders_global_function_details() {
    let md = common::generate_global_markdown("markdown_globals", SOURCE);

    assert!(
        md.contains("**Parameters**"),
        "missing `**Parameters**` section:\n{md}"
    );
    assert!(
        md.contains("**Returns**"),
        "missing `**Returns**` section:\n{md}"
    );
    assert!(
        md.contains("**Overloads**"),
        "missing `**Overloads**` section:\n{md}"
    );
    assert!(
        md.contains("who to greet"),
        "missing parameter description:\n{md}"
    );
    assert!(
        md.contains("the greeting"),
        "missing return-value description:\n{md}"
    );
    assert!(
        md.contains("punctuation: string"),
        "missing overload signature:\n{md}"
    );
}
