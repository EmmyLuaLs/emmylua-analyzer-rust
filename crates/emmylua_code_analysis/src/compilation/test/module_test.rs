#[cfg(test)]
mod test {
    use crate::{ModuleVisibility, VirtualWorkspace};

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
}
