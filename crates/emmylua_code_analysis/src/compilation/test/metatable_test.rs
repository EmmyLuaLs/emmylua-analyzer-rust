#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_metatable() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            cmd = setmetatable({}, {
                --- @param command string|string[]
                __call = function (_, command)
                end,
                
                --- @param command string
                --- @return fun(...:string)
                __index = function(_, command)
                end,
            })
            "#,
        );

        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            cmd(1)
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            cmd("hello)
        "#
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            cmd({ "hello", "world" })
        "#
        ));

        let ty = ws.expr_ty("cmd.hihihi");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "fun(...: string)");
    }

    #[test]
    fn test_metatable_2() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class switch
            ---@field map table
            ---@field cachedCases table
            local switchMT = {}
            switchMT.__index = switchMT

            ---@return switch
            local function switch()
                local obj = setmetatable({
                    map = {},
                    cachedCases = {},
                }, switchMT)
                a =  obj
            end
            "#,
        );

        let ty = ws.expr_ty("a");
        assert_eq!(ws.humanize_type(ty), "switch");
    }

    #[test]
    fn test_issue_599() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Class.Config
            ---@field abc string
            local ClassConfigMeta = {}

            ---@type table<string, Class.Config>
            local _classConfigMap = {}


            ---@param name string
            ---@return Class.Config
            local function getConfig(name)
                local config = _classConfigMap[name]
                if not config then
                    A = setmetatable({ name = name }, { __index = ClassConfigMeta })
                end
            end
            "#,
        );

        let ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(ty), "Class.Config");
    }
}
