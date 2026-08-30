#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    /// Two-class circular inheritance: both class definitions are reported.
    #[test]
    fn test_circular_inheritance_pair() {
        let diags = super::super::check_source(
            "---@class A : B\n---@class B : A\nlocal A = {}\nlocal B = {}",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::CircleDocClass), 2);
    }

    /// Three-node cycle: each class is reported once.
    #[test]
    fn test_circular_inheritance_triple() {
        let diags = super::super::check_source(
            "---@class A : B\n---@class B : C\n---@class C : A\nlocal A = {}\nlocal B = {}\nlocal C = {}",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::CircleDocClass), 3);
    }

    /// Self-inheritance: reported.
    #[test]
    fn test_self_inheritance() {
        let diags = super::super::check_source("---@class A : A\nlocal A = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::CircleDocClass), 1);
    }

    /// Acyclic inheritance: not reported.
    #[test]
    fn test_acyclic_inheritance_ok() {
        let diags = super::super::check_source(
            "---@class A : B\n---@class B : C\n---@class C\nlocal A = {}\nlocal B = {}\nlocal C = {}",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::CircleDocClass), 0);
    }

    /// No inheritance: not reported.
    #[test]
    fn test_no_inheritance_ok() {
        let diags = super::super::check_source("---@class A\nlocal A = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::CircleDocClass), 0);
    }
}
