#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use lsp_types::Uri;

    use crate::check::check_file;
    use crate::{CheckConfig, DiagnosticCode, EmmyLuaAnalysis};

    use crate::check::test::{check_source, count_by_code};

    /// Same-name class defined twice: each definition site is reported once.
    #[test]
    fn test_duplicate_class() {
        let diags = check_source("---@class Foo\nlocal Foo = {}\n---@class Foo\nlocal Bar = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// All partial: valid, not reported.
    #[test]
    fn test_duplicate_class_all_partial_ok() {
        let diags = check_source("---@class(partial) Foo\n---@class(partial) Foo\nlocal Foo = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 0);
    }

    /// Mixed partial and non-partial: reported.
    #[test]
    fn test_duplicate_class_partial_mixed() {
        let diags = check_source("---@class Foo\n---@class(partial) Foo\nlocal Foo = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// Same-name enum defined twice: reported.
    #[test]
    fn test_duplicate_enum() {
        let diags = check_source("---@enum Color\n---@enum Color");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// Same-name alias defined twice: reported.
    #[test]
    fn test_duplicate_alias() {
        let diags = check_source("---@alias Age number\n---@alias Age string");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 2);
    }

    /// Single definition: not reported.
    #[test]
    fn test_single_type_ok() {
        let diags = check_source("---@class Foo\nlocal Foo = {}");
        assert_eq!(count_by_code(&diags, DiagnosticCode::DuplicateType), 0);
    }

    #[test]
    fn duplicate_types_survive_file_edits() {
        for declaration in [
            "---@class LinkedTable",
            "---@enum LinkedTable",
            "---@alias LinkedTable table",
        ] {
            let mut analysis = EmmyLuaAnalysis::new();
            let first_uri = Uri::from_str("file:///C:/ws/first.lua").unwrap();
            let second_uri = Uri::from_str("file:///C:/ws/second.lua").unwrap();
            let file_ids = analysis.update_files_by_uri(vec![
                (first_uri.clone(), Some(declaration.to_string())),
                (second_uri.clone(), Some(declaration.to_string())),
            ]);
            let config = Arc::new(CheckConfig::new(&analysis.get_emmyrc()));
            for &file_id in &file_ids {
                let diagnostics = check_file(&analysis.semantic_model(file_id), config.clone());
                assert_eq!(
                    count_by_code(&diagnostics, DiagnosticCode::DuplicateType),
                    1
                );
            }

            for (uri, source, expected) in [
                (first_uri.clone(), declaration.to_string(), 1),
                (first_uri.clone(), format!("{declaration}\n-- edit"), 1),
                (second_uri.clone(), format!("\n{declaration}"), 1),
                (
                    second_uri.clone(),
                    declaration.replace("LinkedTable", "OtherTable"),
                    0,
                ),
                (second_uri.clone(), declaration.to_string(), 1),
            ] {
                analysis.update_file_by_uri(&uri, Some(source));
                for &file_id in &file_ids {
                    let diagnostics = check_file(&analysis.semantic_model(file_id), config.clone());
                    assert_eq!(
                        count_by_code(&diagnostics, DiagnosticCode::DuplicateType),
                        expected,
                        "{declaration}, edited {uri:?}, checked {file_id:?}"
                    );
                }
            }

            analysis.remove_file_by_uri(&second_uri);
            let diagnostics = check_file(&analysis.semantic_model(file_ids[0]), config);
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::DuplicateType),
                0
            );
        }
    }
}
