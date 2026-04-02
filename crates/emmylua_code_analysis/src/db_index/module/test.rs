#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc};

    use emmylua_parser::VisibilityKind;

    use crate::{
        Emmyrc, FileId, WorkspaceId,
        db_index::{
            module::{LuaModuleIndex, ModuleVisibility},
            traits::LuaIndex,
        },
    };

    fn create_module() -> LuaModuleIndex {
        let mut m = LuaModuleIndex::new();
        m.set_module_extract_patterns(["?.lua".to_string(), "?/init.lua".to_string()].to_vec());
        m
    }

    #[test]
    fn test_basic() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test");
        assert_eq!(module_info.full_module_name, "test");
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test2/init.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test2");
        assert_eq!(module_info.full_module_name, "test2");
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        let file_id = FileId { id: 3 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test3/hhhhiii.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "hhhhiii");
        assert_eq!(module_info.full_module_name, "test3.hhhhiii");
        assert_eq!(module_info.visible, ModuleVisibility::Default);
    }

    #[test]
    fn test_multi_workspace() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        m.add_workspace_root(
            Path::new("C:/Users/username/Downloads").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test");
        assert_eq!(module_info.full_module_name, "test");
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Downloads/test2/init.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test2");
        assert_eq!(module_info.full_module_name, "test2");
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        let file_id = FileId { id: 3 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test3/hhhhiii.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "hhhhiii");
        assert_eq!(module_info.full_module_name, "test3.hhhhiii");
        assert_eq!(module_info.visible, ModuleVisibility::Default);
    }

    #[test]
    fn test_find_module() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        let module_info = m.find_module("test").unwrap();
        assert_eq!(module_info.name, "test");
        assert_eq!(module_info.full_module_name, "test");
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test2/init.lua");
        let module_info = m.find_module("test2").unwrap();
        assert_eq!(module_info.name, "test2");
        assert_eq!(module_info.full_module_name, "test2");
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        let file_id = FileId { id: 3 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test3/hhhhiii.lua");
        let module_info = m.find_module("test3.hhhhiii").unwrap();
        assert_eq!(module_info.name, "hhhhiii");
        assert_eq!(module_info.full_module_name, "test3.hhhhiii");
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        let not_found = m.find_module("test3.hhhhiii.notfound");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_module_node() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test/aaa.lua");
        let file_id = FileId { id: 3 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test/hhhhiii.lua");

        let module_node = m.find_module_node("test").unwrap();
        assert_eq!(module_node.children.len(), 2);
        let first_child = module_node.children.get("aaa");
        assert!(first_child.is_some());
        let second_child = module_node.children.get("hhhhiii");
        assert!(second_child.is_some());
    }

    #[test]
    fn test_set_module_visibility() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        m.set_module_visibility(file_id, ModuleVisibility::Hide);
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.visible, ModuleVisibility::Hide);
    }

    #[test]
    fn test_remove_module() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        m.remove(file_id);
        let module_info = m.get_module(file_id);
        assert!(module_info.is_none());

        let file_id = FileId { id: 2 };
        m.add_module_by_path(
            file_id,
            "C:/Users/username/Documents/test2/aaa/bbb/cccc/dddd.lua",
        );
        m.remove(file_id);
        let module_info = m.get_module(file_id);
        assert!(module_info.is_none());
        let module_node = m.find_module_node("test2.aaa");
        assert!(module_node.is_none());
    }

    #[test]
    fn test_require_fuzzy_match_honors_segment_boundaries() {
        let mut m = LuaModuleIndex::new();
        m.update_config(Arc::new(Emmyrc::default()));
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );

        let file_id = FileId { id: 1 };
        m.add_module_by_path(
            file_id,
            "C:/Users/username/Documents/nvim-cmp/lua/cmp/utils/event.lua",
        );

        assert!(m.find_module("pckr.event").is_none());
        let module_info = m.find_module("event").unwrap();
        assert_eq!(module_info.full_module_name, "nvim-cmp.lua.cmp.utils.event");
    }

    #[test]
    fn test_require_fuzzy_match_prefers_shortest_prefix_independent_of_insert_order() {
        const PLUGIN_ENTRY: &str = "C:/Users/username/Documents/plugin/treesitter-context.lua";
        const LUA_ENTRY: &str = "C:/Users/username/Documents/lua/treesitter-context.lua";

        // Validate both insertion orders to ensure lookup does not depend on indexing order.
        for paths in [[PLUGIN_ENTRY, LUA_ENTRY], [LUA_ENTRY, PLUGIN_ENTRY]] {
            let mut m = LuaModuleIndex::new();
            m.update_config(Arc::new(Emmyrc::default()));
            m.add_workspace_root(
                Path::new("C:/Users/username/Documents").into(),
                WorkspaceId::MAIN,
            );

            for (file_id, path) in [FileId { id: 1 }, FileId { id: 2 }].into_iter().zip(paths) {
                m.add_module_by_path(file_id, path);
            }

            let module_info = m.find_module("treesitter-context").unwrap();
            assert_eq!(module_info.full_module_name, "lua.treesitter-context");
        }
    }

    #[test]
    fn test_merge_visibility_treats_default_as_neutral_state() {
        assert_eq!(
            ModuleVisibility::Default.merge(ModuleVisibility::Default),
            ModuleVisibility::Default
        );
        assert_eq!(
            ModuleVisibility::Default.merge(ModuleVisibility::Internal),
            ModuleVisibility::Internal
        );
        assert_eq!(
            ModuleVisibility::Default.merge(ModuleVisibility::Public),
            ModuleVisibility::Public
        );
        assert_eq!(
            ModuleVisibility::Internal.merge(ModuleVisibility::Internal),
            ModuleVisibility::Internal
        );
        assert_eq!(
            ModuleVisibility::Public.merge(ModuleVisibility::Internal),
            ModuleVisibility::Internal
        );
        assert_eq!(
            ModuleVisibility::Internal.merge(ModuleVisibility::Public),
            ModuleVisibility::Public
        );
        assert_eq!(
            ModuleVisibility::Public.merge(ModuleVisibility::Default),
            ModuleVisibility::Public
        );
        assert_eq!(
            ModuleVisibility::Internal.merge(ModuleVisibility::Default),
            ModuleVisibility::Internal
        );
        assert_eq!(
            ModuleVisibility::Hide.merge(ModuleVisibility::Public),
            ModuleVisibility::Hide
        );
        assert_eq!(
            ModuleVisibility::Public.merge(ModuleVisibility::Hide),
            ModuleVisibility::Hide
        );
    }

    #[test]
    fn test_module_visibility_source_has_higher_priority_than_return_visibility() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        m.set_module_visibility(file_id, ModuleVisibility::Hide);

        let module_info = m.get_module_mut(file_id).unwrap();
        module_info.merge_visibility(VisibilityKind::Public);
        assert_eq!(module_info.visible, ModuleVisibility::Hide);

        module_info.merge_visibility(VisibilityKind::Internal);
        assert_eq!(module_info.visible, ModuleVisibility::Hide);
    }

    #[test]
    fn test_return_visibility_uses_latest_explicit_state() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");

        let module_info = m.get_module_mut(file_id).unwrap();
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        module_info.merge_visibility(VisibilityKind::Internal);
        assert_eq!(module_info.visible, ModuleVisibility::Internal);

        module_info.merge_visibility(VisibilityKind::Public);
        assert_eq!(module_info.visible, ModuleVisibility::Public);
    }

    #[test]
    fn test_explicit_internal_can_narrow_public_default_visibility() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");

        let module_info = m.get_module_mut(file_id).unwrap();
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        module_info.merge_visibility(VisibilityKind::Internal);
        assert_eq!(module_info.visible, ModuleVisibility::Internal);
    }

    #[test]
    fn test_explicit_public_preserves_default_public_visibility() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");

        let module_info = m.get_module_mut(file_id).unwrap();
        assert_eq!(module_info.visible, ModuleVisibility::Default);

        module_info.merge_visibility(VisibilityKind::Public);
        assert_eq!(module_info.visible, ModuleVisibility::Public);
    }

    #[test]
    fn test_default_public_visibility_is_requireable_across_workspaces() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");

        let module_info = m.get_module(file_id).unwrap();
        assert!(module_info.is_requireable_from(WorkspaceId::MAIN));
        assert!(module_info.is_requireable_from(WorkspaceId { id: 99 }));
    }
}
