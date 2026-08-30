#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    #[test]
    fn test_redefined_local_in_same_block() {
        let diags = super::super::check_source("local a = 1\nlocal a = 2\nprint(a)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::RedefinedLocal), 1);
    }

    #[test]
    fn test_redefined_in_nested_scope() {
        // Outer `a`, inner `a` declared again → treated as redefinition.
        let diags =
            super::super::check_source("local a = 1\nif true then\n  local a = 2\nend\nprint(a)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::RedefinedLocal), 1);
    }

    #[test]
    fn test_distinct_names_ok() {
        let diags = super::super::check_source("local a = 1\nlocal b = 2\nprint(a + b)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::RedefinedLocal), 0);
    }
}
