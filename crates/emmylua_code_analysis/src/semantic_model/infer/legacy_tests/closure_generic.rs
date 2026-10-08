#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_generic() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.has_no_diagnostic(
            DiagnosticCode::TypeNotFound,
            r#"
        --- @generic T
        --- @param ... T
        --- @return T
        return function (...) end
        "#,
        );
    }

    #[test]
    fn test_issue_240() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @generic T
        --- @param fn fun(...:T...)
        --- @return fun(...:T...)
        local function wrap(fn)
            return fn
        end

        --- @param a integer
        foo = wrap(function(a)
        end) -- type unknown

        --- @param a integer
        bar = wrap(function(a)
            _ = a
        end) -- type fun(a: integer)
        "#,
        );

        let foo = ws.expr_ty("foo");
        let foo_desc = ws.humanize_type(foo);
        assert_eq!(foo_desc, "fun(a: integer)");

        let bar = ws.expr_ty("bar");
        let bar_desc = ws.humanize_type(bar);
        assert_eq!(bar_desc, "fun(a: integer)");
    }

    #[test]
    fn test_issue_241() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @generic T
        --- @param fn fun(...:T...)
        --- @return fun(...:T...)
        local function wrap(fn) return fn end

        E = {}

        --- @param a integer
        E.foo = function(a) _ = a end -- type fun(a: integer) - correct

        --- @param a integer
        E.foo_wrapped = wrap(function(a) _ = a end) -- type fun(a: integer) - correct
        E.foo_wrapped2_a = wrap(wrap(E.foo))        -- type fun(a: integer) - correct
        E.foo_wrapped2_b = wrap(E.foo_wrapped)      -- type fun() - wrong
        "#,
        );

        let foo_wrapper2_b = ws.expr_ty("E.foo_wrapped2_b");
        let desc = ws.humanize_type(foo_wrapper2_b);
        assert_eq!(desc, "fun(a: integer)");
    }
}
