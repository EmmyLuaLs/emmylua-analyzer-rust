#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use super::super::count_by_code;

    /// Unknown doc tags are disabled by default: not reported unless explicitly enabled.
    #[test]
    fn test_unknown_doc_tag_disabled_by_default() {
        let diags = super::super::check_source("---@foobar\nfunction bar() end");
        assert_eq!(count_by_code(&diags, DiagnosticCode::UnknownDocTag), 0);
    }

    /// After explicitly enabling: unknown doc tags are reported.
    #[test]
    fn test_unknown_doc_tag() {
        let mut emmyrc = crate::Emmyrc::default();
        emmyrc
            .diagnostics
            .enables
            .push(DiagnosticCode::UnknownDocTag);
        let diags =
            super::super::check_source_with_emmyrc("---@foobar\nfunction bar() end", emmyrc);
        assert_eq!(count_by_code(&diags, DiagnosticCode::UnknownDocTag), 1);
    }

    /// `emmyrc.doc.known_tags` whitelist: known tags are not reported.
    #[test]
    fn test_known_doc_tag_ok() {
        let mut emmyrc = crate::Emmyrc::default();
        emmyrc.doc.known_tags.push("foobar".to_string());
        let diags =
            super::super::check_source_with_emmyrc("---@foobar\nfunction bar() end", emmyrc);
        assert_eq!(count_by_code(&diags, DiagnosticCode::UnknownDocTag), 0);
    }
}
