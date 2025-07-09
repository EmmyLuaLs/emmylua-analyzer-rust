#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::{LuaType, LuaUnionType, VirtualWorkspace};

    #[test]
    fn test_closure_param_infer() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@alias foo (fun(tbl: any): (number, string))

        ---@type foo
        local b = {}

        for k3, v3 in b do
            k1 = k3
            v1 = v3
        end


        ---@class bar
        ---@overload fun(tbl: any): (number, string)

        ---@type bar
        local c = {}

        for k4, v4 in c do
            k2 = k4
            v2 = v4
        end
        "#,
        );

        assert_eq!(ws.expr_ty("k1"), LuaType::Number);
        assert_eq!(ws.expr_ty("v1"), LuaType::String);
        assert_eq!(ws.expr_ty("k2"), LuaType::Number);
        assert_eq!(ws.expr_ty("v2"), LuaType::String);
    }

    #[test]
    fn test_issue_227() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local a --- @type any

        for k in pairs(a) do
            -- k should be any not integer
            d = k
        end
        "#,
        );

        assert_eq!(ws.expr_ty("d"), LuaType::Any);
    }

    #[test]
    fn test_issue_321() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@return fun():string?
        local function test(...) end

        for k in test() do
            -- k can't be nil
            d = k
        end
        "#,
        );

        assert_eq!(ws.expr_ty("d"), LuaType::String);
    }

    #[test]
    fn test_issue_490() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@generic T: table, K, V
        ---@param t T
        ---@return fun(table: table<K, V>, index?: K):K, V
        ---@return T
        local function spairs(t) end

        --- @type table<string, integer>
        local t = { a = 1, b = 2, c = 3 }
        for name, value in spairs(t) do
            a = name
            b = value
        end
        "#,
        );

        let a = ws.expr_ty("a");
        let b = ws.expr_ty("b");
        assert_eq!(a, LuaType::String);
        assert_eq!(b, LuaType::Integer);
    }

    #[test]
    fn test_issue_291() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            --- @class A
            --- @field [integer] string
            --- @field a boolean
            --- @field b number
            local a

            for _, v in ipairs(a) do
                d = v
            end
        "#,
        );

        assert_eq!(ws.expr_ty("d"), LuaType::String);
    }

    #[test]
    fn test_issue_291_2() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            --- @class A
            --- @field [1] string
            --- @field [2] number
            local a

            for _, v in ipairs(a) do
                d = v
            end
        "#,
        );

        assert_eq!(
            ws.expr_ty("d"),
            LuaType::Union(Arc::new(LuaUnionType::from_vec(vec![
                LuaType::String,
                LuaType::Number
            ]))),
        );
    }
}
