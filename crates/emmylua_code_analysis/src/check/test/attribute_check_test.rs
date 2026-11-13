//! Tests for the attribute_check checker (mirrors the old `compilation/test/attribute_test.rs`
//! `test_attribute_overload_uses_arg_type_for_diagnostic` + parameter count checks).

use super::{check_source, count_by_code};
use crate::DiagnosticCode;

const ATTRIBUTE_HEADER: &str = r#"
---@class Attribute
"#;

/// Overload covers the argument type → no type mismatch reported.
#[test]
fn test_attribute_overload_accepts_matching_arg() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class custom_attribute: Attribute
---@overload fun(value: string)
---@overload fun(value: integer)

---@[custom_attribute(1)]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeParamTypeMismatch),
        0,
        "diagnostics: {:?}",
        diags
    );
}

/// Argument type does not match the only overload → AttributeParamTypeMismatch.
#[test]
fn test_attribute_param_type_mismatch() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class string_attribute: Attribute
---@overload fun(value: string)

---@[string_attribute(1)]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeParamTypeMismatch),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// Required parameter missing → AttributeMissingParameter.
#[test]
fn test_attribute_missing_parameter() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class two_arg_attribute: Attribute
---@overload fun(first: string, second: integer)

---@[two_arg_attribute("x")]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeMissingParameter),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// Redundant parameter → AttributeRedundantParameter.
#[test]
fn test_attribute_redundant_parameter() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class one_arg_attribute: Attribute
---@overload fun(value: string)

---@[one_arg_attribute("x", "y")]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeRedundantParameter),
        1,
        "diagnostics: {:?}",
        diags
    );
}

/// Annotations on classes not derived from Attribute are not checked.
#[test]
fn test_non_attribute_class_ignored() {
    let diags = check_source(&format!(
        r#"{ATTRIBUTE_HEADER}
---@class plain
---@overload fun(value: string)

---@[plain(1)]
local value
"#
    ));
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeParamTypeMismatch),
        0
    );
    assert_eq!(
        count_by_code(&diags, DiagnosticCode::AttributeMissingParameter),
        0
    );
}

/// The built-in attribute signatures come from `std/builtin.lua`; these tests load
/// the real std lib so the contracts documented there are pinned.
mod builtin_attributes {
    use crate::{DiagnosticCode, VirtualWorkspace};

    fn workspace() -> VirtualWorkspace {
        VirtualWorkspace::new_with_init_std_lib()
    }

    #[test]
    fn test_deprecated_accepts_optional_message() {
        let mut ws = workspace();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            "---@[deprecated]\nlocal value"
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            "---@[deprecated(\"use the new API\")]\nlocal value"
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            "---@[deprecated(1)]\nlocal value"
        ));
    }

    #[test]
    fn test_lsp_optimization_only_accepts_delayed_definition() {
        let mut ws = workspace();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            "---@[lsp_optimization(\"delayed_definition\")]\nlocal value"
        ));
        // The legacy `check_table_field` alias was removed together with
        // `skip_table_fields_check`; the annotation no longer accepts it.
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            "---@[lsp_optimization(\"check_table_field\")]\nlocal value"
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeMissingParameter,
            "---@[lsp_optimization]\nlocal value"
        ));
    }

    #[test]
    fn test_index_alias_requires_string_name() {
        let mut ws = workspace();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            "---@class IndexAliasOk\n---@[index_alias(\"first\")]\n---@field [1] string\nlocal IndexAliasOk = {}"
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            "---@class IndexAliasBad\n---@[index_alias(1)]\n---@field [1] string\nlocal IndexAliasBad = {}"
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeMissingParameter,
            "---@class IndexAliasMissing\n---@[index_alias]\n---@field [1] string\nlocal IndexAliasMissing = {}"
        ));
    }

    #[test]
    fn test_constructor_argument_types() {
        let mut ws = workspace();
        let header = r#"
        ---@class Attribute
"#;
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &format!(
                r#"{header}
        ---@generic T
        ---@[constructor("__init")]
        ---@param name `T`
        ---@return T
        function meta_ok(name) end
        "#
            )
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &format!(
                r#"{header}
        ---@generic T
        ---@[constructor("__init", "Base", false, "doc")]
        ---@param name `T`
        ---@return T
        function meta_full(name) end
        "#
            )
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &format!(
                r#"{header}
        ---@generic T
        ---@[constructor(1)]
        ---@param name `T`
        ---@return T
        function meta_bad_name(name) end
        "#
            )
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &format!(
                r#"{header}
        ---@generic T
        ---@[constructor("__init", "Base", "not-a-boolean", "doc")]
        ---@param name `T`
        ---@return T
        function meta_bad_strip_self(name) end
        "#
            )
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &format!(
                r#"{header}
        ---@generic T
        ---@[constructor("__init", "Base", false, "bogus")]
        ---@param name `T`
        ---@return T
        function meta_bad_return_mode(name) end
        "#
            )
        ));
    }

    #[test]
    fn test_field_accessor_argument_types() {
        let mut ws = workspace();
        let class = |attr: &str| {
            format!(
                "---@class FieldAccessorClass\n---@[{attr}]\n---@field age number\nlocal FieldAccessorClass = {{}}"
            )
        };
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &class("field_accessor")
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &class("field_accessor(\"camelCase\")")
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &class("field_accessor(\"PascalCase\", \"readAge\", \"writeAge\")")
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &class("field_accessor(\"bogus\")")
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            &class("field_accessor(\"camelCase\", 1)")
        ));
    }
}

