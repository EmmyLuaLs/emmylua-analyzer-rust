use emmylua_code_analysis::VirtualWorkspace;
use serde_json::{Value, json};

use crate::doc_model::DocModel;
use crate::json_generator::export::export;

/// The JSON export of a single Lua file.
fn export_json(code: &str) -> Value {
    export_files(&[code])
}

/// The JSON export of several Lua files.
fn export_files(files: &[&str]) -> Value {
    let mut ws = VirtualWorkspace::new();
    for code in files {
        ws.def(code);
    }
    let model = DocModel::build(&ws.analysis);
    serde_json::to_value(export(&model, ws.analysis.emmyrc.clone())).expect("export serializes")
}

fn find<'a>(json: &'a Value, section: &str, name: &str) -> &'a Value {
    json[section]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["name"] == name))
        .unwrap_or_else(|| panic!("no {name} in {section}: {json:#}"))
}

fn member<'a>(owner: &'a Value, name: &str) -> &'a Value {
    owner["members"]
        .as_array()
        .and_then(|members| members.iter().find(|member| member["name"] == name))
        .unwrap_or_else(|| panic!("no member {name}: {owner:#}"))
}

fn names(values: &Value, key: &str) -> Vec<Value> {
    values
        .as_array()
        .map(|values| values.iter().map(|value| value[key].clone()).collect())
        .unwrap_or_default()
}

#[test]
fn global_table_has_functions_declared_on_it() {
    let json = export_json(
        r#"
        SomeTable = {}

        ---@param x integer
        ---@return string
        function SomeTable.Foo(x) end
        "#,
    );
    let foo = member(find(&json, "globals", "SomeTable"), "Foo");
    assert_eq!(foo["type"], "fn");
    assert_eq!(names(&foo["params"], "name"), ["x"]);
    assert_eq!(names(&foo["params"], "typ"), ["integer"]);
    assert_eq!(names(&foo["returns"], "typ"), ["string"]);
}

#[test]
fn class_has_methods_declared_on_its_table() {
    let json = export_json(
        r#"
        ---@class Car
        ---@field maxSpeed number
        local Car = {}

        ---@param speed number
        function Car:drive(speed) end
        "#,
    );
    let car = find(&json, "types", "Car");
    assert_eq!(names(&car["members"], "name"), ["drive", "maxSpeed"]);
    let drive = member(car, "drive");
    assert_eq!(drive["type"], "fn");
    assert_eq!(drive["is_meth"], true);
    assert_eq!(names(&drive["params"], "typ"), ["number"]);
}

#[test]
fn class_has_functions_declared_on_a_differently_named_global() {
    let json = export_json(
        r#"
        ---@class Widget
        widget = {}

        ---@param b integer
        function widget.Foo(b) end
        "#,
    );
    let foo = member(find(&json, "types", "Widget"), "Foo");
    assert_eq!(names(&foo["params"], "typ"), ["integer"]);
}

#[test]
fn namespaced_class_has_its_members() {
    let json = export_json(
        r#"
        Ns = {}

        ---@class Ns.Point
        Ns.Point = {
            ---@type number
            x = 0,
        }

        ---@param dx number
        function Ns.Point:move(dx) end
        "#,
    );
    let point = find(&json, "types", "Ns.Point");
    assert_eq!(names(&point["members"], "name"), ["move", "x"]);
}

#[test]
fn every_definition_of_a_function_is_exported() {
    let json = export_json(
        r#"
        T = {}

        ---@param a integer
        function T.f(a) end

        ---@param a string
        ---@param b string
        function T.f(a, b) end
        "#,
    );
    let t = find(&json, "globals", "T");
    let params: Vec<_> = t["members"]
        .as_array()
        .map(|members| {
            members
                .iter()
                .map(|f| names(&f["params"], "name"))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(params, [vec!["a"], vec!["a", "b"]]);
}

#[test]
fn functions_at_the_same_place_in_different_files_keep_their_own_overloads() {
    let json = export_files(&[
        "A = {}
---@overload fun(x: string)
function A.f(x) end
",
        "B = {}
---@overload fun(y: number)
function B.f(y) end
",
    ]);
    let a = member(find(&json, "globals", "A"), "f");
    let b = member(find(&json, "globals", "B"), "f");
    assert_eq!(a["overloads"], serde_json::json!(["fun(x: string)"]));
    assert_eq!(b["overloads"], serde_json::json!(["fun(y: number)"]));
}

#[test]
fn enum_has_its_table_constructor_entries() {
    let json = export_json(
        r#"
        ---@enum Numbers
        Numbers = {
            ONE = 1,
            TWO = 2,
        }
        "#,
    );
    let numbers = find(&json, "types", "Numbers");
    assert_eq!(names(&numbers["members"], "name"), ["ONE", "TWO"]);
}

#[test]
fn module_has_functions_declared_on_its_export() {
    let json = export_json(
        r#"
        local M = {}

        ---@param a string
        function M.greet(a) end

        return M
        "#,
    );
    let module = &json["modules"][0];
    let greet = member(module, "greet");
    assert_eq!(names(&greet["params"], "typ"), ["string"]);
}

#[test]
fn function_member_typed_with_a_named_type_keeps_the_name() {
    let json = export_json(
        r#"
        ---@alias Callback fun(unitID: integer): boolean

        T = {}

        ---@type Callback
        T.onUnit = function(unitID) end
        "#,
    );
    let on_unit = member(find(&json, "globals", "T"), "onUnit");
    assert_eq!(on_unit["type"], "field");
    assert_eq!(on_unit["typ"], "Callback");
}

#[test]
fn documented_unknown_return_is_kept() {
    let json = export_json(
        r#"
        T = {}

        ---@return unknown
        function T.f() end
        "#,
    );
    let f = member(find(&json, "globals", "T"), "f");
    assert_eq!(names(&f["returns"], "typ"), ["unknown"]);
}

#[test]
fn function_without_returns_has_none() {
    let json = export_json(
        r#"
        T = {}
        function T.f() end
        "#,
    );
    let f = member(find(&json, "globals", "T"), "f");
    assert_eq!(f["returns"], Value::Array(Vec::new()));
}

#[test]
fn types_refer_to_classes_by_name() {
    let json = export_json(
        r#"
        ---@class Element
        ---@field id string

        ---@alias Elements Element[]
        "#,
    );
    assert_eq!(find(&json, "types", "Elements")["typ"], "Element[]");
}

#[test]
fn alias_has_no_members_of_the_code_below_it() {
    let json = export_json(
        r#"
        ---@alias Id integer
        T = {}

        function T.f() end
        "#,
    );
    assert_eq!(find(&json, "types", "Id")["members"], json!([]));
}
