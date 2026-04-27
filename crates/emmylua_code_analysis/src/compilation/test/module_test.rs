#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use crate::{
        CompilationModuleVisibility, CompilationTypeDeclId, CompilationTypeDeclScope,
        DiagnosticCode, LuaSemanticDeclId, SemanticDeclLevel, VirtualWorkspace, WorkspaceFolder,
    };
    use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaIndexExpr};

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
        let module_index = ws.analysis.compilation.module_index();
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
        let module_index = ws.analysis.compilation.module_index();
        let module = module_index.get_module(file_id);
        assert!(module.is_some());
        assert!(module.unwrap().visible == CompilationModuleVisibility::Default);
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
            let module_index = ws.analysis.compilation.module_index();
            let module = module_index.get_module(file_id);
            assert!(module.is_some());
            assert!(module.unwrap().visible == CompilationModuleVisibility::Internal);
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
            let module_index = ws.analysis.compilation.module_index();
            let module = module_index.get_module(file_id);
            assert!(module.is_some());
            assert!(module.unwrap().visible == CompilationModuleVisibility::Default);
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
            let module_index = ws.analysis.compilation.module_index();
            let module = module_index.get_module(file_id);
            assert!(module.is_some());
            assert!(module.unwrap().visible == CompilationModuleVisibility::Internal);
        }

        {
            // 导出全局函数时, 语义层需要同时覆盖 decl/signature 目标
            let file_id = ws.def_file(
                "d.lua",
                r#"
                ---@internal
                function build()
                end

                return build
                "#,
            );
            let module_index = ws.analysis.compilation.module_index();
            let module = module_index.get_module(file_id);
            assert!(module.is_some());
            assert!(module.unwrap().visible == CompilationModuleVisibility::Internal);
        }
    }

    #[test]
    fn test_compilation_module_node_tree() {
        let mut ws = VirtualWorkspace::new();

        let file_ids = ws.def_files(vec![
            ("pkg/root.lua", "return true"),
            ("pkg/root/aaa.lua", "return true"),
            ("pkg/root/hhhhiii.lua", "return true"),
        ]);

        let module_index = ws.analysis.compilation.module_index();
        let module_node = module_index
            .find_module_node("pkg.root")
            .expect("module node");

        assert_eq!(module_node.children.len(), 2);
        assert!(module_node.children.contains_key("aaa"));
        assert!(module_node.children.contains_key("hhhhiii"));
        assert!(module_node.file_ids.contains(&file_ids[0]));
    }

    #[test]
    fn test_compilation_module_workspace_file_queries() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let library_root = ws.virtual_url_generator.base.join("module");
        ws.analysis
            .add_library_workspace(&WorkspaceFolder::with_package(
                library_root.clone(),
                PathBuf::from("socket"),
            ));
        ws.analysis
            .add_library_workspace(&WorkspaceFolder::with_package(
                library_root,
                PathBuf::from("net"),
            ));

        let file_ids = ws.def_files(vec![
            (
                "module/socket/init.lua",
                r#"
                ---@internal
                return true
                "#,
            ),
            (
                "module/net/init.lua",
                r#"
                ---@internal
                return true
                "#,
            ),
            ("main.lua", "return true"),
        ]);

        let socket_file_id = file_ids[0];
        let net_file_id = file_ids[1];
        let main_file_id = file_ids[2];

        let compilation = &ws.analysis.compilation;
        let socket_info = compilation
            .find_module_by_file_id(socket_file_id)
            .expect("socket module");
        let net_info = compilation
            .find_module_by_file_id(net_file_id)
            .expect("net module");

        assert!(compilation.module_is_library(socket_file_id));
        assert!(compilation.module_is_library(net_file_id));
        assert!(compilation.module_is_main(main_file_id));
        assert!(!compilation.module_is_std(main_file_id));
        assert!(!compilation.std_file_ids().is_empty());
        assert!(compilation.library_file_ids().contains(&socket_file_id));
        assert!(compilation.library_file_ids().contains(&net_file_id));
        assert!(
            compilation
                .main_workspace_file_ids()
                .contains(&main_file_id)
        );
        assert_eq!(socket_info.full_module_name.as_str(), "socket");
        assert_eq!(net_info.full_module_name.as_str(), "net");
        assert_ne!(socket_info.workspace_id, net_info.workspace_id);
    }

    #[test]
    fn test_compilation_module_and_type_indexes() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let file_ids = ws.def_files(vec![
            (
                "pkg/foo.lua",
                r#"
                local Foo = {}

                ---@internal
                return Foo
                "#,
            ),
            (
                "types/foo.lua",
                r#"
                ---@namespace Demo
                ---@class Foo
                local Foo = {}

                return Foo
                "#,
            ),
            (
                "consumer.lua",
                r#"
                ---@using Demo
                ---@class Bar: Foo
                local Bar = {}

                return Bar
                "#,
            ),
        ]);

        let foo_file_id = file_ids[0];
        let type_file_id = file_ids[1];
        let consumer_file_id = file_ids[2];

        let module_index = ws.analysis.compilation.module_index();
        let foo_module = module_index.get_module(foo_file_id).expect("foo module");
        assert_eq!(foo_module.full_module_name.as_str(), "pkg.foo");
        assert_eq!(foo_module.visible, CompilationModuleVisibility::Internal);
        assert_eq!(
            module_index
                .find_module("foo")
                .expect("fuzzy module lookup")
                .file_id,
            foo_file_id
        );

        let type_index = ws.analysis.compilation.type_index();
        assert_eq!(
            type_index
                .get_file_namespace(&type_file_id)
                .map(|namespace| namespace.as_str()),
            Some("Demo")
        );
        assert_eq!(
            type_index
                .get_file_using_namespace(&consumer_file_id)
                .expect("using namespaces")
                .iter()
                .map(|namespace| namespace.as_str())
                .collect::<Vec<_>>(),
            vec!["Demo"]
        );

        let foo_decl = type_index
            .find_type_decl(consumer_file_id, "Foo", Some(foo_module.workspace_id))
            .expect("Foo decl");
        assert_eq!(foo_decl.id.full_name.as_str(), "Demo.Foo");
        assert!(matches!(
            foo_decl.id.scope,
            CompilationTypeDeclScope::Internal(_)
        ));

        let bar_decl = type_index
            .find_type_decl(consumer_file_id, "Bar", Some(foo_module.workspace_id))
            .expect("Bar decl");
        assert_eq!(bar_decl.simple_name.as_str(), "Bar");
        assert_eq!(bar_decl.super_type_offsets.len(), 1);

        let visible =
            type_index.find_type_decls(consumer_file_id, "", Some(foo_module.workspace_id));
        assert_eq!(visible.get("Foo").cloned(), Some(Some(foo_decl.id.clone())));
        assert_eq!(visible.get("Bar").cloned(), Some(Some(bar_decl.id.clone())));
        assert_eq!(visible.get("Demo").cloned(), Some(None));

        let visible_demo =
            type_index.find_type_decls(consumer_file_id, "Demo.", Some(foo_module.workspace_id));
        assert_eq!(
            visible_demo.get("Foo").cloned(),
            Some(Some(CompilationTypeDeclId::internal(
                foo_module.workspace_id,
                "Demo.Foo"
            )))
        );
        assert!(
            type_index
                .get_visible_type_decls_by_full_name(
                    consumer_file_id,
                    "Demo.Foo",
                    Some(foo_module.workspace_id)
                )
                .iter()
                .any(|decl| decl.full_name() == "Demo.Foo")
        );
    }

    #[test]
    fn test_compilation_require_dependencies_ignore_plain_globals() {
        let mut ws = VirtualWorkspace::new();

        let file_ids = ws.def_files(vec![
            (
                "meta.lua",
                r#"
                vim = {}
                vim.o = {}
                "#,
            ),
            (
                "consumer.lua",
                r#"
                require("meta")
                local value = vim.o
                "#,
            ),
            (
                "global_only.lua",
                r#"
                local value = vim.o
                "#,
            ),
        ]);

        let meta_file_id = file_ids[0];
        let consumer_file_id = file_ids[1];
        let global_only_file_id = file_ids[2];

        let dependencies = ws
            .analysis
            .compilation
            .legacy_db()
            .get_file_dependencies_index();

        assert_eq!(
            dependencies
                .get_required_files(&consumer_file_id)
                .map(|deps| deps.iter().copied().collect::<Vec<_>>()),
            Some(vec![meta_file_id])
        );
        assert!(
            dependencies
                .get_required_files(&global_only_file_id)
                .is_none()
        );
    }

    #[test]
    fn test_compilation_require_export_helpers() {
        let mut ws = VirtualWorkspace::new();

        let subject_file_id = ws.def_file(
            "subject.lua",
            r#"
            ---@class Subject
            local subject = {}

            function subject.new()
            end

            return subject
            "#,
        );

        let module_file_id = ws
            .analysis
            .compilation
            .find_module_by_require_path("subject")
            .expect("subject module")
            .file_id;
        assert_eq!(module_file_id, subject_file_id);

        let export_type = ws
            .analysis
            .compilation
            .find_required_module_export_type("subject")
            .expect("subject export type");
        let require_type = ws.expr_ty("require('subject')");
        assert_eq!(export_type, require_type);

        let semantic_id = ws
            .analysis
            .compilation
            .find_required_module_semantic_id("subject")
            .expect("subject semantic id");
        assert!(matches!(semantic_id, LuaSemanticDeclId::LuaDecl(_)));
    }

    #[test]
    fn test_compilation_require_export_semantic_id_for_closure_module() {
        let mut ws = VirtualWorkspace::new();

        ws.def_file(
            "factory.lua",
            r#"
            return function()
            end
            "#,
        );

        let semantic_id = ws
            .analysis
            .compilation
            .find_required_module_semantic_id("factory")
            .expect("factory semantic id");
        assert!(matches!(semantic_id, LuaSemanticDeclId::Signature(_)));

        let export_type = ws
            .analysis
            .compilation
            .find_required_module_export_type("factory")
            .expect("factory export type");
        let require_type = ws.expr_ty("require('factory')");
        assert_eq!(export_type, require_type);
    }

    #[test]
    fn test_semantic_require_resolves_member_export_without_module_semantic_cache() {
        let mut ws = VirtualWorkspace::new();

        let file_ids = ws.def_files(vec![
            (
                "subject.lua",
                r#"
                local subject = { value = 1 }
                return subject.value
                "#,
            ),
            (
                "consumer.lua",
                r#"
                local exported = require("subject")
                local result = exported
                "#,
            ),
        ]);

        let consumer_file_id = file_ids[1];
        let require_type = ws.expr_ty("require('subject')");
        let integer_type = ws.ty("integer");
        assert!(ws.check_type(&require_type, &integer_type));

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(consumer_file_id)
            .expect("semantic model");
        let require_call = semantic_model
            .get_root()
            .descendants::<LuaCallExpr>()
            .find(|call_expr| call_expr.is_require())
            .expect("require call expr");

        let semantic_decl = semantic_model
            .find_decl(
                require_call.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )
            .expect("require semantic decl");
        assert!(matches!(semantic_decl, LuaSemanticDeclId::Member(_)));
    }

    #[test]
    fn test_infer_require_call_uses_compilation_module_export_type() {
        let mut ws = VirtualWorkspace::new();

        let file_ids = ws.def_files(vec![
            (
                "subject.lua",
                r#"
                local subject = { value = 1 }
                return subject.value
                "#,
            ),
            (
                "consumer.lua",
                r#"
                local exported = require("subject")
                return exported
                "#,
            ),
        ]);

        let consumer_file_id = file_ids[1];
        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(consumer_file_id)
            .expect("semantic model");
        let require_call = semantic_model
            .get_root()
            .descendants::<LuaCallExpr>()
            .find(|call_expr| call_expr.is_require())
            .expect("require call expr");

        let require_type = semantic_model
            .infer_expr(require_call.clone().into())
            .expect("require inferred type");
        let integer_type = ws.ty("integer");
        assert!(ws.check_type(&require_type, &integer_type));
    }

    #[test]
    fn test_semantic_module_export_member_decl_uses_compilation_fallback() {
        let mut ws = VirtualWorkspace::new();

        let subject_file_id = ws.def_file(
            "subject.lua",
            r#"
            local subject = { value = 1 }
            return subject.value
            "#,
        );

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(subject_file_id)
            .expect("semantic model");
        let export_expr = semantic_model
            .get_root()
            .descendants::<LuaIndexExpr>()
            .last()
            .expect("export index expr");

        let semantic_decl = semantic_model
            .find_decl(
                export_expr.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )
            .expect("export semantic decl");
        assert!(matches!(semantic_decl, LuaSemanticDeclId::Member(_)));

        let info = semantic_model
            .get_semantic_info(export_expr.syntax().clone().into())
            .expect("export semantic info");
        assert!(matches!(
            info.semantic_decl,
            Some(LuaSemanticDeclId::Member(_))
        ));
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
        assert!(ws.check_code_for(
            DiagnosticCode::AccessInvisible,
            r#"
                local export = require("virtual_0")
                export.hidden = 1
                "#,
        ));
    }
}
