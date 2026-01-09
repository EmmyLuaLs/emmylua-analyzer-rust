#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, Emmyrc, VirtualWorkspace};

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        let mut config = Emmyrc::default();
        config.strict.array_index = true;
        ws.analysis.update_config(config.into());

        assert!(ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            ---@type boolean
            local b
            assert(b)

            ---@type any
            local c
            assert(c)

            ---@type unknown
            local d
            assert(d)

            ---@type boolean
            local e
            assert(e)

            ---@type number?
            local f
            assert(f)

            ---@type integer[]
            local ints = {1, 2}
            assert(ints[3])

            ---@type [integer, integer]
            local enum = {1, 2}
            assert(enum[3])
        "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            assert(true)
        "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            ---@return integer
            local function hi()
              return 1
            end
            assert(hi(1))
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            assert({}, 'hi')
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            ---@type [integer, integer]
            local enum = {1, 2}
            assert(enum[2])
            "#
        ));
    }

    #[test]
    fn test_impossible_assert() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            assert(false)
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            assert(nil)
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            assert(nil and 5)
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            local a = false ---@type false
            assert(a)
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            ---@type integer[]
            local a = {1,2,3}
            local b = a[2] ---@type integer|nil
            if not b then
              assert(b, "No second element!")
            end
            "#
        ));
    }

    #[test]
    fn test_nonnli_with_nil() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            assert("abc" ~= nil)
            "#
        ));
        assert!(!ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            assert(1 == nil)
            "#
        ));
    }
}
