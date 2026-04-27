#[cfg(test)]
mod tests {
    use crate::{
        LuaAstNode, LuaCallExpr, LuaIndexExpr, LuaIndexKey, LuaNameExpr, LuaParser, LuaSyntaxTree,
        LuaTableExpr, ParserConfig, PathTrait,
    };

    fn get_tree(code: &str) -> LuaSyntaxTree {
        let config = ParserConfig::default();

        LuaParser::parse(code, config)
    }

    #[test]
    fn test_call_access_path() {
        let code = "call.ddd()";
        let tree = get_tree(code);
        let root = tree.get_chunk_node();
        let call_expr = root.descendants::<LuaCallExpr>().next().unwrap();
        assert_eq!(call_expr.get_access_path().unwrap(), "call.ddd");
    }

    #[test]
    fn test_call_access_path2() {
        let code = "call[1].aaa.bbb.ccc()";
        let tree = get_tree(code);
        let root = tree.get_chunk_node();
        let call_expr = root.descendants::<LuaCallExpr>().next().unwrap();
        assert_eq!(call_expr.get_access_path().unwrap(), "call.1.aaa.bbb.ccc");
    }

    #[test]
    fn test_name_access_path() {
        let code = "local a = name";
        let tree = get_tree(code);
        let root = tree.get_chunk_node();
        let name_expr = root.descendants::<LuaNameExpr>().next().unwrap();
        assert_eq!(name_expr.get_access_path().unwrap(), "name");
    }

    #[test]
    fn test_index_expr_access_path() {
        let code = "local a = name.bbb.ccc";
        let tree = get_tree(code);
        let root = tree.get_chunk_node();
        let index_expr = root.descendants::<LuaIndexExpr>().next().unwrap();
        assert_eq!(index_expr.get_access_path().unwrap(), "name.bbb.ccc");
    }

    #[test]
    fn test_index_expr_access_path2() {
        let code = "local a = name[okok.yes]";
        let tree = get_tree(code);
        let root = tree.get_chunk_node();
        let index_expr = root.descendants::<LuaIndexExpr>().next().unwrap();
        assert_eq!(index_expr.get_access_path().unwrap(), "name.[okok.yes]");
    }

    #[test]
    fn test_table_expr_fields_with_keys_preserves_sequence_indexes() {
        let code = "local t = { foo = 1, 10, [2] = 20, bar = 3, 30 }";
        let tree = get_tree(code);
        let root = tree.get_chunk_node();
        let table_expr = root.descendants::<LuaTableExpr>().next().unwrap();
        let fields = table_expr.get_fields_with_keys();

        assert_eq!(fields.len(), 5);
        assert!(matches!(fields[0].1, LuaIndexKey::Name(_)));
        assert!(matches!(fields[1].1, LuaIndexKey::Idx(1)));
        assert!(matches!(fields[2].1, LuaIndexKey::Integer(_)));
        assert!(matches!(fields[3].1, LuaIndexKey::Name(_)));
        assert!(matches!(fields[4].1, LuaIndexKey::Idx(2)));
    }
}
