#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_195() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
            local ret = {} --- @type string[]
            local b
            ret[#ret + 1] = b
            ret[#ret + 1] = 'a'
        "#
        ));
    }

    #[test]
    fn test_inject_field() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
            ---@class test1

            ---@type test1
            local test
            test.a = 1

        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
            ---@class test2
            ---@field a number

            ---@type test2
            local test
            test.a = 1

        "#
        ));
    }

    #[test]
    fn test_super_table() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
            ---@class test1<T>: {[string]: number }, table<string, string>

            ---@type test1<string>
            local test

            test.a = "1"
        "#
        ));
    }

    #[test]
    fn test_object() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
            ---@type { [number]: number }
            local test2 = {
            }
            test2.a = 1
        "#
        ));
    }

    #[test]
    fn test_self() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
            ---@class Diagnostic.8_1
            ---@field a number
            local Test = {}

            function Test:name()
                self.a = 1
            end
        "#
        ));
    }

    #[test]
    fn test_any_key() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
            ---@type { [number]: number }
            local t

            t[any] = 1
        "#
        ));
    }

    #[test]
    fn test_issue_264() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
                local a = { 'a' }
                a[#a + 1] = 'b'

                ---@type string[]
                local b = { 'a' }
                b[#b + 1] = 'b'

                ---@type table<integer, string>
                local c = { 'a' }
                c[#c + 1] = 'b'

                ---@type { [integer]: string }
                local d = { 'a' }
                d[#d + 1] = 'b'
        "#
        ));
    }

    #[test]
    fn test_tuple() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
                local a = { 'a' }
                a[#a + 1] = 'b'
        "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::InjectField,
            r#"
                ---@type [ 'a' ] 
                local a = { 'a' } 
                a[#a + 1] = 'b'
        "#
        ));
    }
}
