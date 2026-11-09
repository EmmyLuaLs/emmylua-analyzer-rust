#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use crate::check::test::{check_source, count_by_code};

    #[test]
    fn test_const_reassign() {
        // const a is reassigned → LocalConstReassign.
        let diags = check_source("local a <const> = 1\na = a");
        assert_eq!(count_by_code(&diags, DiagnosticCode::LocalConstReassign), 1);
    }

    #[test]
    fn test_const_not_reassigned_ok() {
        let diags = check_source("local a <const> = 1\nprint(a)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::LocalConstReassign), 0);
    }

    #[test]
    fn test_const_assignment_reads_ok() {
        let diags = check_source(
            r#"
            local index <const> = 1
            local values <const> = {}
            local copy = 0
            copy = index
            values[index] = index
            print(copy, values)
            "#,
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::LocalConstReassign), 0);
    }

    /// Reassigning `i` in `for i = 1, 10 do` → IterVariableReassign (enabled by default in Lua 5.5).
    #[test]
    fn test_iter_variable_reassign_for_range() {
        let diags = check_source("for i = 1, 10 do\n    i = i\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            1
        );
    }

    /// Both assignment targets are iteration variables; RHS reads are not writes.
    #[test]
    fn test_iter_variable_reassign_for_stat() {
        let diags = check_source("for k, v in pairs({}) do\n    k, v = v, k\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            2
        );
    }

    #[test]
    fn test_iter_variable_numeric_assignment_reads_ok() {
        let diags = check_source(
            r#"
            local values = {}
            for i = 1, 3 do
                values[#values + 1] = i
            end
            print(values)
            "#,
        );
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            0
        );
    }

    #[test]
    fn test_iter_variable_generic_assignment_reads_ok() {
        let diags = check_source(
            r#"
            local values = {}
            local key, value
            for k, v in pairs({10, 20}) do
                key, value = k, v
                values[k] = v
            end
            print(key, value, values)
            "#,
        );
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            0
        );
    }

    /// Read-only iteration variable → not reported.
    #[test]
    fn test_iter_variable_read_ok() {
        let diags = check_source("for i = 1, 10 do\n    print(i)\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            0
        );
    }

    /// Inner `local k` shadows the iteration variable: assigning to local k is not an iteration-variable reassignment.
    #[test]
    fn test_iter_variable_shadowed_local_ok() {
        let diags = check_source("for k, v in pairs({}) do\n    local k = 1\n    k = 2\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            0
        );
    }
}
