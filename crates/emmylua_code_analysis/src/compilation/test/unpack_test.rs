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

        assert!(ws.check_code_for(
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
        ws.analysis.update_config(emmyrc.into());
        assert!(ws.check_code_for(
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
