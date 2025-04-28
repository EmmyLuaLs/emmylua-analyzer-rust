#[cfg(test)]
mod tests {

    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "2.lua",
            r#"
               delete = require("virtual_0").delete
               delete()
            "#,
        );
        ws.def_file(
            "3.lua",
            r#"
               delete = require("virtual_0").delete
               delete()
            "#,
        );

        ws.check_implementation(
            r#"
                local M = {}
                function M.de<??>lete(a)
                end
                return M
            "#,
        );
    }

    #[test]
    fn test_2() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "1.lua",
            r#"
                ---@class (partial) Test
                test = {}

                test.a = 1
            "#,
        );
        ws.def_file(
            "2.lua",
            r#"
                ---@class (partial) Test
                test = {}
                test.a = 1
            "#,
        );
        ws.def_file(
            "3.lua",
            r#"
                local a = test.a
            "#,
        );
        ws.check_implementation(
            r#"
                t<??>est
            "#,
        );
    }

    #[test]
    fn test_3() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "1.lua",
            r#"
                ---@class YYY
                ---@field a number
                yyy = {}

                if false then
                    yyy.a = 1
                    if yyy.a then
                    end
                end

            "#,
        );
        ws.check_implementation(
            r#"
                yyy.<??>a = 2
            "#,
        );
    }
}
