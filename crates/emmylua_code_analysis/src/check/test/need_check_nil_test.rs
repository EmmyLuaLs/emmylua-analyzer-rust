#[cfg(test)]
mod test {
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    use crate::{DiagnosticCode, VirtualWorkspace};

    fn check_class_array_guard(element_type: &str, body: &str, expected_codes: &[DiagnosticCode]) {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.strict.array_index = true;
        ws.update_emmyrc(emmyrc);
        let file_id = ws.def(&format!(
            r#"
            ---@class Container
            ---@field items {element_type}[]
            local _Container = {{}}

            ---@param t Container
            ---@param _other Container
            ---@param _index integer
            local function read(t, _other, _index)
                {body}
            end
            read({{ items = {{ 'hello' }} }}, {{ items = {{}} }}, 1)
            "#,
        ));
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .expect("diagnostics");
        let expected = expected_codes
            .iter()
            .map(|code| Some(NumberOrString::String(code.get_name().to_string())))
            .collect::<Vec<_>>();
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>(),
            expected,
            "{body}: {diagnostics:#?}"
        );
    }

    #[test]
    fn test_class_array_element_guards() {
        for body in [
            "if t.items[1] then print(t.items[1]:sub(1)) end",
            "if not t.items[1] then return end; print(t.items[1]:sub(1))",
            "if t.items[1] ~= nil then print(t.items[1]:sub(1)) end",
            "if t.items[1] == nil then return end; print(t.items[1]:sub(1))",
            "if t.items[1] then t.items[2] = 'other'; print(t.items[1]:sub(1)) end",
            "t.items = {}; if t.items[1] then print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[1] = t.items[1]:sub(1) end",
            "t.items[1] = t.items[2]; if t.items[1] then print(t.items[1]:sub(1)) end",
        ] {
            check_class_array_guard("string", body, &[]);
        }
    }

    #[test]
    fn test_class_array_element_guard_writes() {
        for body in [
            "if t.items[1] then t.items[1] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[1.0] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[1e0] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items = {}; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t = _other; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[_index] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then if _index == 1 then t.items = {} end; print(t.items[1]:sub(1)) end",
        ] {
            check_class_array_guard(
                "string",
                body,
                &[
                    DiagnosticCode::ParamTypeMismatch,
                    DiagnosticCode::NeedCheckNil,
                ],
            );
        }
    }

    #[test]
    fn test_class_array_element_guard_scope() {
        for body in [
            "print(t.items[1]:sub(1))",
            "if t.items[1] then print(t.items[2]:sub(1)) end",
            "if t.items[1] then end; print(t.items[1]:sub(1))",
            "if t.items[1] then print(_other.items[1]:sub(1)) end",
            "if t.items[1] then local function later() print(t.items[1]:sub(1)) end; t.items = {}; later() end",
            "if (t.items[1] ~= nil) ~= nil then print(t.items[1]:sub(1)) end",
            "if (not not t.items[1]) ~= nil then print(t.items[1]:sub(1)) end",
        ] {
            check_class_array_guard(
                "string",
                body,
                &[
                    DiagnosticCode::ParamTypeMismatch,
                    DiagnosticCode::NeedCheckNil,
                ],
            );
        }
        check_class_array_guard(
            "string",
            "if t.items[1] then local t = _other; print(t.items[1]:sub(1)) end",
            &[
                DiagnosticCode::RedefinedLocal,
                DiagnosticCode::ParamTypeMismatch,
                DiagnosticCode::NeedCheckNil,
            ],
        );
    }

    #[test]
    fn test_class_array_element_guard_preserves_assignment_type() {
        for (value, expected) in [
            ("'hello'", &[] as &[DiagnosticCode]),
            ("1", &[DiagnosticCode::AssignTypeMismatch]),
        ] {
            check_class_array_guard(
                "(string|number)",
                &format!(
                    r#"
                    t.items[1] = {value}
                    if t.items[1] then
                        ---@type string
                        local _s = t.items[1]
                    end
                    "#,
                ),
                expected,
            );
        }
    }

    #[test]
    fn test_issue_245() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        local a --- @type table?
        local _ = (a and a.type == 'change') and a.field
        "#
        ));
    }
    #[test]
    fn test_issue_402() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@class A
            local a = {}

            ---@param self table?
            function a.new(self)
                if self then
                    self.a = 1
                end
            end
        "#
        ));
    }

    #[test]
    fn test_issue_474() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@class Range4
            ---@class TSNode: userdata
            ---@field range fun(self: TSNode): Range4

            ---@param node_or_range TSNode|Range4
            ---@return Range4
            function foo(node_or_range)
                if type(node_or_range) == 'table' then
                    return node_or_range
                else
                    return node_or_range:range()
                end
            end
            "#
        ));
    }

    #[test]
    fn test_cast() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Cast1
            ---@field get fun(self: self, a: number): Cast1?
            ---@field get2 fun(self: self, a: number): Cast1?
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
                ---@type Cast1
                local A

                local a = A:get(1) --[[@cast -?]]
                    :get2(2)
            "#
        ));
    }

    #[test]
    fn test_issue_895_891() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        local t = {
        123,
        234,
        345,
        }

        ---@param id number
        function test(id) end

        for i = 1, #t do
            test(t[i]) -- expected 'number' but found (123|234|345)?
        end
        "#,
        ));
    }

    #[test]
    fn test_issue_886() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        ---@type string[]
        local a = {}

        -- if #a == 0 then return end
        if not a[1] then return end

        -- ---@type string
        -- local s = a[1]

        ---@type string
        local s = a[#a]
        "#,
        ));
    }

    #[test]
    fn test_asserted_index_assignment_prefix_is_not_nil() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type integer[][]
        local res
        res[1][1] = assert(res[1])[2]
        "#,
        ));
    }

    #[test]
    fn test_numeric_for_len_expr_narrows_loop_body_value_to_non_nil() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type false|fun(...)[]?
        local calls

        for i = 1, #calls do
            calls[i](...)
        end
        "#,
        ));
    }

    #[test]
    fn test_different_asserted_index_assignment_prefix_still_needs_nil_check() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type integer[][]
        local res
        res[1][1] = assert(res[2])[2]
        "#,
        ));
    }

    #[test]
    fn test_and_narrows_after_merge_of_earlier_guard() {
        // After `if prev and ok()` merges back to `St?`, the later `prev and prev.sig` still
        // narrows its right side, even when an unguarded read of `prev` memoized the merge
        // first. Needs the std library and every check: with less, nothing reads `prev`
        // before the guarded read, and the test passes without the fix.
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@class St
            ---@field sig string

            ---@return St?
            local function decode() end

            ---@return boolean
            local function ok() return true end

            local prev = decode()
            if prev and ok() then end
            print(prev and prev.sig)
        "#,
        );
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap_or_default();
        let code = Some(NumberOrString::String(
            DiagnosticCode::NeedCheckNil.get_name().to_string(),
        ));
        assert!(
            !diagnostics.iter().any(|diag| diag.code == code),
            "{diagnostics:#?}"
        );
    }
}
