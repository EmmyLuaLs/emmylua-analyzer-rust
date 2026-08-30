//! Tests for the global_non_module checker (mirrors the old `diagnostic/test/global_in_non_module_test.rs`).

#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::{check_source, count_by_code};

    /// Assigning to an undefined global inside a nested closure → reported.
    #[test]
    fn test_global_in_non_module() {
        let diags = check_source(
            r#"
            local function name()
                bbbb = 123
            end
        "#,
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::GlobalInNonModule), 1);
    }

    /// Assigning to a global at module top level → not reported.
    #[test]
    fn test_global_at_chunk_top_level_ok() {
        let diags = check_source("bbbb = 123");
        assert_eq!(count_by_code(&diags, DiagnosticCode::GlobalInNonModule), 0);
    }
}
