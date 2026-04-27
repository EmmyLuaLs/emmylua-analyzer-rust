#[cfg(test)]
mod test {
    use rowan::TextSize;

    use crate::VirtualWorkspace;

    #[test]
    fn test_compilation_decl_index_find_local_decl_prefers_nearest_visible_local() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def_file(
            "decl_index_shadow.lua",
            r#"
local value = 1
do
    local value = 2
    print(value)
end
"#,
        );

        let source = ws
            .analysis
            .compilation
            .legacy_db()
            .get_vfs()
            .get_file_content(&file_id)
            .expect("file content")
            .to_string();
        let offset = source.rfind("value)").expect("shadowed value offset") as u32;
        let decl_tree = ws
            .analysis
            .compilation
            .decl_index()
            .get_decl_tree(file_id)
            .expect("compilation decl tree");
        let decl = decl_tree
            .find_local_decl("value", TextSize::from(offset))
            .expect("nearest visible local decl");
        let inner_name_offset = source.find("value = 2").expect("inner name token") as u32;
        let outer_name_offset = source.find("value = 1").expect("outer name token") as u32;

        assert_eq!(decl.name.as_str(), "value");
        assert!(decl.start_offset > TextSize::from(outer_name_offset));
        assert!(decl.start_offset < TextSize::from(offset));
        assert_eq!(decl.start_offset, TextSize::from(inner_name_offset));
    }

    #[test]
    fn test_compilation_decl_index_get_env_decls_collects_visible_locals() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def_file(
            "decl_index_env.lua",
            r#"
local outer = 1
do
    local inner = 2
    print(outer + inner)
end
"#,
        );

        let source = ws
            .analysis
            .compilation
            .legacy_db()
            .get_vfs()
            .get_file_content(&file_id)
            .expect("file content")
            .to_string();
        let offset = source.rfind("outer + inner").expect("env lookup offset") as u32;
        let decl_tree = ws
            .analysis
            .compilation
            .decl_index()
            .get_decl_tree(file_id)
            .expect("compilation decl tree");

        let env_decl_names = decl_tree
            .get_env_decls(TextSize::from(offset))
            .expect("visible env decls")
            .into_iter()
            .filter_map(|decl_id| decl_tree.get_decl(&decl_id))
            .map(|decl| decl.name.to_string())
            .collect::<Vec<_>>();

        assert!(env_decl_names.iter().any(|name| name == "outer"));
        assert!(env_decl_names.iter().any(|name| name == "inner"));
    }
}
