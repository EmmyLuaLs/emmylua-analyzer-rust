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

    #[test]
    fn test_unused_implicit_self_reports_on_colon() {
        // The implicit `self` of `function C:foo()` is never used. The diagnostic should
        // target the colon so editors grey out the colon, not the method name.
        let diags = super::super::check_source("local C = {}\nfunction C:foo() end\n");
        let unused = diags
            .iter()
            .find(|d| d.code == DiagnosticCode::Unused)
            .expect("expected unused self diagnostic");
        assert_eq!(unused.range.start(), rowan::TextSize::from(23));
        assert_eq!(unused.range.end(), rowan::TextSize::from(24));
        assert!(unused.message.contains("Implicit self"));
    }

    #[test]
    fn test_used_implicit_self_no_diagnostic() {
        let diags = super::super::check_source(
            "local C = {}\nfunction C:bar() return self end\n",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }
}
