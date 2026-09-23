#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    fn workspace_with_require_like(names: &[&str]) -> VirtualWorkspace {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.runtime.require_like_function = names.iter().map(|s| s.to_string()).collect();
        ws.update_emmyrc(emmyrc);
        ws
    }

    #[test]
    fn test_require_like_plain_name_infers_module_type() {
        let mut ws = workspace_with_require_like(&["include"]);
        ws.def_file(
            "mod.lua",
            r#"
            ---@class Mod
            ---@field value integer
            local M = {}
            return M
            "#,
        );
        ws.def(
            r#"
            A = include("mod")
            "#,
        );
        let ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(ty), "Mod");
    }

    #[test]
    fn test_require_like_dotted_name_infers_module_type() {
        let mut ws = workspace_with_require_like(&["VFS.Include"]);
        ws.def_file(
            "mod.lua",
            r#"
            ---@class Mod
            ---@field value integer
            local M = {}
            return M
            "#,
        );
        ws.def(
            r#"
            B = VFS.Include("mod")
            "#,
        );
        let ty = ws.expr_ty("B");
        assert_eq!(ws.humanize_type(ty), "Mod");
    }
}
