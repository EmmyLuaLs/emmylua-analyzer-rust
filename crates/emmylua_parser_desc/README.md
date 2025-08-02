## EmmyLua-Parser-Desc

EmmyLua-Parser-Desc is an extension for EmmyLua-Parser that uses its internal machinery to provide lexic information about markup of documentation comments. It supports parsing Markdown, MyST and RST.


### Features

- Ability to parse description blocks provided by EmmyLua-Parser and report ranges of interest: highlighted keywords,  code blocks, cross-references and so on
- Supports Markdown, MyST and RST
- Ability to parse possible broken or unterminated MyST and RST cross-references in order to facilitate Autocompletion and Go To Definition functionality

### Usage

```rust
let code = r#"
    --- Description in **markdown format**, with example code:
    ---
    --- ```lua
    --- print(a)
    --- ```
    local a = 1
"#;
let tree = LuaParser::parse(code, ParserConfig::default());

let chunk = tree.get_chunk_node();
for desc in chunk.descendants::<LuaDocDescription>() {
    let doc_items = emmylua_parser_desc::parse(
        DescParserType::Md,
        code,
        desc,
        None
    );
    println!("{:?}", doc_items);
}
```
