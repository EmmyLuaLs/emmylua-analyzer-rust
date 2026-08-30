#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    /// Referencing a deprecated global declaration → reported; the definition site itself is not reported.
    #[test]
    fn test_deprecated_global_use() {
        let diags = super::super::check_source(
            "---@deprecated\nglobal_x = 1\nlocal a = global_x\nlocal b = global_x",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 2);
    }

    /// The definition site itself is not counted as a reference.
    #[test]
    fn test_deprecated_definition_site_not_reported() {
        let diags = super::super::check_source("---@deprecated\nglobal_x = 1");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 0);
    }

    /// Referencing a deprecated local function.
    #[test]
    fn test_deprecated_local_function_use() {
        let diags = super::super::check_source(
            "---@deprecated\nlocal old_fn = function() end\nlocal a = old_fn",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 1);
    }

    /// No deprecated marker → not reported.
    #[test]
    fn test_not_deprecated_ok() {
        let diags = super::super::check_source("local old_fn = function() end\nlocal a = old_fn");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 0);
    }

    /// Referencing a deprecated runtime member (`function T.old()`).
    #[test]
    fn test_deprecated_member_use() {
        let diags = super::super::check_source(
            "local T = {}\n---@deprecated\nfunction T.old_method() end\nlocal f = T.old_method",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 1);
    }

    /// `---@deprecated` before `@field`: only that field is marked (mirrors old tag-position semantics).
    #[test]
    fn test_deprecated_class_field_use() {
        let diags = super::super::check_source(
            "---@class C\n---@deprecated\n---@field old number\nlocal C = {}\nlocal x = C.old",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 1);
    }

    /// `---@deprecated` after `@field`: marks the type + statement declaration (`C` is reported, not the field).
    #[test]
    fn test_deprecated_after_field_marks_type_and_decl() {
        let diags = super::super::check_source(
            "---@class C\n---@field old number ---@deprecated\nlocal C = {}\nlocal x = C.old",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 1);
    }

    /// A doc name type referencing a deprecated class (`---@type Old`).
    #[test]
    fn test_deprecated_doc_name_type() {
        let diags =
            super::super::check_source("---@class Old ---@deprecated\n\n---@type Old\nlocal u");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Deprecated), 1);
    }
}
