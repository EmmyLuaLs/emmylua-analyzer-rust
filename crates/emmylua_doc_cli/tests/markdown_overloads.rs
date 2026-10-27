//! Regression test for `---@overload` rendering in the markdown output.
//!
//! The HTML generator renders `---@overload` signatures in a dedicated
//! "Overloads" block, but the markdown generator used to drop them entirely.
//! This test checks that the overload signature appears in the markdown.

mod common;

const SOURCE: &str = r#"local M = {}

---@param a number @the first operand
---@return number @the sum
---@overload fun(text: string): string
function M.add(a)
    return a
end

return M
"#;

#[test]
fn markdown_renders_overload_signatures() {
    let md = common::generate_module_markdown("markdown_overloads", SOURCE);

    assert!(
        md.contains("**Overloads**"),
        "missing `**Overloads**` section:\n{md}"
    );
    assert!(
        md.contains("text: string"),
        "missing overload signature:\n{md}"
    );
}
