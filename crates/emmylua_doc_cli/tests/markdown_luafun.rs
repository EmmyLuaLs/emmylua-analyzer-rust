//! Regression tests for the patterns used by the Lua Fun documentation:
//! export wrappers, aliases, `@field` members and class pages.

mod common;

const WRAPPER_SOURCE: &str = r#"local M = {}

local function export1(fun)
    return function(arg1, gen, param, state)
        return fun(arg1, gen)
    end
end

---@param a number @the first operand
---@param b number @the second operand
---@return number @the sum
---@overload fun(text: string): number
local add = function(a, b)
    return a + b
end

--- Adds two numbers.
M.add = export1(add)

--- An alias for add.
M.plus = M.add

return M
"#;

const OVERLOAD_WITH_CLASS_SOURCE: &str = r#"---@class demo
local M = {}

---@param a number @the first operand
---@overload fun(text: string): number
local add = function(a)
    return a
end

M.add = add

return M
"#;

const FIELD_SOURCE: &str = r#"---@class Repro
---@field a string @simple type, description is rendered
---@field b fun(x: number): string @function type, description is rendered
---@field c function @bare function type, description is rendered
local Repro = {}

return Repro
"#;

const FIELD_FALLBACK_SOURCE: &str = r#"---@class demo
---@field add fun(a: number, b: number): number
local demo = {}

---@param a number @the first operand
---@param b number @the second operand
---@return number @the sum
local add = function(a, b)
    return a + b
end

return demo
"#;

const TYPE_SOURCE: &str = r#"--- An iterator over a sequence.
---@class Iter
local Iter = {}

---@class Exported
local exports = {}

---@param a number @the first operand
function exports.add(a)
    return a
end

return exports
"#;

const UNRELATED_TABLE_SOURCE: &str = r#"---@class Iterator
---@type table<string, any>
local methods = {}

---@param b number @the second operand
function methods.foo(b)
    return b
end

---@class Exported
local exports = {}

return exports
"#;

#[test]
fn markdown_resolves_docs_through_export_wrapper() {
    let md = common::generate_module_markdown("markdown_luafun_wrapper", WRAPPER_SOURCE);

    assert!(
        md.contains("### mod.add"),
        "module members should use the module name:\n{md}"
    );
    assert!(
        md.contains("- **a** (*number*) - the first operand"),
        "missing wrapped parameter description:\n{md}"
    );
    assert!(
        md.contains("- **b** (*number*) - the second operand"),
        "missing wrapped parameter description:\n{md}"
    );
    assert!(
        md.contains("**Returns**"),
        "missing return details behind the wrapper:\n{md}"
    );
    assert!(
        md.contains("**Overloads**"),
        "missing overloads behind the wrapper:\n{md}"
    );
    assert!(
        md.contains("Adds two numbers."),
        "missing member description:\n{md}"
    );
}

#[test]
fn markdown_keeps_overloads_when_a_class_is_in_scope() {
    let md = common::generate_module_markdown(
        "markdown_luafun_overload_with_class",
        OVERLOAD_WITH_CLASS_SOURCE,
    );

    assert!(
        md.contains("**Overloads**"),
        "overloads must stay on the signature when a class was declared earlier:\n{md}"
    );
    assert!(
        md.contains("text: string"),
        "missing overload signature:\n{md}"
    );
}

#[test]
fn markdown_resolves_docs_through_alias() {
    let md = common::generate_module_markdown("markdown_luafun_alias", WRAPPER_SOURCE);

    assert!(
        md.contains("### mod.plus"),
        "alias should be documented as a member:\n{md}"
    );
    assert!(
        md.contains("An alias for add."),
        "missing alias description:\n{md}"
    );
    // The alias is rendered as a function with the target's documentation.
    let alias_start = md.find("### mod.plus").expect("alias section");
    let alias = &md[alias_start..];
    assert!(
        alias.contains("- **a** (*number*) - the first operand"),
        "alias should inherit parameter descriptions:\n{md}"
    );
}

#[test]
fn markdown_renders_field_descriptions_for_function_types() {
    let md = common::generate_type_markdown("markdown_luafun_fields", FIELD_SOURCE, "Repro");

    assert!(
        md.contains("simple type, description is rendered"),
        "missing non-function field description:\n{md}"
    );
    assert!(
        md.contains("function type, description is rendered"),
        "missing fun(...) field description:\n{md}"
    );
    assert!(
        md.contains("bare function type, description is rendered"),
        "missing bare function field description:\n{md}"
    );
}

#[test]
fn markdown_field_function_falls_back_to_declaration() {
    let md = common::generate_type_markdown(
        "markdown_luafun_field_fallback",
        FIELD_FALLBACK_SOURCE,
        "demo",
    );

    assert!(
        md.contains("**Parameters**"),
        "missing parameters for an @field fun(...) member:\n{md}"
    );
    assert!(
        md.contains("- **a** (*number*) - the first operand"),
        "missing parameter description from the same-named declaration:\n{md}"
    );
    assert!(
        md.contains("- (*number*) - the sum"),
        "missing return description from the same-named declaration:\n{md}"
    );
}

#[test]
fn markdown_renders_type_description() {
    let md = common::generate_type_markdown("markdown_luafun_type_desc", TYPE_SOURCE, "Iter");

    assert!(
        md.contains("An iterator over a sequence."),
        "missing class description:\n{md}"
    );
    assert!(
        md.starts_with("# class Iter"),
        "type pages should use the simple type name:\n{md}"
    );
}

#[test]
fn markdown_skips_unrelated_table_members_on_type_page() {
    let md = common::generate_type_markdown(
        "markdown_luafun_unrelated",
        UNRELATED_TABLE_SOURCE,
        "Iterator",
    );

    assert!(
        !md.contains("Iterator.foo"),
        "members of an unrelated `---@type` table leaked onto the class page:\n{md}"
    );
}
