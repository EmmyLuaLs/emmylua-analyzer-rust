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
}
