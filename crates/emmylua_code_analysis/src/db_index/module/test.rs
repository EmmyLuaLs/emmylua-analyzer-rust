#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{db_index::{module::LuaModuleIndex, traits::LuaIndex}, FileId, WorkspaceId};

    fn create_module() -> LuaModuleIndex {
        let mut m = LuaModuleIndex::new();
        m.set_module_extract_patterns(["?.lua".to_string(), "?/init.lua".to_string()].to_vec());
        m
    }

    #[test]
    fn test_basic() {
        let mut m = create_module();
        m.add_workspace_root(Path::new("C:/Users/username/Documents").into(), WorkspaceId::MAIN);
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test");
        assert_eq!(module_info.full_module_name, "test");
        assert_eq!(module_info.visible, true);

        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test2/init.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test2");
        assert_eq!(module_info.full_module_name, "test2");
        assert_eq!(module_info.visible, true);

        let file_id = FileId { id: 3 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test3/hhhhiii.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "hhhhiii");
        assert_eq!(module_info.full_module_name, "test3.hhhhiii");
        assert_eq!(module_info.visible, true);
    }

    #[test]
    fn test_multi_workspace() {
        let mut m = create_module();
        m.add_workspace_root(Path::new("C:/Users/username/Documents").into(), WorkspaceId::MAIN);
        m.add_workspace_root(Path::new("C:/Users/username/Downloads").into(), WorkspaceId::MAIN);
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test");
        assert_eq!(module_info.full_module_name, "test");
        assert_eq!(module_info.visible, true);

        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Downloads/test2/init.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "test2");
        assert_eq!(module_info.full_module_name, "test2");
        assert_eq!(module_info.visible, true);

        let file_id = FileId { id: 3 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test3/hhhhiii.lua");
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.name, "hhhhiii");
        assert_eq!(module_info.full_module_name, "test3.hhhhiii");
        assert_eq!(module_info.visible, true);
    }

    #[test]
    fn test_find_module() {
        let mut m = create_module();
        m.add_workspace_root(Path::new("C:/Users/username/Documents").into(), WorkspaceId::MAIN);
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        let module_info = m.find_module("test").unwrap();
        assert_eq!(module_info.name, "test");
        assert_eq!(module_info.full_module_name, "test");
        assert_eq!(module_info.visible, true);

        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test2/init.lua");
        let module_info = m.find_module("test2").unwrap();
        assert_eq!(module_info.name, "test2");
        assert_eq!(module_info.full_module_name, "test2");
        assert_eq!(module_info.visible, true);

        let file_id = FileId { id: 3 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test3/hhhhiii.lua");
        let module_info = m.find_module("test3.hhhhiii").unwrap();
        assert_eq!(module_info.name, "hhhhiii");
        assert_eq!(module_info.full_module_name, "test3.hhhhiii");
        assert_eq!(module_info.visible, true);

        let not_found = m.find_module("test3.hhhhiii.notfound");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_module_node() {
        let mut m = create_module();
        m.add_workspace_root(Path::new("C:/Users/username/Documents").into(), WorkspaceId::MAIN);
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
        m.add_workspace_root(Path::new("C:/Users/username/Documents").into(), WorkspaceId::MAIN);
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        m.set_module_visibility(file_id, false);
        let module_info = m.get_module(file_id).unwrap();
        assert_eq!(module_info.visible, false);
    }

    #[test]
    fn test_remove_module() {
        let mut m = create_module();
        m.add_workspace_root(Path::new("C:/Users/username/Documents").into(), WorkspaceId::MAIN);
        let file_id = FileId { id: 1 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test.lua");
        m.remove(file_id);
        let module_info = m.get_module(file_id);
        assert!(module_info.is_none());

        let file_id = FileId { id: 2 };
        m.add_module_by_path(file_id, "C:/Users/username/Documents/test2/aaa/bbb/cccc/dddd.lua");
        m.remove(file_id);
        let module_info = m.get_module(file_id);
        assert!(module_info.is_none());
        let module_node = m.find_module_node("test2.aaa");
        assert!(module_node.is_none());
    }
}