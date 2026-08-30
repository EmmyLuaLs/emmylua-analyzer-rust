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
        let module = ws.analysis.salsa.module_info_of(file_id);
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
        let module = ws.analysis.salsa.module_info_of(file_id);
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
            let module = ws.analysis.salsa.module_info_of(file_id);
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
            let module = ws.analysis.salsa.module_info_of(file_id);
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
            let module = ws.analysis.salsa.module_info_of(file_id);
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
        let nil = ws.ty("nil");
        assert!(ws.check_type(&ty, &integer));
        assert!(!ws.check_type(&ty, &nil));
    }

    // Migrated to salsa check/ (new pipeline); module export metadata block semantics pending salsa support.
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
}
