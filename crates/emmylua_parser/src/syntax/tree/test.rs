#[cfg(test)]
mod test {
    use crate::{
        LuaAstNode, LuaLanguageLevel, LuaNonStdSymbolSet, LuaParser, ParserConfig, set_locale,
    };
    // use std::time::Instant;
    use std::{collections::HashMap, thread};

    #[test]
    fn test_multithreaded_syntax_tree_traversal() {
        let code = r#"
            local a = 1
            local b = 2
            print(a + b)
        "#;
        let tree = LuaParser::parse(code, ParserConfig::default());
        let tree_arc = std::sync::Arc::new(tree);

        let mut handles = vec![];

        for i in 0..4 {
            let tree_ref = tree_arc.clone();
            let handle = thread::spawn(move || {
                let node = tree_ref.get_chunk_node();
                println!("{:?} {}", node.dump(), i);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_lua51() {
        let code = r#"
if a ~= b then
end
        "#;
        let parse_config = ParserConfig::new(
            LuaLanguageLevel::Lua51,
            None,
            HashMap::new(),
            LuaNonStdSymbolSet::new(),
            false,
        );
        let tree = LuaParser::parse(code, parse_config);
        assert_eq!(tree.get_errors().len(), 0);
    }

    #[test]
    fn test_tree_struct() {
        let code = r#"
function f()
    -- hh
    local t
end
        "#;
        let tree = LuaParser::parse(code, ParserConfig::default());
        let chunk = tree.get_chunk_node();
        println!("{:?}", chunk.dump());
    }

    #[test]
    fn test_error() {
        let code = r#"
local
"#;
        set_locale("zh_CN");
        let tree = LuaParser::parse(code, ParserConfig::default());
        let errors = tree.get_errors();
        for error in errors {
            println!("{:?}", error);
        }
    }

    #[test]
    fn test_bad_syntax() {
        let code = r#"
JsonData.this[] = nil

---@param key string
---@return boolean
local t
        "#;

        let _ = LuaParser::parse(code, ParserConfig::default());
    }

    #[test]
    fn test_without_emmylua() {
        let code = r#"
        ---@param key string
        ---@return boolean
        local t
        "#;

        let c = ParserConfig::new(
            LuaLanguageLevel::Lua54,
            None,
            HashMap::new(),
            LuaNonStdSymbolSet::new(),
            false,
        );
        let t = LuaParser::parse(code, c);
        println!("{:#?}", t.get_red_root());
    }
}
