#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test() {
        let mut ws = VirtualWorkspace::new();
        // 作用域不同
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::DuplicateRequire,
            r#"
            if true then
                require("a")
            else
                require("a")
            end
            "#,
        ));

        // 父作用域已存在
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::DuplicateRequire,
            r#"
            require("a")
            if true then
                require("a")
            else
                require("a")
            end
            "#,
        ));
    }

    #[test]
    fn test_field() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::DuplicateRequire,
            r#"
            require("a").a
            require("a")
            "#,
        ));
    }
}
