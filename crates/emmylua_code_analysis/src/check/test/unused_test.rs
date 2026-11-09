#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;

    use crate::check::test::{check_source, count_by_code};

    #[test]
    fn test_unused_local() {
        // `a` is declared but unused → Unused; `b` is used → not reported.
        let diags = check_source("local a = 1\nlocal b = 2\nprint(b)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 1);
    }

    #[test]
    fn test_underscore_prefix_ignored() {
        // `_x` prefix is treated as intentionally ignored.
        let diags = check_source("local _ignored = 1");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }

    #[test]
    fn test_used_local_ok() {
        let diags = check_source("local a = 1\nlocal b = a + 1\nprint(b)");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }

    #[test]
    fn test_used_varargs_forwarded() {
        let diags = check_source(
            r#"
            local function forward(...)
                print(...)
            end
            local function identity(...)
                return ...
            end
            forward(identity('hello'))
            "#,
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }

    #[test]
    fn test_used_varargs_packed() {
        let diags = check_source(
            r#"
            local function pack(...)
                return { ... }
            end
            local function pack_with_count(...)
                return table.pack(...)
            end
            print(pack('hello'), pack_with_count('hello'))
            "#,
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }

    #[test]
    fn test_used_varargs_counted() {
        let diags = check_source(
            r#"
            local function count(...)
                return select('#', ...)
            end
            print(count('hello'))
            "#,
        );
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }

    #[test]
    fn test_unused_varargs_reported() {
        let source = r#"
            local function ignore(...)
                print('hello')
            end
            ignore('hello')
        "#;
        let diags = check_source(source);
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.code == DiagnosticCode::Unused)
            .collect();
        assert_eq!(unused.len(), 1);
        assert_eq!(
            usize::from(unused[0].range.start()),
            source.find("...").unwrap()
        );
        assert_eq!(unused[0].range.len(), rowan::TextSize::from(3));
        assert_eq!(
            unused[0].message,
            "... is never used, if this is intentional, prefix it with an underscore: _..."
        );
    }

    #[test]
    fn test_used_inner_varargs_do_not_use_outer_varargs() {
        let source = r#"
            local function outer(...)
                local function inner(...)
                    print(...)
                end
                inner('hello')
            end
            outer('ignored')
        "#;
        let diags = check_source(source);
        let unused_offsets: Vec<_> = diags
            .iter()
            .filter(|d| d.code == DiagnosticCode::Unused)
            .map(|d| usize::from(d.range.start()))
            .collect();
        assert_eq!(unused_offsets, vec![source.find("...").unwrap()]);
    }

    #[test]
    fn test_used_outer_varargs_do_not_use_inner_varargs() {
        let source = r#"
            local function outer(...)
                local function inner(...)
                    print('hello')
                end
                inner()
                print(...)
            end
            outer('hello')
        "#;
        let diags = check_source(source);
        let unused_offsets: Vec<_> = diags
            .iter()
            .filter(|d| d.code == DiagnosticCode::Unused)
            .map(|d| usize::from(d.range.start()))
            .collect();
        assert_eq!(unused_offsets, vec![source.find("inner(...)").unwrap() + 6]);
    }

    #[test]
    fn test_invalid_inner_varargs_do_not_use_outer_varargs() {
        let source = r#"
            local function outer(...)
                local function inner()
                    print(...)
                end
                inner()
            end
            outer('ignored')
        "#;
        let diags = check_source(source);
        let unused_offsets: Vec<_> = diags
            .iter()
            .filter(|d| d.code == DiagnosticCode::Unused)
            .map(|d| usize::from(d.range.start()))
            .collect();
        assert_eq!(unused_offsets, vec![source.find("...").unwrap()]);
        let syntax_errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code == DiagnosticCode::SyntaxError)
            .map(|d| (usize::from(d.range.start()), d.message.as_str()))
            .collect();
        assert_eq!(
            syntax_errors,
            vec![(
                source.rfind("...").unwrap(),
                "Cannot use `...` outside a vararg function."
            )]
        );
    }

    #[test]
    fn test_chunk_varargs_do_not_add_diagnostics() {
        let diags = check_source(
            r#"
            local args = { ... }
            return args
            "#,
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn test_unused_implicit_self_reports_on_colon() {
        // The implicit `self` of `function C:foo()` is never used. The diagnostic should
        // target the colon so editors grey out the colon, not the method name.
        let diags = check_source("local C = {}\nfunction C:foo() end\n");
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
        let diags = check_source("local C = {}\nfunction C:bar() return self end\n");
        assert_eq!(count_by_code(&diags, DiagnosticCode::Unused), 0);
    }
}
