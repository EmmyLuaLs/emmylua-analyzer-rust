#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, EmmyrcLuaVersion, LuaType, VirtualWorkspace};

    #[test]
    fn test_unpack() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        a, b = table.unpack({ 1, 2, 3 })

        ---@type string[]
        local ddd

        e = table.unpack(ddd)
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let a_expected = ws.expr_ty("1");
        assert_eq!(a_ty, a_expected);

        let b_ty = ws.expr_ty("b");
        let b_expected = ws.expr_ty("2");
        assert_eq!(b_ty, b_expected);

        let e_ty = ws.expr_ty("e");
        let e_expected = ws.ty("string?");
        assert_eq!(e_ty, e_expected);
    }

    #[test]
    fn test_unpack_array_element_is_nullable() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@type integer[]
            local values = { 1, 2 }

            first = table.unpack(values)
            "#,
        );

        assert_eq!(ws.expr_ty("first"), ws.ty("integer?"));
    }

    #[test]
    fn test_unpack_integer_index_signature_is_not_nullable() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@type {[integer]: integer}
            local values = { 1, 2 }

            first = table.unpack(values)
            "#,
        );

        assert_eq!(ws.expr_ty("first"), ws.ty("integer"));
    }

    #[test]
    fn test_unpack_merges_applicable_integer_index_signatures() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@type {[integer]: integer, [number]: string}
            local values = {}

            first = table.unpack(values)
            "#,
        );

        // Both signatures accept integer keys, so unpack merges their values just
        // like `values[1]` does.
        assert_eq!(ws.expr_ty("first"), ws.ty("integer|string"));
    }

    #[test]
    fn test_unpack_integer_index_signature_alias() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@alias IntMap {[integer]: integer}

            ---@type IntMap
            local values = { 1, 2 }

            first = table.unpack(values)
            "#,
        );

        assert_eq!(ws.expr_ty("first"), ws.ty("integer"));
    }

    #[test]
    fn test_unpack_integer_index_signature_table_generic() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@type table<integer, integer>
            local values = {}

            first = table.unpack(values)
            "#,
        );

        assert_eq!(ws.expr_ty("first"), ws.ty("integer"));
    }

    #[test]
    fn test_unpack_object_integer_fields_are_expanded() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@type {[1]: integer, [2]: string}
            local values = { 1, "a" }

            a, b = table.unpack(values)
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("b"), ws.ty("string"));
    }

    #[test]
    fn test_unpack_alias_call_integer_index_signature() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@overload fun<T>(t: T): std.Unpack<T>
            local function f(t) end

            ---@type {[integer]: integer}
            local values = { 1, 2 }

            a, b = f(values)
            "#,
        );

        // `expand_unpack_return` falls back to the integer index signature when an
        // object has no concrete integer fields.
        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("b"), ws.ty("integer"));
    }

    #[test]
    fn test_unpack_alias_call_integer_index_signature_alias() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@alias IntMap {[integer]: integer}

            ---@overload fun<T>(t: T): std.Unpack<T>
            local function f(t) end

            ---@type IntMap
            local values = { 1, 2 }

            a, b = f(values)
            "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("b"), ws.ty("integer"));
    }

    #[test]
    fn test_unpack_union_array_element_keeps_nullable_union() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@type (string|integer)[]
            local values = { 1, "a" }

            element = table.unpack(values)
            "#,
        );

        assert_eq!(ws.expr_ty("element"), ws.ty("string|integer?"));
    }

    #[test]
    fn test_global_unpack_dense_without_range_and_nullable_with_range() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.runtime.version = EmmyrcLuaVersion::Lua51;
        ws.update_emmyrc(emmyrc);

        ws.def(
            r#"
            ---@type integer[]
            local values = { 1, 2 }

            dense = unpack(values)
            ranged = unpack(values, 1, 10)
            "#,
        );

        // The global `unpack` keeps its historical dense element type without an
        // explicit range, but an explicit range can address out-of-bounds indices.
        assert_eq!(ws.expr_ty("dense"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ranged"), ws.ty("integer?"));
    }

    #[test]
    fn test_table_unpack_with_explicit_range_is_nullable() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@type integer[]
            local values = { 1, 2 }

            first = table.unpack(values, 1, 10)
            "#,
        );

        assert_eq!(ws.expr_ty("first"), ws.ty("integer?"));
    }

    #[test]
    fn test_unpack_alias_call_union() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@overload fun<T>(t: T): std.Unpack<T>
            ---@overload fun(t: number): number
            local function f(t)
            end

            a, b, c = f({ 1, 2, 3 })
        "#,
        );

        assert_eq!(ws.expr_ty("a"), LuaType::IntegerConst(1));
        assert_eq!(ws.expr_ty("b"), LuaType::IntegerConst(2));
        assert_eq!(ws.expr_ty("c"), LuaType::IntegerConst(3));
    }

    #[test]
    fn test_unpack_alias_call_colon_mismatch() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class Obj
            ---@field unpack (fun<T>(self: Obj, t: T): std.Unpack<T>) | (fun(self: Obj, t: number): number)
            local Obj = {}

            a, b = Obj:unpack({ 1, 2 })
        "#,
        );

        assert_eq!(ws.expr_ty("a"), LuaType::IntegerConst(1));
        assert_eq!(ws.expr_ty("b"), LuaType::IntegerConst(2));
    }

    #[test]
    fn test_unpack_method_dot_call_with_instance() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class Obj
            local Obj = {}

            ---@generic T
            ---@param t T
            ---@return std.Unpack<T>
            function Obj:unpack(t)
            end

            ---@type Obj
            obj = {}

            a, b = Obj.unpack(obj, { 1, 2 })
        "#,
        );

        assert_eq!(ws.expr_ty("a"), LuaType::IntegerConst(1));
        assert_eq!(ws.expr_ty("b"), LuaType::IntegerConst(2));
    }

    #[test]
    fn test_unpack_alias_call_self_type() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@return std.Unpack<[self, self]>
            function table:dup()
            end

            a, b = table:dup()
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let b_ty = ws.expr_ty("b");
        assert_eq!(ws.humanize_type(a_ty), "tablelib");
        assert_eq!(ws.humanize_type(b_ty), "tablelib");
    }

    #[test]
    fn test_unpack_alias_call_explicit_generic_list() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@generic T
            ---@return std.Unpack<T>
            function f()
            end

            a, b = f--[[@<[string, number]>]]()
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let b_ty = ws.expr_ty("b");
        assert_eq!(ws.humanize_type(a_ty), "string");
        assert_eq!(ws.humanize_type(b_ty), "number");
    }

    #[test]
    fn test_issue_484() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        --- @type integer,integer,integer
        local _a, _b, _c = unpack({ 1, 2, 3 })
        "#,
        ));
    }

    #[test]
    fn test_issue_594() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.runtime.version = EmmyrcLuaVersion::Lua51;
        ws.update_emmyrc(emmyrc);
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        --- @type string[]
        local s = {}

        --- @type string[]
        local s2 = { 'a', unpack(s) }
        "#,
        ));
    }
}
