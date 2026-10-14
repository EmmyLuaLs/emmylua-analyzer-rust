#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, ModuleVisibility, VirtualWorkspace};

    #[test]
    fn test_module_annotation() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def_files(vec![(
            "a.lua",
            r#"
                local a = {
                }
                return a
                "#,
        )]);

        ws.def(
            r#"
            ---@module "a"
            aaa = {}
            "#,
        );

        let aaa_ty = ws.expr_ty("aaa");
        assert!(aaa_ty.is_module_ref());
    }

    #[test]
    fn test_module_no_require() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        // ---@meta no-require has the highest priority
        let file_id = ws.def_file(
            "a.lua",
            r#"
                ---@meta no-require

                ---@public
                A = {
                }

                return A
                "#,
        );
        let module = ws.analysis.db.module_info_of(file_id);
        assert!(module.is_some());
        assert!(module.as_ref().unwrap().visible.is_hidden());
    }

    #[test]
    fn test_module_default_visibility() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let file_id = ws.def_file(
            "a.lua",
            r#"
                A = {
                }

                return A
                "#,
        );
        let module = ws.analysis.db.module_info_of(file_id);
        assert!(module.is_some());
        assert!(module.as_ref().unwrap().visible == ModuleVisibility::Public);
    }

    #[test]
    fn test_module_internal() {
        let mut ws = VirtualWorkspace::new();
        {
            let file_id = ws.def_file(
                "a.lua",
                r#"
                ---@internal
                A = {
                }

                return A
                "#,
            );
            let module = ws.analysis.db.module_info_of(file_id);
            assert!(module.is_some());
            assert!(module.as_ref().unwrap().visible == ModuleVisibility::Internal);
        }
        {
            // Visibility must be attached to the definition statement
            let file_id = ws.def_file(
                "b.lua",
                r#"
                B = {
                }

                ---@internal
                return B
                "#,
            );
            let module = ws.analysis.db.module_info_of(file_id);
            assert!(module.is_some());
            assert!(module.as_ref().unwrap().visible == ModuleVisibility::Public);
        }

        {
            // When return returns an anonymous structure, visibility may be attached to it
            let file_id = ws.def_file(
                "c.lua",
                r#"

                ---@internal
                return {
                }
                "#,
            );
            let module = ws.analysis.db.module_info_of(file_id);
            assert!(module.is_some());
            assert!(module.as_ref().unwrap().visible == ModuleVisibility::Internal);
        }
    }

    #[test]
    fn test_module_return_from_truthy_while_block() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
                while {} do
                    return 1
                end
                "#,
        );

        // `def()` creates `virtual_0.lua`, so the block is requireable as `virtual_0`.
        let ty = ws.expr_ty(r#"require("virtual_0")"#);
        let integer = ws.ty("integer");
        assert_eq!(ty, integer);
    }

    // Migrated to semantic check/ (new pipeline); module export metadata block semantics pending semantic support.
    #[test]
    fn test_module_multiple_return_paths_preserve_export_metadata_block() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
                ---@class (partial) ModuleExport
                ---@field private hidden integer
                local export = {}

                if flag then
                    return export
                end

                return export
                "#,
        );

        // `AccessInvisible` only fires if the export still points at `export`.
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AccessInvisible,
            r#"
                local export = require("virtual_0")
                export.hidden = 1
                "#,
        ));
    }
    #[test]
    fn test_require_inference_uses_source_root() {
        use emmylua_parser::{LuaAstToken, LuaLocalName};
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let base = ws.virtual_url_generator.base.clone();
        ws.analysis.add_main_workspace(base.join("a"));
        ws.analysis.add_main_workspace(base.join("b"));
        ws.def_files(vec![
            ("a/helper.lua", "return 11"),
            ("b/helper.lua", "return 'from-b'"),
        ]);
        for (path, expected) in [("a/control.lua", "integer"), ("b/control.lua", "string")] {
            let file = ws.def_file(path, "local value = require('helper')");
            let local = ws.get_node::<LuaLocalName>(file);
            let model = ws.analysis.semantic_model(file);
            let decl = model
                .decl_by_offset(local.get_name_token().unwrap().get_position())
                .unwrap();
            let typ = model.type_of_decl(&decl).unwrap();
            assert_eq!(ws.humanize_type(typ), expected);
        }
    }
}
