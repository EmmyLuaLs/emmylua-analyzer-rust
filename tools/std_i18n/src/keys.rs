use crate::extractor::owner_symbol_from_ast;
use emmylua_parser::{LuaAst, LuaAstNode, LuaComment, LuaDocTag, LuaParser, ParserConfig};
use std::collections::HashMap;

/// 从 std 源文件中构建“模块表 -> class 名”的映射。
///
/// 典型例子（io.lua）：
/// - `---@class iolib` + `io = {}` -> `io -> iolib`
pub fn build_module_table_to_class_map(lua_content: &str) -> HashMap<String, String> {
    let tree = LuaParser::parse(lua_content, ParserConfig::default());
    let chunk = tree.get_chunk_node();

    let mut map: HashMap<String, String> = HashMap::new();
    for comment in chunk.descendants::<LuaComment>() {
        let Some(owner_ast) = comment.get_owner() else {
            continue;
        };

        // 仅对“模块表/全局对象”做映射（例如 `io = {}` -> `io -> iolib`）。
        // 对 `local x` 这类局部变量的 `---@class ...` 不做映射：
        // - 局部变量的成员（`function x:set() end`）应以代码标识符为前缀生成 key（`x.set`），
        //   而不是类型名（例如 `ffi.cb*`）。
        if matches!(
            owner_ast,
            LuaAst::LuaLocalStat(_) | LuaAst::LuaLocalFuncStat(_)
        ) {
            continue;
        }

        let Some(owner) = owner_symbol_from_ast(owner_ast) else {
            continue;
        };
        for tag in comment.get_doc_tags() {
            if let LuaDocTag::Class(class_tag) = tag {
                let Some(class_name) = class_tag
                    .get_name_token()
                    .map(|t| t.get_name_text().to_string())
                else {
                    continue;
                };
                map.insert(owner.clone(), class_name);
            }
        }
    }
    map
}

/// 将源码中的符号名映射为“locale key 中使用的符号路径”。
///
/// 规则：
/// - `io.open` -> `iolib.open`（根据 `io -> iolib` 映射）
/// - `io` -> `iolib`（表本身）
/// - `file:close` -> `file.close`
/// - `std.readmode` -> `std.readmode`（符号本身包含 `std.` 时不做特殊处理）
pub fn map_symbol_for_locale_key(symbol: &str, module_map: &HashMap<String, String>) -> String {
    let mut s = symbol.to_string();
    if let Some(class) = module_map.get(symbol) {
        s = class.clone();
    }
    if let Some((first, rest)) = s.split_once('.')
        && let Some(class) = module_map.get(first)
    {
        s = format!("{class}.{rest}");
    }
    s.replace(':', ".")
}

pub fn locale_key_desc(base: &str) -> String {
    base.to_string()
}

pub fn locale_key_param(base: &str, name: &str) -> String {
    format!("{base}.param.{name}")
}

pub fn locale_key_return(base: &str, index: &str) -> String {
    format!("{base}.return.{index}")
}

pub fn locale_key_return_item(base: &str, index: &str, value: &str) -> String {
    format!("{base}.return.{index}.{value}")
}

pub fn locale_key_field(base: &str, name: &str) -> String {
    format!("{base}.field.{name}")
}

pub fn locale_key_item(base: &str, value: &str) -> String {
    format!("{base}.item.{value}")
}
