#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    /// Same-name class defined twice: each definition site is reported once.
    #[test]
    fn test_duplicate_class() {
        let diags = super::super::check_source(
            "---@class Foo\nlocal Foo = {}\n---@class Foo\nlocal Bar = {}",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// All partial: valid, not reported.
    #[test]
    fn test_duplicate_class_all_partial_ok() {
        let diags = super::super::check_source(
            "---@class(partial) Foo\n---@class(partial) Foo\nlocal Foo = {}",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 0);
    }

    /// Mixed partial and non-partial: reported.
    #[test]
    fn test_duplicate_class_partial_mixed() {
        let diags =
            super::super::check_source("---@class Foo\n---@class(partial) Foo\nlocal Foo = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// Same-name enum defined twice: reported.
    #[test]
    fn test_duplicate_enum() {
        let diags = super::super::check_source("---@enum Color\n---@enum Color");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// Same-name alias defined twice: reported.
    #[test]
    fn test_duplicate_alias() {
        let diags = super::super::check_source("---@alias Age number\n---@alias Age string");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// Single definition: not reported.
    #[test]
    fn test_single_type_ok() {
        let diags = super::super::check_source("---@class Foo\nlocal Foo = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 0);
    }
}
