#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_str_tpl_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class aaa.xxx.bbb

            ---@generic T
            ---@param a aaa.`T`.bbb
            ---@return T
            function get_type(a)
            end
            "#,
        );

        let string_ty = ws.expr_ty("get_type('xxx')");
        let expected = ws.ty("aaa.xxx.bbb");
        assert_eq!(string_ty, expected);
    }
}
