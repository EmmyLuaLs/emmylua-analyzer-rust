#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    #[test]
    fn test_duplicate_require() {
        let diags =
            super::super::check_source("local a = require('foo')\nlocal b = require('foo')");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateRequire), 1);
    }

    #[test]
    fn test_distinct_requires_ok() {
        let diags =
            super::super::check_source("local a = require('foo')\nlocal b = require('bar')");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateRequire), 0);
    }
}
