#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    #[test]
    fn test_variadic_func() {
        let mut ws = crate::VirtualWorkspace::new();
        ws.def(
            r#"
        ---@generic T, R
        ---@param call async fun(...: T...): R...
        ---@return async fun(...: T...): R...
        function async_create(call)

        end


        ---@param a number
        ---@param b string
        ---@param c boolean
        ---@return number
        function locaf(a, b, c)

        end
        "#,
        );

        let ty = ws.expr_ty("async_create(locaf)");
        let expected = ws.ty("async fun(a: number, b: string, c:boolean): number...");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_select_type() {
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
        ---@param ... string
        function ffff(...)
            a, b, c = select(2, ...)
        end
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let b_ty = ws.expr_ty("b");
        let c_ty = ws.expr_ty("c");
        let expected = ws.ty("string");
        assert_eq!(a_ty, expected);
        assert_eq!(b_ty, expected);
        assert_eq!(c_ty, expected);

        ws.def(
            r#"
        e, f = select(2, "a", "b", "c")
        "#,
        );

        let e = ws.expr_ty("e");
        let expected = LuaType::String;
        let f = ws.expr_ty("f");
        let expected_f = LuaType::String;
        assert_eq!(e, expected);
        assert_eq!(f, expected_f);

        ws.def(
            r#"
        h = select('#', "a", "b")
        "#,
        );

        let h = ws.expr_ty("h");
        let expected = LuaType::IntegerConst(2);
        assert_eq!(h, expected);
    }

    #[test]
    fn test_unpack() {
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local h ---@type number[]
        a, b, c = table.unpack(h)
        "#,
        );

        let a = ws.expr_ty("a");
        let expected = ws.ty("number?");
        let b = ws.expr_ty("b");
        let expected_b = ws.ty("number?");
        let c = ws.expr_ty("c");
        let expected_c = ws.ty("number?");
        assert_eq!(a, expected);
        assert_eq!(b, expected_b);
        assert_eq!(c, expected_c);
    }

    #[test]
    fn test_return() {
        let mut ws = crate::VirtualWorkspace::new();
        ws.def(
            r#"
                ---@class ab
                ---@field a number
                local A

                ---@generic T
                ---@param a T
                ---@return T
                local function name(a)
                    return a
                end

                local a = name(A)
                a.b = 1
                R = A.b
        "#,
        );

        let a = ws.expr_ty("R");
        let expected = ws.ty("nil");
        assert_eq!(a, expected);
    }

    #[test]
    fn test_issue_797() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
---@class Holder<T>

---@class C_StringHolder : Holder<string>

---@class C_StringHolderExt : C_StringHolder

---@class C_StringHolderWith<T> : Holder<string>

---@class C_StringHolderWithExt<T> : C_StringHolderWith<T>

---@alias A_StringHolder Holder<string>

---@alias A_StringHolderExt A_StringHolder

---@alias A_StringHolderWith<T> Holder<string>

---@alias A_StringHolderWithExt<T> A_StringHolderWith<T>

---@generic T
---@param v Holder<T>
---@return T
local function extract_holder(v) return v end

local direct ---@type Holder<string>

local class_a ---@type C_StringHolder
local class_b ---@type C_StringHolderExt
local class_c ---@type C_StringHolderWith<table>
local class_d ---@type C_StringHolderWithExt<table>

local alias_a ---@type A_StringHolder
local alias_b ---@type A_StringHolderExt
local alias_c ---@type A_StringHolderWith<table>
local alias_d ---@type A_StringHolderWithExt<table>

result = {
    direct = extract_holder(direct),

    class_a = extract_holder(class_a),
    class_b = extract_holder(class_b),
    class_c = extract_holder(class_c),
    class_d = extract_holder(class_d),

    alias_a = extract_holder(alias_a),
    alias_b = extract_holder(alias_b),
    alias_c = extract_holder(alias_c),
    alias_d = extract_holder(alias_d),
}
        "#,
        );

        let a = ws.expr_ty("result");
        let a_desc = ws.humanize_type_detailed(a);
        let expected = r#"{
    direct: string,
    class_a: string,
    class_b: string,
    class_c: string,
    class_d: string,
    alias_a: string,
    alias_b: string,
    alias_c: string,
    alias_d: string,
}"#;
        assert_eq!(a_desc, expected);
    }

    #[test]
    fn test_call_generic() {
        let mut ws = crate::VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Warp<T> T

            ---@generic T
            ---@param ... Warp<T>
            function test(...)
            end
        "#,
        );

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Warp<number>, Warp<string>
            local a, b
            test(a, b)
        "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Warp<number>, Warp<string>
            local a, b
            test--[[@<number | string>]](a, b)
        "#,
        ));
    }

    #[test]
    fn test_generic_alias_instantiation() {
        let mut ws = crate::VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Arrayable<T> T | T[]

            ---@class Suite

            ---@generic T
            ---@param value Arrayable<T>
            ---@return T[]
            function toArray(value)
            end
        "#,
        );

        ws.def(
            r#"
            ---@type Arrayable<Suite>
            local suite

            arraySuites = toArray(suite)
        "#,
        );

        let a = ws.expr_ty("arraySuites");
        let expected = ws.ty("Suite[]");
        assert_eq!(a, expected);
    }

    #[test]
    fn test_generic_alias_instantiation2() {
        let mut ws = crate::VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Arrayable<T> T | T[]

            ---@class Suite

            ---@param value Arrayable<Suite>
            function toArray(value)

            end
        "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeMismatch,
            r#"

            ---@type Suite
            local suite

            local arraySuites = toArray(suite)
            "#
        ));
    }
}
