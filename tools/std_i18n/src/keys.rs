use std::collections::HashMap;

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
