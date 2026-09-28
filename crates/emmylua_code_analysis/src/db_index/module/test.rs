#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc};

    use emmylua_parser::VisibilityKind;

    use crate::{
        Emmyrc, EmmyrcWorkspaceModuleMap, FileId, WorkspaceId,
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
    fn test_repeated_module_node_registration_is_removed_once() {
        let mut m = create_module();
        let file = FileId { id: 1 };
        let sibling = FileId { id: 2 };
        m.add_module_by_module_path(file, "shared.helper".into(), WorkspaceId::MAIN);
        m.add_module_by_module_path(sibling, "shared.helper".into(), WorkspaceId::MAIN);
        let node = m.add_module_node(file, "shared.helper").unwrap();
        m.add_module_node(file, "shared.helper");
        assert_eq!(m.file_module_nodes[&file], vec![node]);
        assert_eq!(
            m.get_module_node(&node).unwrap().file_ids,
            vec![file, sibling]
        );
        m.remove(file);
        assert_eq!(m.find_module("shared.helper").unwrap().file_id, sibling);
        m.remove(sibling);
        assert!(m.find_module_node("shared").is_none());
    }

    #[test]
    fn test_remove_and_rename_preserve_shared_fuzzy_name_bucket() {
        let mut m = create_module();
        m.fuzzy_search = true;
        for root in ["/project", "/project/a", "/project/b"] {
            m.add_workspace_root(Path::new(root).into(), WorkspaceId::MAIN);
        }
        m.set_module_replace_patterns(vec![("^([ab])[.](.*)$".into(), "package_$1.$2".into())]);
        let a = FileId { id: 1 };
        let b = FileId { id: 2 };
        let other = FileId { id: 3 };
        m.add_module_by_path(a, "/project/a/lib/helper.lua");
        m.add_module_by_path(b, "/project/b/lib/helper.lua");
        m.add_module_by_path(other, "/project/a/lib/other.lua");
        assert_eq!(m.module_name_to_file_ids["helper"], vec![a, b]);

        m.add_module_by_path(a, "/project/a/lib/renamed.lua");
        assert_eq!(m.module_name_to_file_ids["helper"], vec![b]);
        assert_eq!(m.find_module("helper").unwrap().file_id, b);
        assert!(m.find_module_node("package_a.lib.helper").is_none());
        assert_eq!(m.find_module("package_a.lib.renamed").unwrap().file_id, a);

        m.remove(b);
        m.remove(b);
        assert!(!m.module_name_to_file_ids.contains_key("helper"));
        assert!(m.find_module("helper").is_none());
        assert!(m.find_module_node("package_b").is_none());
        assert_eq!(m.module_name_to_file_ids["other"], vec![other]);
        assert_eq!(m.find_module("other").unwrap().file_id, other);
        m.remove(a);
        assert!(!m.module_name_to_file_ids.contains_key("renamed"));
        assert_eq!(m.module_name_to_file_ids.len(), 1);
    }

    #[test]
    fn test_overlapping_roots_apply_module_mapping_once() {
        for reverse in [false, true] {
            for relative in ["helper.lua", "helper/init.lua"] {
                let mut m = create_module();
                let mut roots = vec!["/project", "/project/a"];
                if reverse {
                    roots.reverse();
                }
                for root in roots {
                    m.add_workspace_root(Path::new(root).into(), WorkspaceId::MAIN);
                }
                // A second rewrite would add another prefix, making it observable.
                m.set_module_replace_patterns(vec![("^(.*)$".into(), "mapped.$1".into())]);
                let path = format!("/project/a/{relative}");
                assert_eq!(m.match_pattern(relative).as_deref(), Some("helper"));
                assert_eq!(m.extract_module_path(&path).unwrap().0, "helper");
                let file = FileId { id: 1 };
                m.add_module_by_path(file, &path);
                assert_eq!(
                    m.get_module(file).unwrap().full_module_name,
                    "mapped.helper"
                );
                for name in ["mapped.helper", "mapped.a.helper"] {
                    assert_eq!(m.find_module_node(name).unwrap().file_ids, vec![file]);
                    assert_eq!(m.find_module(name).unwrap().file_id, file);
                    assert!(m.find_module_node(&format!("mapped.{name}")).is_none());
                }
                assert_eq!(m.find_module("helper").unwrap().file_id, file);
                assert_eq!(m.find_module("a.helper").unwrap().file_id, file);
                assert_eq!(m.file_module_nodes[&file].len(), 2);
            }
        }
    }

    #[test]
    fn test_source_root_selection_is_independent_of_workspace_order() {
        for roots in [
            ["/project/a", "/project/b", "/project"],
            ["/project", "/project/b", "/project/a"],
        ] {
            let mut m = create_module();
            for root in roots {
                m.add_workspace_root(Path::new(root).into(), WorkspaceId::MAIN);
            }
            // The same root can have distinct import filters, but its path is
            // still the same resolution scope when both filters accept a file.
            m.add_workspace_root_with_import(
                Path::new("/project/a").into(),
                crate::WorkspaceImport::Package("lib".into()),
                WorkspaceId::LIBRARY_START,
            );
            let a = FileId { id: 1 };
            let b = FileId { id: 2 };
            m.add_module_by_path(b, "/project/b/lib/helper.lua");
            m.add_module_by_path(a, "/project/a/lib/helper.lua");
            assert_eq!(m.file_workspace_roots[&a], Path::new("/project/a"));
            assert_eq!(m.file_workspace_roots[&b], Path::new("/project/b"));
            assert_eq!(m.find_module_from("lib.helper", a).unwrap().file_id, a);
            assert_eq!(m.find_module_from("lib.helper", b).unwrap().file_id, b);
        }
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
    fn test_module_map_applies_to_factorio_require_paths() {
        let mut config = Emmyrc::default();
        config.workspace.module_map = vec![
            EmmyrcWorkspaceModuleMap {
                pattern: "^__(.*)__(.*)$".to_string(),
                replace: "$1$2".to_string(),
            },
            EmmyrcWorkspaceModuleMap {
                pattern: "^(.*)\\.lua$".to_string(),
                replace: "$1".to_string(),
            },
        ];

        let mut m = LuaModuleIndex::new();
        m.update_config(Arc::new(config));
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents/mods").into(),
            WorkspaceId::MAIN,
        );

        let file_id = FileId { id: 1 };
        m.add_module_by_path(
            file_id,
            "C:/Users/username/Documents/mods/signalstrings/signalstrings.lua",
        );

        for module_path in [
            "__signalstrings__/signalstrings.lua",
            "__signalstrings__.signalstrings",
            "__signalstrings__/signalstrings",
        ] {
            let module_info = m.find_module(module_path).unwrap();
            assert_eq!(module_info.file_id, file_id);
            assert_eq!(module_info.full_module_name, "signalstrings.signalstrings");
        }
    }

    #[test]
    fn test_module_map_keeps_configured_rule_order() {
        let mut config = Emmyrc::default();
        config.workspace.module_map = vec![
            EmmyrcWorkspaceModuleMap {
                pattern: "^foo$".to_string(),
                replace: "bar".to_string(),
            },
            EmmyrcWorkspaceModuleMap {
                pattern: "^bar$".to_string(),
                replace: "baz".to_string(),
            },
        ];

        let mut m = LuaModuleIndex::new();
        m.update_config(Arc::new(config));
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );

        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/bar.lua");

        let module_info = m.find_module("foo").unwrap();
        assert_eq!(module_info.file_id, file_id);
        assert_eq!(module_info.full_module_name, "baz");
    }

    #[test]
    fn test_module_map_exact_match_has_priority_over_fuzzy_match() {
        let mut config = Emmyrc::default();
        config.workspace.module_map = vec![EmmyrcWorkspaceModuleMap {
            pattern: "^foo$".to_string(),
            replace: "bar.baz".to_string(),
        }];

        let mut m = LuaModuleIndex::new();
        m.update_config(Arc::new(config));
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );

        let mapped_file_id = FileId { id: 1 };
        m.add_module_by_path(mapped_file_id, "C:/Users/username/Documents/bar/baz.lua");

        let fuzzy_file_id = FileId { id: 2 };
        m.add_module_by_path(fuzzy_file_id, "C:/Users/username/Documents/x/foo.lua");

        let module_info = m.find_module("foo").unwrap();
        assert_eq!(module_info.file_id, mapped_file_id);
        assert_eq!(module_info.full_module_name, "bar.baz");
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

    #[test]
    fn test_sibling_packages_under_same_parent_keep_distinct_package_scopes() {
        let mut m = create_module();
        m.add_workspace_root_with_import(
            Path::new("C:/Users/username/Documents/module").into(),
            crate::WorkspaceImport::Package("socket".into()),
            WorkspaceId { id: 3 },
        );
        m.add_workspace_root_with_import(
            Path::new("C:/Users/username/Documents/module").into(),
            crate::WorkspaceImport::Package("net".into()),
            WorkspaceId { id: 4 },
        );

        let socket_file = FileId { id: 1 };
        let net_file = FileId { id: 2 };
        m.add_module_by_path(
            socket_file,
            "C:/Users/username/Documents/module/socket/init.lua",
        );
        m.add_module_by_path(net_file, "C:/Users/username/Documents/module/net/init.lua");
        m.set_module_visibility(socket_file, ModuleVisibility::Internal);
        m.set_module_visibility(net_file, ModuleVisibility::Internal);

        let socket_info = m.get_module(socket_file).unwrap();
        let net_info = m.get_module(net_file).unwrap();

        assert_eq!(socket_info.full_module_name, "socket");
        assert_eq!(net_info.full_module_name, "net");
        assert_eq!(socket_info.workspace_id, WorkspaceId { id: 3 });
        assert_eq!(net_info.workspace_id, WorkspaceId { id: 4 });
        assert_ne!(socket_info.workspace_id, net_info.workspace_id);
        assert!(!socket_info.is_requireable_from(net_info.workspace_id));
        assert!(!net_info.is_requireable_from(socket_info.workspace_id));
    }

    #[test]
    fn test_find_module_prefers_non_hidden_candidate_when_multiple_modules_share_name() {
        let mut m = create_module();
        m.add_workspace_root(
            Path::new("C:/Users/username/Documents").into(),
            WorkspaceId::MAIN,
        );
        m.add_workspace_root(
            Path::new("C:/Users/username/Downloads").into(),
            WorkspaceId::MAIN,
        );

        let hidden_file_id = FileId { id: 1 };
        m.add_module_by_path(hidden_file_id, "C:/Users/username/Documents/test.lua");
        m.set_module_visibility(hidden_file_id, ModuleVisibility::Hide);

        let visible_file_id = FileId { id: 2 };
        m.add_module_by_path(visible_file_id, "C:/Users/username/Downloads/test.lua");

        let module_info = m.find_module("test").unwrap();
        assert_eq!(module_info.file_id, visible_file_id);
        assert_eq!(module_info.visible, ModuleVisibility::Default);
    }
    #[test]
    fn test_require_prefers_callers_root_in_either_index_order() {
        for reverse in [false, true] {
            let mut m = create_module();
            // Both source roots have the same MAIN id and the same directory name.
            for root in ["/project", "/one/mod", "/two/mod"] {
                m.add_workspace_root(Path::new(root).into(), WorkspaceId::MAIN);
            }
            let caller_a = FileId { id: 1 };
            let caller_b = FileId { id: 2 };
            m.add_module_by_path(caller_a, "/one/mod/scripts/control.lua");
            m.add_module_by_path(caller_b, "/two/mod/scripts/control.lua");
            let mut helpers = vec![
                (FileId { id: 3 }, "/one/mod/helper.lua"),
                (FileId { id: 4 }, "/two/mod/helper.lua"),
            ];
            if reverse {
                helpers.reverse();
            }
            for (id, path) in helpers {
                m.add_module_by_path(id, path);
            }
            assert_eq!(
                m.find_module_from("helper", caller_a).unwrap().file_id,
                FileId { id: 3 }
            );
            assert_eq!(
                m.find_module_from("helper", caller_b).unwrap().file_id,
                FileId { id: 4 }
            );
            // The root is the configured source root, not the caller's scripts directory.
            assert!(m.find_module_from("scripts.helper", caller_a).is_none());
        }
    }

    #[test]
    fn test_mapped_aliases_survive_overlapping_roots_and_are_removed() {
        let mut m = create_module();
        m.add_workspace_root(Path::new("/project").into(), WorkspaceId::MAIN);
        for root in ["/project/a", "/project/b"] {
            m.add_workspace_root(Path::new(root).into(), WorkspaceId::MAIN);
        }
        m.set_module_replace_patterns(vec![("^([ab])[.](.*)$".into(), "package_$1.$2".into())]);
        let a = FileId { id: 1 };
        let b = FileId { id: 2 };
        m.add_module_by_path(a, "/project/a/helper.lua");
        m.add_module_by_path(b, "/project/b/helper.lua");
        assert_eq!(m.get_module(a).unwrap().full_module_name, "helper");
        assert_eq!(m.find_module_from("helper", a).unwrap().file_id, a);
        assert_eq!(
            m.find_module_from("package_b/helper", a).unwrap().file_id,
            b
        );
        assert_eq!(
            m.find_module_node("package_b.helper").unwrap().file_ids,
            vec![b]
        );
        assert_eq!(m.find_module("a.helper").unwrap().file_id, a);
        m.add_module_by_path(b, "/project/b/renamed.lua");
        assert!(m.find_module("package_b.helper").is_none());
        assert_eq!(
            m.find_module_from("package_b.renamed", a).unwrap().file_id,
            b
        );
        m.remove(b);
        assert!(m.find_module_node("package_b").is_none());
        assert!(m.find_module("renamed").is_none());
        assert_eq!(m.find_module("helper").unwrap().file_id, a);
        m.clear();
        assert!(m.find_module("package_a.helper").is_none());
        assert!(m.file_workspace_roots.is_empty());
        assert!(m.file_module_nodes.is_empty());
    }

    #[test]
    fn test_fuzzy_require_prefers_local_root_and_retains_library_fallback() {
        let mut m = create_module();
        m.fuzzy_search = true;
        for root in ["/a", "/b"] {
            m.add_workspace_root(Path::new(root).into(), WorkspaceId::MAIN);
        }
        let a = FileId { id: 1 };
        let b = FileId { id: 2 };
        m.add_module_by_path(a, "/a/control.lua");
        m.add_module_by_path(b, "/b/control.lua");
        m.add_module_by_path(FileId { id: 3 }, "/a/lib/helper.lua");
        m.add_module_by_path(FileId { id: 4 }, "/b/lib/helper.lua");
        assert_eq!(
            m.find_module_from("helper", a).unwrap().file_id,
            FileId { id: 3 }
        );
        assert_eq!(
            m.find_module_from("helper", b).unwrap().file_id,
            FileId { id: 4 }
        );
        m.add_workspace_root(Path::new("/library").into(), WorkspaceId::LIBRARY_START);
        m.add_module_by_path(FileId { id: 5 }, "/library/shared.lua");
        assert_eq!(
            m.find_module_from("shared", a).unwrap().file_id,
            FileId { id: 5 }
        );
        m.remove(FileId { id: 3 });
        m.remove(FileId { id: 4 });
        assert!(m.find_module_from("helper", a).is_none());
        assert!(!m.module_name_to_file_ids.contains_key("helper"));
    }

    #[test]
    fn test_nested_root_prefers_nearest_workspace_and_explicit_module_keeps_scope() {
        let mut m = create_module();
        m.add_workspace_root(Path::new("/project").into(), WorkspaceId::MAIN);
        m.add_workspace_root(Path::new("/project/nested").into(), WorkspaceId::MAIN);
        let caller = FileId { id: 1 };
        let helper = FileId { id: 2 };
        m.add_module_by_path(FileId { id: 3 }, "/project/helper.lua");
        m.add_module_by_path(caller, "/project/nested/control.lua");
        m.add_module_by_path(helper, "/project/nested/helper.lua");
        assert_eq!(
            m.find_module_from("helper", caller).unwrap().file_id,
            helper
        );
        m.add_module_by_module_path(caller, "explicit_control".into(), WorkspaceId::MAIN);
        assert_eq!(
            m.find_module_from("helper", caller).unwrap().file_id,
            helper
        );
    }
}
