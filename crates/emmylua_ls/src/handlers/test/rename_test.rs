#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_int_key() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(ws.check_rename(
            r#"
                local export = {
                    [<??>1] = 1,
                }

                export[1] = 2
            "#,
            "2".to_string(),
            2,
        ));

        assert!(ws.check_rename(
            r#"
                local export = {
                    [1] = 1,
                }

                export[<??>1] = 2
            "#,
            "2".to_string(),
            2,
        ));
    }

    #[test]
    fn test_int_key_in_class() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_rename(
            r#"
            ---@class Test
            ---@field [<??>1] number
            local Test = {}

            Test[1] = 2
            "#,
            "2".to_string(),
            2,
        );
        assert!(result);
    }

    #[test]
    fn test_rename_class_field() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_rename(
            r#"
                ---@class AnonymousObserver
                local AnonymousObserver

                function AnonymousObserver:__init(next)
                    self.ne<??>xt = next
                end

                function AnonymousObserver:onNextCore(value)
                    self.next(value)
                end
            "#,
            "_next".to_string(),
            2,
        );
        assert!(result);
    }

    #[test]
    fn test_rename_generic_type() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_rename(
            r#"
            ---@class Params<T>

            ---@type Para<??>ms<number>
            "#,
            "Params1".to_string(),
            2,
        );
        assert!(result);
    }

    #[test]
    fn test_rename_class_field_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_rename(
            r#"
                ---@class ABC
                local ABC = {}

                local function test()
                end
                ABC.te<??>st = test

                ABC.test()
            "#,
            "test1".to_string(),
            2,
        );
        assert!(result);
    }

    #[test]
    fn test_doc_param() {
        let mut ws = ProviderVirtualWorkspace::new();
        {
            let result = ws.check_rename(
                r#"
                ---@param aaa<??> number
                local function test(aaa)
                    local b = aaa
                end
            "#,
                "aaa1".to_string(),
                3,
            );
            assert!(result);
        }
        {
            let result = ws.check_rename(
                r#"
                    ---@param aaa<??> number
                    function testA(aaa)
                        local b = aaa
                    end
                "#,
                "aaa1".to_string(),
                3,
            );
            assert!(result);
        }
    }

    #[test]
    fn test_namespace_class() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            r#"
                ---@param a Luakit.Test.Abc
                local function Of(a)
                end

            "#,
        );
        ws.check_rename(
            r#"
                ---@namespace Luakit
                ---@class Test.Abc<??>
                local Test = {}
            "#,
            "Abc".to_string(),
            2,
        );
    }
}
