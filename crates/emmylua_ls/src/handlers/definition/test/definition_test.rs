#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_basic_definition() {
        let mut ws = ProviderVirtualWorkspace::new();
        ws.check_definition(
            r#"
                ---@generic T: string
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
}
