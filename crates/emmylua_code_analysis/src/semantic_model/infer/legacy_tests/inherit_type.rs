#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_inherit_type() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@class A
            ---@field aaa? fun(a: string)

            local a ---@type A

            function a.aaa(a)
                d = a
            end
            "#,
        );

        let string_ty = ws.expr_ty("d");
        let expected = ws.ty("string");
        assert_eq!(string_ty, expected);
    }
}
