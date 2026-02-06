#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    #[test]
    fn test_string() {
        let mut ws = VirtualWorkspace::new();

        let string_ty = ws.ty("string");

        let right_ty = ws.ty("'ssss'");
        assert!(ws.check_type(&string_ty, &right_ty));

        let right_ty = ws.ty("number");
        assert!(!ws.check_type(&string_ty, &right_ty));

        let right_ty = ws.ty("string | number");
        assert!(!ws.check_type(&string_ty, &right_ty));

        let right_ty = ws.ty("'a' | 'b' | 'c'");
        assert!(ws.check_type(&string_ty, &right_ty));
    }

    #[test]
    fn test_number_types() {
        let mut ws = VirtualWorkspace::new();

        let number_ty = ws.ty("number");
        let integer_ty = ws.ty("integer");

        let number_expr1 = ws.expr_ty("1");
        assert!(ws.check_type(&number_ty, &number_expr1));
        let number_expr2 = ws.expr_ty("1.5");
        assert!(ws.check_type(&number_ty, &number_expr2));

        assert!(ws.check_type(&number_ty, &integer_ty));
        assert!(!ws.check_type(&integer_ty, &number_ty));

        let number_union = ws.ty("1 | 2 | 3");
        assert!(ws.check_type(&number_ty, &number_union));
        assert!(ws.check_type(&integer_ty, &number_union));
    }

    #[test]
    fn test_union_types() {
        let mut ws = VirtualWorkspace::new();

        let ty_union = ws.ty("number | string");
        let ty_number = ws.ty("number");
        let ty_string = ws.ty("string");
        let ty_boolean = ws.ty("boolean");

        assert!(ws.check_type(&ty_union, &ty_number));
        assert!(ws.check_type(&ty_union, &ty_string));
        assert!(!ws.check_type(&ty_union, &ty_boolean));
        assert!(ws.check_type(&ty_union, &ty_union));

        let ty_union2 = ws.ty("number | string | boolean");
        assert!(ws.check_type(&ty_union2, &ty_number));
        assert!(ws.check_type(&ty_union2, &ty_string));
        assert!(ws.check_type(&ty_union2, &ty_union));
        assert!(ws.check_type(&ty_union2, &ty_union2));

        let ty_union3 = ws.ty("1 | 2 | 3");
        let ty_union4 = ws.ty("1 | 2");

        assert!(ws.check_type(&ty_union3, &ty_union4));
        assert!(!ws.check_type(&ty_union4, &ty_union3));
        assert!(ws.check_type(&ty_union3, &ty_union3));
    }

    #[test]
    fn test_object_types() {
        let mut ws = VirtualWorkspace::new();

        // case 1
        {
            let object_ty = ws.ty("{ x: number, y: string }");
            let matched_object_ty2 = ws.ty("{ x: 1, y: 'test' }");
            let mismatch_object_ty2 = ws.ty("{ x: 2, y: 3 }");
            let matched_table_ty = ws.expr_ty("{ x = 1, y = 'test' }");
            let mismatch_table_ty = ws.expr_ty("{ x = 2, y = 3 }");

            assert!(ws.check_type(&object_ty, &matched_object_ty2));
            assert!(!ws.check_type(&object_ty, &mismatch_object_ty2));
            assert!(ws.check_type(&object_ty, &matched_table_ty));
            assert!(!ws.check_type(&object_ty, &mismatch_table_ty));
        }

        // case for tuple, object, and table
        {
            let object_ty = ws.ty("{ [1]: string, [2]: number }");
            let matched_tulple_ty = ws.ty("[string, number");
            let matched_object_ty = ws.ty("{ [1]: 'test', [2]: 1 }");

            assert!(ws.check_type(&object_ty, &matched_tulple_ty));
            assert!(ws.check_type(&object_ty, &matched_object_ty));
            let mismatch_tulple_ty = ws.ty("[number, string]");
            assert!(!ws.check_type(&object_ty, &mismatch_tulple_ty));

            let matched_table_ty = ws.expr_ty("{ [1] = 'test', [2] = 1 }");
            assert!(ws.check_type(&object_ty, &matched_table_ty));
        }

        // issue #69
        {
            let object_ty = ws.ty("{ [1]: number, [2]: integer }?");

            assert!(ws.check_type(&object_ty, &object_ty));
        }
    }

    #[test]
    fn test_array_types() {
        let mut ws = VirtualWorkspace::new();

        let array_ty = ws.ty("number[]");
        let matched_tuple_ty = ws.ty("[1, 2, 3]");
        let mismatch_array_ty = ws.ty("['a', 'b', 'c']");

        assert!(ws.check_type(&array_ty, &matched_tuple_ty));
        assert!(!ws.check_type(&array_ty, &mismatch_array_ty));

        let array_ty2 = ws.ty("integer[]");
        assert!(ws.check_type(&array_ty, &array_ty2));
        assert!(!ws.check_type(&array_ty2, &array_ty));
    }

    #[test]
    fn test_tuple_types() {
        let mut ws = VirtualWorkspace::new();

        let tuple_ty = ws.ty("[number, string]");
        let matched_tuple_ty = ws.ty("[1, 'test']");
        let mismatch_tuple_ty = ws.ty("['a', 1]");

        assert!(ws.check_type(&tuple_ty, &matched_tuple_ty));
        assert!(!ws.check_type(&tuple_ty, &mismatch_tuple_ty));

        let tuple_ty2 = ws.ty("[integer, string]");
        assert!(ws.check_type(&tuple_ty, &tuple_ty2));
        assert!(!ws.check_type(&tuple_ty2, &tuple_ty));
    }

    #[test]
    fn test_issue_86() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let ty = ws.ty("string?");
        let ty2 = ws.expr_ty("(\"hello\"):match(\".*\")");
        assert!(ws.check_type(&ty, &ty2));
    }

    #[test]
    fn test_issue_634() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            --- @class A
            --- @field a integer

            --- @param x table<integer,string>
            local function foo(x) end

            local y --- @type A
            foo(y) -- should error
        "#
        ));
    }

    #[test]
    fn test_issue_790() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Holder<T>

        ---@class StringHolder: Holder<string>

        ---@class NumberHolder: Holder<number>

        ---@class StringHolderWith<T>: Holder<string>

        ---@generic T
        ---@param a T
        ---@param b T
        function test(a, b) end
        "#,
        );

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Holder<string>, NumberHolder
            local a, b
            test(a, b)
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Holder<string>, StringHolderWith<table>
            local a, b
            test(a, b)
        "#
        ));
    }

    #[test]
    fn test_set_index_expr_owner_prefers_declared_global_type() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def_file(
            "def.lua",
            r#"
            table = table

            ---@cast table unknown
            AFTER_CAST = table

            ---@return integer
            function table.__sentinel()
                return 1
            end
            "#,
        );

        assert_eq!(ws.expr_ty("AFTER_CAST"), LuaType::Unknown);
        assert_eq!(ws.expr_ty("table.__sentinel()"), ws.ty("integer"));
    }
}
