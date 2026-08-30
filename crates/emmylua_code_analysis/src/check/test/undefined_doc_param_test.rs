#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    #[test]
    fn test_undefined_doc_param() {
        // `@param b` but the function only has parameter a → UndefinedDocParam.
        let diags = super::super::check_source(
            "---@param a number\n---@param b number\nfunction bar(a) end",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedDocParam), 1);
    }

    #[test]
    fn test_matching_doc_params_ok() {
        let diags = super::super::check_source(
            "---@param a number\n---@param b number\nfunction bar(a, b) end",
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::UndefinedDocParam), 0);
    }
}
