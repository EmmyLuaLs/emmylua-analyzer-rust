#[cfg(test)]
mod tests {
    use lsp_types::WorkspaceEdit;

    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    fn check_len(result: &Option<WorkspaceEdit>, len: usize) -> bool {
        let Some(result) = result else {
            return false;
        };
        if let Some(changes) = &result.changes {
            let mut count = 0;
            for (_, edits) in changes {
                count += edits.len();
            }
            if count != len {
                return false;
            }
        }
        true
    }

    /// 检查指定名称的文件的`new_text`是否为预期值
    fn check_new_text(result: &Option<WorkspaceEdit>, file_name: &str, new_text: &str) -> bool {
        let Some(result) = result else {
            return false;
        };
        if let Some(changes) = &result.changes {
            for (uri, edits) in changes {
                let path = uri.path().as_str();
                if path.ends_with(format!("/{}", file_name).as_str()) {
                    if let Some(edit) = edits.first() {
                        if edit.new_text == new_text {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    #[test]
    fn test_int_key() {
        let mut ws = ProviderVirtualWorkspace::new();
        assert!(check_len(
            &ws.check_rename(
                r#"
                local export = {
                    [<??>1] = 1,
                }

                export[1] = 2
            "#,
                "2".to_string()
            ),
            2
        ));

        assert!(check_len(
            &ws.check_rename(
                r#"
                local export = {
                    [1] = 1,
                }

                export[<??>1] = 2
            "#,
                "2".to_string()
            ),
            2
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
        );
        assert!(check_len(&result, 2));
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
        );
        assert!(check_len(&result, 2));
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
        );
        assert!(check_len(&result, 2));
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
        );
        assert!(check_len(&result, 2));
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
            );
            assert!(check_len(&result, 3));
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
            );
            assert!(check_len(&result, 3));
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
        let result = ws.check_rename(
            r#"
                ---@namespace Luakit
                ---@class Test.Abc<??>
                local Test = {}
            "#,
            "Abc".to_string(),
        );
        assert!(check_len(&result, 2));
        assert!(check_new_text(&result, "a.lua", "Luakit.Abc"));
    }

    #[test]
    fn test_namespace_class_1() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_rename(
            r#"
                ---@namespace Luakit
                ---@class Abc
                local Test = {}

                ---@type Abc<??>
                local a = Test
            "#,
            "AAA".to_string(),
        );
        assert!(check_len(&result, 2));
        assert!(check_new_text(&result, "virtual_0.lua", "AAA"));
    }
}
