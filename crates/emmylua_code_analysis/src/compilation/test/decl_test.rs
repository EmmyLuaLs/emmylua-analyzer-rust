#[cfg(test)]
mod test {
    use crate::{
        DiagnosticCode, DocTypeInferContext, GenericTplId, LuaType, VirtualWorkspace,
        build_compilation_signature_doc_function,
        compilation::{global_type, infer_compilation_decl_type},
        complete_type_generic_args, find_compilation_decl_by_position,
        find_compilation_param_generic_params, infer_doc_type,
    };
    use emmylua_parser::{LuaAstNode, LuaDocType, LuaNameExpr, LuaParamName};

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

    #[test]
    fn test_signature_projection_preserves_function_tpls() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
                ---@alias IsString<T> T extends string and number or boolean

                ---@generic T
                ---@param box Box<T>
                ---@return IsString<T>
                function get(box) end
            "#,
        );

        let globals = ws
            .analysis
            .compilation
            .get_db()
            .get_summary_db()
            .file()
            .globals(file_id)
            .expect("global summary");
        let get_function = globals
            .functions
            .iter()
            .find(|function| function.name.as_str() == "get")
            .expect("get summary function");
        let projected = build_compilation_signature_doc_function(
            ws.analysis.compilation.get_db(),
            file_id,
            get_function.signature_offset,
        )
        .expect("projected signature function");

        assert!(projected.contain_tpl(), "projected = {projected:?}");
        assert!(
            projected.get_ret().contain_tpl(),
            "projected return = {:?}",
            projected.get_ret()
        );
    }

    #[test]
    fn test_signature_projection_preserves_function_tpl_defaults() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
                ---@generic T = string
                ---@return T
                function use_default() end
            "#,
        );

        let globals = ws
            .analysis
            .compilation
            .get_db()
            .get_summary_db()
            .file()
            .globals(file_id)
            .expect("global summary");
        let function = globals
            .functions
            .iter()
            .find(|function| function.name.as_str() == "use_default")
            .expect("use_default summary function");
        let projected = build_compilation_signature_doc_function(
            ws.analysis.compilation.get_db(),
            file_id,
            function.signature_offset,
        )
        .expect("projected signature function");

        let LuaType::TplRef(tpl) = projected.get_ret() else {
            panic!("expected tpl return, got {:?}", projected.get_ret());
        };

        assert_eq!(
            ws.humanize_type(tpl.get_default_type().cloned().expect("tpl default type")),
            "string"
        );
    }

    #[test]
    fn test_compilation_decl_preserves_bare_generic_defaults() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
                ---@class Box<T = string>

                ---@type Box
                BoxDefault = {}
            "#,
        );

        let tree = ws
            .analysis
            .compilation
            .get_db()
            .get_vfs()
            .get_syntax_tree(&file_id)
            .expect("Tree must exist");
        let name_expr = tree
            .get_chunk_node()
            .descendants::<LuaNameExpr>()
            .find(|name_expr| {
                name_expr
                    .get_name_text()
                    .is_some_and(|name| name == "BoxDefault")
            })
            .expect("BoxDefault name expr");
        let doc_type = tree
            .get_chunk_node()
            .descendants::<LuaDocType>()
            .find(|doc_type| {
                doc_type.syntax().text() == "Box"
                    && doc_type.syntax().ancestors().any(|ancestor| {
                        ancestor.kind() == emmylua_parser::LuaSyntaxKind::DocTagType.into()
                    })
            })
            .expect("Box doc type");
        let decl = find_compilation_decl_by_position(
            ws.analysis.compilation.get_db(),
            file_id,
            name_expr.get_position(),
        )
        .expect("compilation decl");
        let type_id = crate::LuaTypeDeclId::global("Box");
        let completed =
            complete_type_generic_args(ws.analysis.compilation.get_db(), &type_id, Vec::new())
                .completed_args
                .expect("completed args");
        let summary_type_def = ws
            .analysis
            .compilation
            .get_db()
            .get_summary_db()
            .doc()
            .type_def_by_name(file_id, smol_str::SmolStr::new("Box"))
            .expect("summary type def");
        let default_type_key = summary_type_def.generic_params[0]
            .default_type_offset
            .expect("summary default type key");
        let resolved_default_type = ws
            .analysis
            .compilation
            .get_db()
            .get_summary_db()
            .doc()
            .resolved_type_by_key(file_id, default_type_key)
            .expect("resolved summary default type");
        let reconstructed_default_doc_type = LuaDocType::cast(
            resolved_default_type
                .doc_type
                .syntax_id
                .to_lua_syntax_id()
                .to_node_from_root(&tree.get_red_root())
                .expect("default type syntax node"),
        )
        .expect("default type doc node");
        let direct_doc_type = infer_doc_type(
            DocTypeInferContext::new(ws.analysis.compilation.get_db(), file_id),
            &doc_type,
        );

        let decl_type = infer_compilation_decl_type(ws.analysis.compilation.get_db(), &decl)
            .expect("compilation decl type");
        let global =
            global_type(ws.analysis.compilation.get_db(), "BoxDefault").expect("global type");

        assert_eq!(completed.len(), 0);
        assert_eq!(summary_type_def.generic_params.len(), 1);
        assert_eq!(
            format!("{:?}", resolved_default_type.lowered.kind),
            "Name { name: \"string\" }"
        );
        assert_eq!(reconstructed_default_doc_type.syntax().text(), "string");
        let LuaType::Generic(direct_generic) = &direct_doc_type else {
            panic!(
                "expected direct doc type generic, got {:?}",
                direct_doc_type
            );
        };
        assert_eq!(direct_generic.get_params().len(), 1);
        assert_eq!(
            ws.humanize_type(direct_generic.get_params()[0].clone()),
            "string"
        );
        assert_eq!(ws.humanize_type(direct_doc_type), "Box<string>");
        assert_eq!(ws.humanize_type(decl_type), "Box<string>");
        assert_eq!(ws.humanize_type(global), "Box<string>");
    }

    #[test]
    fn test_type_generic_scope_stays_with_owning_type() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
                ---@class Box<T: string>
                ---@field value T

                ---@class Other<T: integer>
                ---@field value T
            "#,
        );

        let tree = ws
            .analysis
            .compilation
            .get_db()
            .get_vfs()
            .get_syntax_tree(&file_id)
            .expect("Tree must exist");
        let generic_refs = tree
            .get_chunk_node()
            .descendants::<LuaDocType>()
            .filter(|doc_type| doc_type.syntax().text() == "T")
            .map(|doc_type| {
                infer_doc_type(
                    DocTypeInferContext::new(ws.analysis.compilation.get_db(), file_id),
                    &doc_type,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(generic_refs.len(), 2);

        let LuaType::TplRef(first_tpl) = &generic_refs[0] else {
            panic!(
                "expected first type generic tpl ref, got {:?}",
                generic_refs[0]
            );
        };
        let LuaType::TplRef(second_tpl) = &generic_refs[1] else {
            panic!(
                "expected second type generic tpl ref, got {:?}",
                generic_refs[1]
            );
        };

        assert_eq!(first_tpl.get_tpl_id(), GenericTplId::Type(0));
        assert_eq!(second_tpl.get_tpl_id(), GenericTplId::Type(0));
        assert_eq!(
            ws.humanize_type(
                first_tpl
                    .get_constraint()
                    .cloned()
                    .expect("first constraint")
            ),
            "string"
        );
        assert_eq!(
            ws.humanize_type(
                second_tpl
                    .get_constraint()
                    .cloned()
                    .expect("second constraint")
            ),
            "integer"
        );
    }
}
