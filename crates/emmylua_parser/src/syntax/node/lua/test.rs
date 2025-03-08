#[cfg(test)]
mod tests {
    use crate::{
        LuaAstNode, LuaCallExpr, LuaIndexExpr, LuaNameExpr, LuaParser, LuaSyntaxTree, ParserConfig,
        PathTrait,
    };

    fn get_tree(code: &str) -> LuaSyntaxTree {
        let config = ParserConfig::default();
        let tree = LuaParser::parse(code, config);
        tree
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
}
