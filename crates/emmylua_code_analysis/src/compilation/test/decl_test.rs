#[cfg(test)]
mod test {
    use crate::{
        DiagnosticCode, VirtualWorkspace, find_compilation_decl_by_position,
        find_compilation_param_generic_params,
    };
    use emmylua_parser::{LuaAstNode, LuaParamName};

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@return any ...
        ---@return integer offset
        local function unpack() end
        a, b, c, d = unpack()
        "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("any"));
        assert_eq!(ws.expr_ty("b"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("c"), ws.ty("nil"));
        assert_eq!(ws.expr_ty("d"), ws.ty("nil"));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@return integer offset
        ---@return any ...
        local function unpack() end
        a, b, c, d = unpack()
        "#,
        );

        assert_eq!(ws.expr_ty("a"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("b"), ws.ty("any"));
        assert_eq!(ws.expr_ty("c"), ws.ty("any"));
        assert_eq!(ws.expr_ty("d"), ws.ty("any"));
    }

    #[test]
    fn test_3() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@return any ...
                ---@return integer offset
                local function unpack() end

                ---@param a nil|integer|'l'|'L'
                local function test(a) end
                local len = unpack()
                test(len)
        "#,
        ));
    }

    #[test]
    fn test_param_generic_metadata_projection() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
                ---@generic T: number, U = string
                ---@param value T
                ---@return U
                local function pick(value)
                    return value
                end
            "#,
        );

        let param = ws.get_node::<LuaParamName>(file_id);
        let decl = find_compilation_decl_by_position(
            ws.analysis.compilation.get_db(),
            file_id,
            param.get_position(),
        )
        .expect("param decl");
        let generic_params =
            find_compilation_param_generic_params(ws.analysis.compilation.get_db(), &decl)
                .expect("param generic metadata");

        assert_eq!(generic_params.len(), 2);
        assert_eq!(generic_params[0].name, "T");
        assert_eq!(
            ws.humanize_type(generic_params[0].constraint.clone().unwrap()),
            "number"
        );
        assert!(generic_params[0].default_type.is_none());
        assert_eq!(generic_params[1].name, "U");
        assert!(generic_params[1].constraint.is_none());
        assert_eq!(
            ws.humanize_type(generic_params[1].default_type.clone().unwrap()),
            "string"
        );
    }
}
