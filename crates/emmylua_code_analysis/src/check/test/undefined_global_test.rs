#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    #[test]
    fn test_undefined_global() {
        // `missing` is undefined; `print` is builtin, `x` is local, and `global_defined` is a workspace global, so none of those count.
        let diags = super::super::check_source(
            "local x = 1\nlocal y = missing\nprint(x)\nglobal_defined = 1\nlocal z = global_defined",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedGlobal), 1);
    }

    #[test]
    fn test_definition_position_not_undefined() {
        // Assignment target (definition site) is not undefined.
        let diags = super::super::check_source("foo = 1\nlocal y = foo");
        assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedGlobal), 0);
    }

    #[test]
    fn test_all_defined_ok() {
        let diags = super::super::check_source("local a = 1\nprint(a)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedGlobal), 0);
    }

    /// `emmyrc.diagnostics.globals` whitelist: configured global names are not reported.
    #[test]
    fn test_globals_config_whitelist() {
        let mut emmyrc = crate::Emmyrc::default();
        emmyrc.diagnostics.globals.push("my_global".to_string());
        let diags = super::super::check_source_with_emmyrc("local a = my_global", emmyrc);
        assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedGlobal), 0);
    }

    /// `emmyrc.diagnostics.globals_regex`: names matching the regex are not reported.
    #[test]
    fn test_globals_regex_config_whitelist() {
        let mut emmyrc = crate::Emmyrc::default();
        emmyrc
            .diagnostics
            .globals_regex
            .push("^m[0-9]+$".to_string());
        let diags = super::super::check_source_with_emmyrc("local a = m42\nlocal b = n42", emmyrc);
        // m42 matches the regex; n42 does not.
        assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedGlobal), 1);
    }
}
