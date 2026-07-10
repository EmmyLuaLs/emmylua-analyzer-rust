#[cfg(test)]
mod test {
    use crate::{
        DiagnosticCode, LuaType, RenderLevel, TypeSubstitutor, VirtualWorkspace, humanize_type,
    };

    #[test]
    fn test_variadic_func() {
        let mut ws = VirtualWorkspace::new();
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
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
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

        // select(n, func()) where func() returns multiple values
        ws.def(
            r#"
        ---@return integer, string
        function multi_ret() end
        g = select(2, multi_ret())
        "#,
        );

        let g = ws.expr_ty("g");
        let expected_g = ws.ty("string");
        assert_eq!(g, expected_g);
    }

    #[test]
    fn test_unpack() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

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
        let mut ws = VirtualWorkspace::new();
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
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Warp<T> T

            ---@generic T
            ---@param ... Warp<T>
            function test(...)
            end
        "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Warp<number>, Warp<string>
            local a, b
            test(a, b)
        "#,
        ));

        assert!(ws.has_no_diagnostic(
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
        let mut ws = VirtualWorkspace::new();
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
    fn test_keyof_generic_instantiates_to_union() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class A
            ---@field one 1
            ---@field two 2
            ---@field three 3

            ---@alias B<T> T extends any and keyof T or never
            "#,
        );

        let ty = ws.ty("B<A>");
        let db = ws.analysis.compilation.get_db();
        let origin = match ty {
            LuaType::Generic(generic) => {
                let type_decl = db
                    .get_type_index()
                    .get_type_decl(&generic.get_base_type_id())
                    .expect("B must resolve to an alias declaration");
                let substitutor = TypeSubstitutor::from_type_array(generic.get_params().clone());
                type_decl
                    .get_alias_origin(&db, Some(&substitutor))
                    .expect("B<A> must expand to its instantiated alias origin")
            }
            ty => ty,
        };

        let LuaType::Union(union) = &origin else {
            panic!(
                "keyof generic should instantiate to union, got {}",
                humanize_type(&db, &origin, RenderLevel::Detailed)
            );
        };

        let mut keys = union
            .into_vec()
            .iter()
            .map(|ty| humanize_type(&db, ty, RenderLevel::Brief))
            .collect::<Vec<_>>();
        keys.sort();

        assert_eq!(keys, vec!["\"one\"", "\"three\"", "\"two\""]);
    }

    #[test]
    fn test_generic_alias_instantiation2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Arrayable<T> T | T[]

            ---@class Suite

            ---@param value Arrayable<Suite>
            function toArray(value)

            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"

            ---@type Suite
            local suite

            local arraySuites = toArray(suite)
            "#
        ));
    }

    #[test]
    fn test_dot_defined_generic_constructor_called_with_colon() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class a
            local a = {}

            ---@generic T
            ---@param cls T
            ---@return T
            function a.create(cls)
                local instance = setmetatable({}, cls)
                return instance
            end

            b = a:create()
            "#,
        );

        let ty = ws.expr_ty("b");
        assert_eq!(ws.humanize_type(ty), "a");
    }

    #[test]
    fn test_generic_map_lambda_return() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T, U
            ---@param list T[]
            ---@param fn fun(item: T): U
            ---@return U[]
            local function map(list, fn)
            end

            local list_1 = {} ---@type string[]

            _mapped_2 = map(list_1, function (item)
                return item
            end)
        "#,
        );

        let ty = ws.expr_ty("_mapped_2");
        let expected = ws.ty("string[]");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_colon_call_infers_generic_self_and_callback_return() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Promise<T>
            local M = {}

            ---@alias Unwrap<T> T extends Promise<infer U> and U or T

            ---@generic T
            ---@return Promise<T>
            function M.new() return M end

            ---@generic U
            ---@param on_resolved fun(value: T): U
            ---@return Promise<Unwrap<U>>
            function M:then1(on_resolved) return self end

            p1 = M.new():then1(function()
                return {} ---@as Promise<integer>
            end)

        "#,
        );

        let expected = ws.ty("Promise<integer>");
        assert_eq!(ws.expr_ty("p1"), expected);
        // assert_eq!(ws.expr_ty("p2"), expected);
    }

    #[test]
    fn test_simple_alias_param_still_infers_function_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Id<T> T

            ---@generic T
            ---@param value Id<T>
            ---@return T
            function id(value)
            end

            result = id("value")
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("string"));
    }

    #[test]
    fn test_function_and_alias_generic_same_name_do_not_collide() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Id<U> U

            ---@generic U
            ---@param value Id<U>
            ---@return U
            function id(value)
            end

            result = id("value")
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("string"));
    }

    #[test]
    fn test_nested_callback_return_alias_waits_for_function_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Promise<T>
            local M = {}

            ---@alias Unwrap<T> T extends Promise<infer U> and U or T
            ---@alias Awaited<T> Unwrap<T>

            ---@generic T
            ---@return Promise<T>
            function M.new() return M end

            ---@generic U
            ---@param on_resolved fun(value: T): U
            ---@return Promise<Awaited<U>>
            function M:then_nested(on_resolved) return self end

            result = M.new():then_nested(function()
                return {} ---@as Promise<integer>
            end)
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("Promise<integer>"));
    }

    #[test]
    fn test_nested_function_generic_shadows_outer_function_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T
            ---@param value T
            ---@return fun<T>(value: T): T
            function make(value)
            end

            local fn = make("outer")
            result = fn(1)
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("integer"));
    }
}
