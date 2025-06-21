#[cfg(test)]
mod tests {
    use crate::DiagnosticCode;
    use crate::VirtualWorkspace;

    #[test]
    fn test_valid_cast() {
        let mut ws = VirtualWorkspace::new();
        let code = r#"
---@cast a number
---@cast a.field string
---@cast A.b.c.d boolean
---@cast -?
        "#;

        assert!(ws.check_code_for(DiagnosticCode::CastTypeMismatch, code));
    }

    #[test]
    fn test_invalid_cast() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type string|boolean
            A = "1"
            "#,
        );
        assert!(!ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@cast A number
            "#
        ));
    }

    #[test]
    fn test_valid_cast_from_union_to_member() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type string|number|boolean
            local value

            ---@cast value string
            "#
        ));
    }

    #[test]
    fn test_invalid_cast_to_non_member() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type string|boolean
            local value

            ---@cast value table
            "#
        ));
    }

    #[test]
    fn test_cast_with_nil() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type string?
            local value

            ---@cast value string
            "#
        ));
    }

    #[test]
    fn test_cast_same_type() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type string
            local value

            ---@cast value string
            "#
        ));
    }

    #[test]
    fn test_cast_multiple_operations() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type string|boolean
            local value

            ---@cast value +number, -boolean
            "#
        ));
    }

    #[test]
    fn test_cast_class_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Animal
            ---@class Dog : Animal
            "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type Animal
            local pet

            ---@cast pet Dog
            "#
        ));
    }

    #[test]
    fn test_cast_invalid_class_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Animal
            ---@class Car
            "#,
        );
        assert!(!ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type Animal
            local pet

            ---@cast pet Car
            "#
        ));
    }

    #[test]
    fn test_cast_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Animal.Dog
            "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
            ---@type any
            local pet

            ---@cast pet Animal.Dog
            "#
        ));
    }

    #[test]
    fn test_cast_alias_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
                ---@alias KV.SupportType
                ---| boolean
                ---| integer
                ---| number
                ---| string


                ---@param value KV.SupportType
                ---@return any
                ---@return string
                local function get_py_value_and_type(value)
                    local tp = type(value)
                    if tp == 'number' then
                        ---@cast value number
                        return value, math.type(value)
                    end
                    return value, tp
                end 
            "#
        ));
    }

    #[test]
    fn test_cast_alias_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
                ---@alias KeyAlias
                ---| "a" # 2010001
                ---| "b" # 2010002

                ---@type string
                local key

                ---@cast key KeyAlias
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
                ---@alias IdAlias
                ---| 2010001
                ---| 2010002

                ---@type string
                local key

                ---@cast key IdAlias
            "#
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::CastTypeMismatch,
            r#"
                ---@alias IdAndKeyAlias IdAlias|KeyAlias

                ---@type string
                local key

                ---@cast key IdAndKeyAlias
            "#
        ));
    }
}
