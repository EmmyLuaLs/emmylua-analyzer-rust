#[cfg(test)]
mod test {
    use crate::{
        DiagnosticCode, ModuleVisibility, SalsaModuleExportSummary, SalsaSemanticTargetSummary,
        VirtualWorkspace,
    };

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

        // ---@meta no-require 的优先级最高
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
        let module_index = ws.analysis.compilation.get_db().get_module_index();
        let module = module_index.get_module(file_id);
        assert!(module.is_some());
        assert!(module.unwrap().visible.is_hidden());
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
        let module_index = ws.analysis.compilation.get_db().get_module_index();
        let module = module_index.get_module(file_id);
        assert!(module.is_some());
        assert!(module.unwrap().visible == ModuleVisibility::Default);
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
            let module_index = ws.analysis.compilation.get_db().get_module_index();
            let module = module_index.get_module(file_id);
            assert!(module.is_some());
            assert!(module.unwrap().visible == ModuleVisibility::Internal);
        }
        {
            // 可见性必须附加在定义语句上
            let file_id = ws.def_file(
                "b.lua",
                r#"
                B = {
                }

                ---@internal
                return B
                "#,
            );
            let module_index = ws.analysis.compilation.get_db().get_module_index();
            let module = module_index.get_module(file_id);
            assert!(module.is_some());
            assert!(module.unwrap().visible == ModuleVisibility::Default);
        }

        {
            // 当 return 返回匿名结构时, 允许为其附加可见性
            let file_id = ws.def_file(
                "c.lua",
                r#"

                ---@internal
                return {
                }
                "#,
            );
            let module_index = ws.analysis.compilation.get_db().get_module_index();
            let module = module_index.get_module(file_id);
            assert!(module.is_some());
            assert!(module.unwrap().visible == ModuleVisibility::Internal);
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
    fn test_compilation_module_projection_resolves_export_query() {
        let mut ws = VirtualWorkspace::new();

        let file_id = ws.def_file(
            "a.lua",
            r#"
                local export = {}
                return export
                "#,
        );

        let module = ws
            .analysis
            .compilation
            .find_module_by_file_id(file_id)
            .unwrap();

        assert_eq!(module.full_module_name, "a");
        assert!(matches!(
            module.export,
            Some(SalsaModuleExportSummary::LocalDecl { .. })
                | Some(SalsaModuleExportSummary::Table { .. })
        ));
        assert!(matches!(
            module.semantic_target,
            Some(SalsaSemanticTargetSummary::Decl(_))
                | Some(SalsaSemanticTargetSummary::Member(_))
                | Some(SalsaSemanticTargetSummary::Signature(_))
        ));
        assert!(module.has_export_type());
    }

    #[test]
    fn test_compilation_module_projection_finds_require_path() {
        let mut ws = VirtualWorkspace::new();

        let file_id = ws.def_file(
            "nested/mod.lua",
            r#"
                local export = {}
                return export
                "#,
        );

        let module = ws
            .analysis
            .compilation
            .find_module_by_require_path("nested.mod")
            .unwrap();

        assert_eq!(module.file_id, file_id);
        assert_eq!(module.name, "mod");
        assert!(module.has_export_type());
    }
}
