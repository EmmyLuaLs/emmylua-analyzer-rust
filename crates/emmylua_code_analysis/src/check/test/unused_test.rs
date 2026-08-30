#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    #[test]
    fn test_unused_local() {
        // `a` is declared but unused → Unused; `b` is used → not reported.
        let diags = super::super::check_source("local a = 1\nlocal b = 2\nprint(b)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 1);
    }

    #[test]
    fn test_underscore_prefix_ignored() {
        // `_x` prefix is treated as intentionally ignored.
        let diags = super::super::check_source("local _ignored = 1");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }

    #[test]
    fn test_used_local_ok() {
        let diags = super::super::check_source("local a = 1\nlocal b = a + 1\nprint(b)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }
}
