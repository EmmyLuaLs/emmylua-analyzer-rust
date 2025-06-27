#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "A.lua",
            r#"
            ---@export
            local A = {}

            return A
        "#,
        );

        ws.def(
            r#"
            local A = require("A")
            A.newField = 1
        "#,
        );

        ws.def(
            r#"
            E = require("A").newField
            
        "#,
        );
        let res = ws.expr_ty("E");
        dbg!(&res);
    }
}
