#[allow(clippy::module_inception)]
#[cfg(test)]
mod test {
    use crate::{reformat_lua_code, styles::LuaCodeStyle};

    #[test]
    fn test_reformat_lua_code() {
        let code = r#"
            local a = 1
            local b =  2
            local c =   a+b
            print  (c     )
        "#;

        let styles = LuaCodeStyle::default();
        let formatted_code = reformat_lua_code(code, &styles);
        println!("Formatted code:\n{}", formatted_code);
    }
}
