#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_221() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        --- @param opts table|nil
        --- @return any
        function foo(opts)
        opts = opts or {}
        return opts.field
        end

        --- @param opts table?
        --- @return any
        function bar(opts)
        opts = opts or {}
        return opts.field
        end
            "#,
        ));
    }

    #[test]
    fn test_issue_222() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local a --- @type boolean
            local b --- @type true?
            a = b or false
            "#,
        ));
    }

    #[test]
    fn test_issue_230() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local b = true ---@type boolean
            a = b and 2 or nil
            "#,
        );

        let a_ty = ws.expr_ty("a");
        println!("{:?}", a_ty);
        assert_eq!(
            format!("{:?}", a_ty).to_string(),
            "Union(Multi([IntegerConst(2), Nil]))"
        );
    }

    #[test]
    fn test_issue_258() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local a            --- @type string|nil
            local b            --- @type string|nil
            c = a or b
            "#,
        );

        let c = ws.expr_ty("c");
        let c_desc = ws.humanize_type(c);
        assert_eq!(c_desc, "string?");
    }

    #[test]
    fn test_issue_470() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Player : PlayerBase

            ---@class PlayerBase
            local Player = {}

            ---@return boolean
            ---@return_cast self Player
            function Player:isPlayer()
                return true
            end

            local topCreature ---@type PlayerBase?


            if not topCreature or not topCreature:isPlayer() then
                return
            end

            a = topCreature
            "#,
        );

        let a = ws.expr_ty("a");
        let desc = ws.humanize_type(a);
        assert_eq!(desc, "Player");
    }

    #[test]
    fn test_issue_482() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local command_provider --- @type {commands: string[]}?

            --- @type string[]
            local commands = type(command_provider) == 'table' and command_provider.commands or {}
            "#,
        ));
    }
}
