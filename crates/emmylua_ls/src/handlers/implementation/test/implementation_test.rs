#[cfg(test)]
mod tests {

    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "1.lua",
            r#"
                local M = {}
                function M.delete(a)
                end
                return M
            "#,
        );
        ws.def_file(
            "2.lua",
            r#"
               delete = require("1").delete
               delete()
            "#,
        );
        ws.def_file(
            "3.lua",
            r#"
               delete = require("1").delete
               delete()
            "#,
        );
        // ws.check_implementation(
        //     r#"
        //         de<??>lete()
        //     "#,
        // );

        ws.check_implementation(
            r#"
                local a = require("1").del<??>ete
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
                        yyy.<??>a = 2
                    end
                end

            "#,
        );
    }
}
