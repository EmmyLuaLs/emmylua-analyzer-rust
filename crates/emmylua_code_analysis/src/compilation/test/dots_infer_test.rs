#[cfg(test)]
mod test {
    use crate::{FileId, LuaType, VirtualWorkspace};
    use emmylua_parser::LuaLocalStat;

    fn get_type_of_first_local_assign(ws: &mut VirtualWorkspace, file_id: FileId) -> LuaType {
        let node: LuaLocalStat = ws.get_node(file_id);
        let expr = &node.get_value_exprs().next().unwrap();
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        semantic_model.infer_expr(expr.clone()).unwrap()
    }

    #[test]
    fn test_dots_normal() {
        let mut ws = VirtualWorkspace::new();

        let file_id = ws.def(
            r#"
                --- @param ... integer
                function foo(...)
                    local a = { ... }
                end
            "#,
        );
        let a_ty = get_type_of_first_local_assign(&mut ws, file_id);
        let a_expected = ws.ty("integer[]");
        assert_eq!(a_ty, a_expected);
    }

    #[test]
    fn test_dots_normal_variadic() {
        let mut ws = VirtualWorkspace::new();

        let file_id = ws.def(
            r#"
                --- @param ... integer...
                function foo(...)
                    local a = { ... }
                end
            "#,
        );
        let a_ty = get_type_of_first_local_assign(&mut ws, file_id);
        let a_expected = ws.ty("integer[]");
        assert_eq!(a_ty, a_expected);
    }

    #[test]
    fn test_dots_generic() {
        let mut ws = VirtualWorkspace::new();

        let file_id = ws.def(
            r#"
                --- @generic T
                --- @param ... T
                function foo(...)
                    local a = { ... }
                end
            "#,
        );
        let a_ty = get_type_of_first_local_assign(&mut ws, file_id);
        assert_eq!(&ws.humanize_type(a_ty), "T[]");
    }

    #[test]
    fn test_dots_variadic() {
        let mut ws = VirtualWorkspace::new();

        let file_id = ws.def(
            r#"
                --- @generic T
                --- @param ... T...
                function foo(...)
                    local a = { ... }
                end
            "#,
        );
        let a_ty = get_type_of_first_local_assign(&mut ws, file_id);
        assert_eq!(&ws.humanize_type(a_ty), "T[]");
    }
}
