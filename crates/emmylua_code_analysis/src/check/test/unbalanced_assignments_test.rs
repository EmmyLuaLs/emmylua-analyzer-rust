#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    #[test]
    fn test_local_unbalanced() {
        // local a, b = 1 → b has no value.
        let diags = super::super::check_source("local a, b = 1");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            1
        );
    }

    #[test]
    fn test_assign_unbalanced() {
        // a, b = 1 → b has no value.
        let diags = super::super::check_source("local a, b\na, b = 1");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            1
        );
    }

    #[test]
    fn test_balanced_ok() {
        let diags = super::super::check_source("local a, b = 1, 2\nlocal c = 3");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            0
        );
    }

    #[test]
    fn test_call_value_skipped() {
        // Last value is a call (may return multiple values) → skipped.
        let diags = super::super::check_source("local a, b = some_func()");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            0
        );
    }
}
