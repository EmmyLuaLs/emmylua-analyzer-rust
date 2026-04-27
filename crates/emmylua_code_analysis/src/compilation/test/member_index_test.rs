#[cfg(test)]
mod test {
    use crate::{CompilationMemberFeature, VirtualWorkspace, WorkspaceId};

    #[test]
    fn test_compilation_member_index_aggregates_type_properties_and_runtime_members() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let file_id = ws.def_file(
            "a.lua",
            r#"
            ---@class Foo
            ---@field id integer
            local Foo = {}

            return Foo
            "#,
        );

        ws.def_file(
            "b.lua",
            r#"
            ---@class Foo
            local Foo = {}

            function Foo:run()
            end

            return Foo
            "#,
        );

        let type_decl = ws
            .analysis
            .compilation
            .type_index()
            .find_type_decl(file_id, "Foo", Some(WorkspaceId::MAIN))
            .expect("Foo type decl");
        let members = ws
            .analysis
            .compilation
            .member_index()
            .get_owner_members(&type_decl.id)
            .expect("Foo member contributions");

        assert!(members.contains_key("id"));
        assert!(members.contains_key("run"));
    }

    #[test]
    fn test_compilation_member_index_prefers_meta_field_decl_over_file_define() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def_file(
            "meta.lua",
            r#"
            ---@meta
            ---@class Foo
            ---@field id integer
            local Foo = {}

            return Foo
            "#,
        );

        let file_id = ws.def_file(
            "runtime.lua",
            r#"
            ---@class Foo
            local Foo = {}
            Foo.id = 1

            return Foo
            "#,
        );

        let type_decl = ws
            .analysis
            .compilation
            .type_index()
            .find_type_decl(file_id, "Foo", Some(WorkspaceId::MAIN))
            .expect("Foo type decl");
        let definition = ws
            .analysis
            .compilation
            .member_index()
            .get_definition_member(&type_decl.id, "id", true)
            .expect("preferred Foo.id definition");

        assert_eq!(definition.feature, CompilationMemberFeature::MetaFieldDecl);
    }

    #[test]
    fn test_compilation_member_index_merges_super_members() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let file_ids = ws.def_files(vec![
            (
                "base.lua",
                r#"
                ---@class Base
                ---@field id integer
                local Base = {}

                return Base
                "#,
            ),
            (
                "child.lua",
                r#"
                ---@class Child: Base
                local Child = {}

                function Child:run()
                end

                return Child
                "#,
            ),
        ]);

        let base_file_id = file_ids[0];
        let child_file_id = file_ids[1];
        let compilation = &ws.analysis.compilation;
        let type_index = compilation.type_index();
        let base_decl = type_index
            .find_type_decl(base_file_id, "Base", Some(WorkspaceId::MAIN))
            .expect("Base type decl");
        let child_decl = type_index
            .find_type_decl(child_file_id, "Child", Some(WorkspaceId::MAIN))
            .expect("Child type decl");

        let merged =
            compilation
                .member_index()
                .get_merged_owner_members(type_index, &child_decl.id, true);

        assert_eq!(
            merged.get("id").expect("inherited id member").owner,
            base_decl.id
        );
        assert_eq!(
            merged.get("run").expect("child run member").owner,
            child_decl.id
        );
    }

    #[test]
    fn test_compilation_member_index_merges_partial_decl_super_members() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let file_ids = ws.def_files(vec![
            (
                "base.lua",
                r#"
                ---@class Base
                ---@field id integer
                local Base = {}

                return Base
                "#,
            ),
            (
                "child_decl.lua",
                r#"
                ---@class Child: Base
                local Child = {}

                return Child
                "#,
            ),
            (
                "child_member.lua",
                r#"
                ---@class Child
                local Child = {}

                function Child:run()
                end

                return Child
                "#,
            ),
        ]);

        let child_file_id = file_ids[2];
        let compilation = &ws.analysis.compilation;
        let type_index = compilation.type_index();
        let child_decl = type_index
            .find_type_decl(child_file_id, "Child", Some(WorkspaceId::MAIN))
            .expect("Child type decl");

        let inherited_id =
            compilation
                .member_index()
                .get_merged_member(type_index, &child_decl.id, "id", true);
        let direct_run =
            compilation
                .member_index()
                .get_merged_member(type_index, &child_decl.id, "run", true);

        assert_eq!(
            inherited_id.expect("partial inherited id").name.as_str(),
            "id"
        );
        assert_eq!(direct_run.expect("partial direct run").name.as_str(), "run");
    }

    #[test]
    fn test_compilation_facade_uses_meta_override_config() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def_file(
            "meta.lua",
            r#"
            ---@meta
            ---@class Foo
            ---@field id integer
            local Foo = {}

            return Foo
            "#,
        );

        let file_id = ws.def_file(
            "runtime.lua",
            r#"
            ---@class Foo
            local Foo = {}
            Foo.id = 1

            return Foo
            "#,
        );

        let compilation = &ws.analysis.compilation;
        let definition = compilation
            .find_type_merged_member(file_id, "Foo", Some(WorkspaceId::MAIN), "id")
            .expect("default merged Foo.id definition");
        assert_eq!(definition.feature, CompilationMemberFeature::MetaFieldDecl);

        let mut emmyrc = ws.get_emmyrc();
        emmyrc.strict.meta_override_file_define = false;
        ws.update_emmyrc(emmyrc);

        let definition = ws
            .analysis
            .compilation
            .find_type_merged_member(file_id, "Foo", Some(WorkspaceId::MAIN), "id")
            .expect("config-driven merged Foo.id definition");
        assert_eq!(definition.feature, CompilationMemberFeature::FileDefine);
    }
}
