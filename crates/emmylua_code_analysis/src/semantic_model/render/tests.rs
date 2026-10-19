use googletest::prelude::*;

use crate::{DiagnosticCode, VirtualWorkspace};

#[gtest]
fn explicit_table_arguments_survive_standard_library_definition() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    ws.def("---@class Train\n---@field id integer");

    for annotation in [
        "table<integer,Train>",
        "table<integer>",
        "table<unknown,Train>",
        "table<string,table<integer,Train>>",
    ] {
        let ty = ws.ty(annotation);
        assert_that!(
            ws.humanize_type(ty.clone()),
            eq(annotation),
            "annotation: {annotation}"
        );
        assert_that!(
            ws.humanize_type_detailed(ty),
            eq(annotation),
            "annotation: {annotation}"
        );
    }
}

#[gtest]
fn rendering_preserves_arguments_beyond_declared_parameters() {
    let mut ws = VirtualWorkspace::new();
    ws.def("---@class Plain\n---@class Box<T>");

    for annotation in [
        "Plain<string>",
        "Box<string,integer>",
        "Missing<string,integer>",
    ] {
        let ty = ws.ty(annotation);
        assert_that!(
            ws.humanize_type(ty.clone()),
            eq(annotation),
            "annotation: {annotation}"
        );
        assert_that!(
            ws.humanize_type_detailed(ty),
            eq(annotation),
            "annotation: {annotation}"
        );
    }
}

#[gtest]
fn rendering_still_fills_defaults_and_missing_parameters() {
    let mut ws = VirtualWorkspace::new();
    ws.def("---@class Pair<T, U = T[]>\n---@class Required<T, U>");

    for (annotation, expected) in [
        ("Pair<string>", "Pair<string,string[]>"),
        ("Pair<string,integer>", "Pair<string,integer>"),
        ("Required<string>", "Required<string,unknown>"),
    ] {
        let ty = ws.ty(annotation);
        assert_that!(
            ws.humanize_type(ty.clone()),
            eq(expected),
            "annotation: {annotation}"
        );
        assert_that!(
            ws.humanize_type_detailed(ty),
            eq(expected),
            "annotation: {annotation}"
        );
    }
}

#[gtest]
fn rendering_instantiates_generic_aliases() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
        ---@class Box<T>
        ---@alias List<T> T[]
        ---@alias Value<T> T['value']
        ---@alias Nested<T> Box<T>
        ---@alias Callback<T> fun(value: T): T
        ---@alias Values<T> table<string, T>
        "#,
    );
    for (annotation, detailed) in [
        ("List<string,integer>", "List<string,integer> = string[]"),
        (
            "Value<{value:integer},string>",
            "Value<{ value: integer },string> = integer",
        ),
        ("Nested<string>", "Nested<string> = Box<string>"),
        (
            "Callback<string>",
            "Callback<string> = fun(value: string) -> string",
        ),
        ("Values<string>", "Values<string> = table<string,string>"),
    ] {
        let ty = ws.ty(annotation);
        assert_that!(
            ws.humanize_type_detailed(ty),
            eq(detailed),
            "annotation: {annotation}"
        );
    }
}

#[gtest]
fn rendering_does_not_change_missing_argument_diagnostics() {
    let mut ws = VirtualWorkspace::new();
    ws.def("---@class Required<T, U>");
    let ty = ws.ty("Required<string>");
    assert_that!(ws.humanize_type(ty), eq("Required<string,unknown>"));
    assert_that!(
        ws.has_no_diagnostic(
            DiagnosticCode::MissingTypeArgument,
            "---@type Required<string>\nlocal value"
        ),
        eq(false)
    );
}
