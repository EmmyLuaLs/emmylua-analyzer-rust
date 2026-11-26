//! Regression test for parameters / return values of module functions in the
//! JSON output (#1457).
//!
//! Module functions declared outside the module table infer as a bare
//! `LuaType::Function`; their `---@param` / `---@return` annotations live in
//! `DocMember.signature`. The JSON export now falls back to that signature, so
//! `params` / `returns` are no longer empty.

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

#[test]
fn json_exports_module_function_parameters_and_returns() {
    let json = common::generate_json_fixture("json_params_module", SOURCE);
    let modules = json["modules"].as_array().expect("modules array");
    assert_eq!(modules.len(), 1, "{json}");
    let members = modules[0]["members"].as_array().expect("members array");
    let add = members
        .iter()
        .find(|member| member["name"] == "add")
        .unwrap_or_else(|| panic!("module member `add` was not exported: {json}"));

    let params = add["params"].as_array().expect("params array");
    assert_eq!(params.len(), 2, "unexpected params: {add}");
    assert_eq!(params[0]["name"], "a");
    assert_eq!(params[0]["typ"], "number");
    assert_eq!(params[1]["name"], "b");
    assert_eq!(params[1]["typ"], "number");

    let returns = add["returns"].as_array().expect("returns array");
    assert_eq!(returns.len(), 1, "unexpected returns: {add}");
    assert_eq!(returns[0]["typ"], "number");
}
