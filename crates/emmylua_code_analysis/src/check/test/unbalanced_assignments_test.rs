#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use crate::check::test::{check_source, count_by_code};

    #[test]
    fn test_local_unbalanced() {
        // local a, b = 1 → b has no value.
        let diags = check_source("local a, b = 1");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            1
        );
    }

    #[test]
    fn test_assign_unbalanced() {
        // a, b = 1 → b has no value.
        let diags = check_source("local a, b\na, b = 1");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            1
        );
    }

    #[test]
    fn test_balanced_ok() {
        let diags = check_source("local a, b = 1, 2\nlocal c = 3");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            0
        );
    }

    #[test]
    fn test_call_value_skipped() {
        // Last value is a call (may return multiple values) → skipped.
        let diags = check_source("local a, b = some_func()");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            0
        );
    }

    #[test]
    fn test_vararg_tail_supplies_multiple_values() {
        let diags = check_source(
            r#"
            local function values(...)
                local first, second = ...
                first, second = 1, ...
                print(first, second)
            end
            values('hello', 'world')
        "#,
        );
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::UnbalancedAssignments),
            0
        );
    }

    #[test]
    fn test_vararg_single_value_still_reports_missing_values() {
        let source = r#"
            local function values(...)
                local first, parenthesized = (...)
                local other, nonfinal, missing = ..., 'last'
                print(first, parenthesized, other, nonfinal, missing)
            end
            values('hello', 'world')
        "#;
        let diags = check_source(source);
        let offsets: Vec<_> = diags
            .iter()
            .filter(|d| d.code == DiagnosticCode::UnbalancedAssignments)
            .map(|d| usize::from(d.range.start()))
            .collect();
        assert_eq!(
            offsets,
            vec![
                source.find("parenthesized").unwrap(),
                source.find("missing").unwrap()
            ]
        );
    }
}
