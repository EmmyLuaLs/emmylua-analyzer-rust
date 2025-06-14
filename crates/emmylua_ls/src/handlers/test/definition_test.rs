#[cfg(test)]
mod tests {
    use lsp_types::GotoDefinitionResponse;

    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_basic_definition() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                ---@generic T
                ---@param name `T`
                ---@return T
                local function new(name)
                    return name
                end

                ---@class Ability

                local a = new("<??>Ability")
            "#,
        );
    }

    #[test]
    fn test_table_field_definition_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                ---@class T
                ---@field func fun(self:string)

                ---@type T
                local t = {
                    f<??>unc = function(self)
                    end
                }
            "#,
        );
    }

    #[test]
    fn test_table_field_definition_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                ---@class T
                ---@field func fun(self: T) 注释注释

                ---@type T
                local t = {
                    func = function(self)
                    end,
                    a = 1,
                }

                t:func<??>()
            "#,
        );
    }

    #[test]
    fn test_goto_field() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                local t = {}
                function t:test(a)
                    self.abc = a
                end

                print(t.abc<??>)
            "#,
        );
    }

    #[test]
    fn test_goto_overload() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws
            .check_definition(
                r#"
                ---@class Goto1
                local Goto1
                ---@class Goto2
                local Goto2
                ---@class Goto3
                local Goto3

                ---@class T
                ---@field func fun(a:Goto1) # 1
                ---@field func fun(a:Goto2) # 2
                ---@field func fun(a:Goto3) # 3
                local T = {}

                function T:func(a)
                end

                ---@type T
                local t

                t.func(Goto1)
                t.fu<??>nc(Goto2)
                t.func(Goto3)
            "#,
            )
            .unwrap();
        match result {
            GotoDefinitionResponse::Scalar(_) => {}
            _ => {
                panic!("expect scalar");
            }
        }
    }
}
