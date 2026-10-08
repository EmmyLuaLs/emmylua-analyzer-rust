#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use crate::check::test::{check_source, count_by_code};

    #[test]
    fn test_parser_error() {
        let diags = check_source("local = =");
        assert_eq!(count_by_code(&diags, DiagnosticCode::SyntaxError), 1);
    }

    #[test]
    fn test_invalid_unicode_escape() {
        let diags = check_source("local s = '\\u{110000}'");
        assert_eq!(count_by_code(&diags, DiagnosticCode::SyntaxError), 1);
    }

    #[test]
    fn test_vararg_outside_vararg_function() {
        let diags = check_source("function f() return ... end");
        assert_eq!(count_by_code(&diags, DiagnosticCode::SyntaxError), 1);
    }

    #[test]
    fn test_vararg_inside_vararg_function_ok() {
        let diags = check_source("function f(...) return ... end");
        assert_eq!(count_by_code(&diags, DiagnosticCode::SyntaxError), 0);
    }

    #[test]
    fn test_goto_undefined_label() {
        let diags = check_source("goto missing");
        assert_eq!(count_by_code(&diags, DiagnosticCode::SyntaxError), 1);
    }

    #[test]
    fn test_goto_valid_label_ok() {
        let diags = check_source("goto ok\n::ok::");
        assert_eq!(count_by_code(&diags, DiagnosticCode::SyntaxError), 0);
    }
}
