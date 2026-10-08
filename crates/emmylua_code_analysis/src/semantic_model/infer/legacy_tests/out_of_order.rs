#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_unorder_analysis() {
        let mut ws = VirtualWorkspace::new();

        let files = vec![
            (
                "rx.lua",
                r#"
            local subject = require("subject")

            local rx = {
                subject = subject,
            }

            return rx
            "#,
            ),
            (
                "subject.lua",
                r#"
            ---@class Subject
            local subject = {}

            ---@return Subject
            function subject.new()

            end

            return subject
            "#,
            ),
        ];

        ws.def_files(files);

        let ty = ws.expr_ty("require('rx').subject.new()");
        let expected = ws.ty("Subject");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_unorder_analysis_2() {
        let mut ws = VirtualWorkspace::new();

        let files = vec![
            (
                "meta.lua",
                r#"
                vim = {}
                vim.o.a = 1
                "#,
            ),
            (
                "options.lua",
                r#"
                require "meta"
            vim.o = {}
            "#,
            ),
        ];

        ws.def_files(files);

        let o_ty = ws.expr_ty("vim.o");
        println!("{:?}", o_ty);
        let a_ty = ws.expr_ty("vim.o.a");
        println!("{:?}", a_ty);
        // let expected = ws.ty("Subject");
        // assert_eq!(ty, expected);
    }
}
