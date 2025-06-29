#[cfg(test)]
mod tests {
    use crate::handlers::test_lib::ProviderVirtualWorkspace;

    #[test]
    fn test_int_key() {
        let mut ws = ProviderVirtualWorkspace::new();
        let result = ws.check_rename(
            r#"
                local export = {
                    [<??>1] = 1,
                }

                export[1] = 2
            "#,
            "2".to_string(),
            2,
        );
        assert!(result);
    }
}
