#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    #[test]
    fn test_const_reassign() {
        // const a is reassigned → LocalConstReassign.
        let diags = super::super::check_source("local a <const> = 1\na = 2");
        assert_eq!(count_by_code(&diags, DiagnosticCode::LocalConstReassign), 1);
    }

    #[test]
    fn test_const_not_reassigned_ok() {
        let diags = super::super::check_source("local a <const> = 1\nprint(a)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::LocalConstReassign), 0);
    }

    /// Reassigning `i` in `for i = 1, 10 do` → IterVariableReassign (enabled by default in Lua 5.5).
    #[test]
    fn test_iter_variable_reassign_for_range() {
        let diags = super::super::check_source("for i = 1, 10 do\n    i = 2\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            1
        );
    }

    /// Reassigning an iteration variable in `for k, v in pairs(t) do` → IterVariableReassign.
    #[test]
    fn test_iter_variable_reassign_for_stat() {
        let diags = super::super::check_source("for k, v in pairs({}) do\n    k = 1\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            1
        );
    }

    /// Read-only iteration variable → not reported.
    #[test]
    fn test_iter_variable_read_ok() {
        let diags = super::super::check_source("for i = 1, 10 do\n    print(i)\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            0
        );
    }

    /// Inner `local k` shadows the iteration variable: assigning to local k is not an iteration-variable reassignment.
    #[test]
    fn test_iter_variable_shadowed_local_ok() {
        let diags =
            super::super::check_source("for k, v in pairs({}) do\n    local k = 1\n    k = 2\nend");
        assert_eq!(
            count_by_code(&diags, DiagnosticCode::IterVariableReassign),
            0
        );
    }
}
