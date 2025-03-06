use emmylua_code_analysis::{DbIndex, FileId};

use crate::meta_text::meta_std;

pub fn is_std_by_name(name: &str) -> bool {
    match name {
        "oslib" => true,
        "std.osdate" => true,
        "std.osdateparam" => true,
        "mathlib" => true,
        "coroutinelib" => true,
        "debuglib" => true,
        "iolib" => true,
        "file" => true,
        "packagelib" => true,
        "string" => true,
        "tablelib" => true,
        "utf8lib" => true,
        "bit32lib" => true,
        // 全局函数/变量
        "assert" => true,
        "collectgarbage" => true,
        "std.collectgarbage_opt" => true,
        "std.loadmode" => true,
        "dofile" => true,
        "error" => true,
        "_G" => true,
        "getmetatable" => true,
        "ipairs" => true,
        "load" => true,
        "loadstring" => true,
        "loadfile" => true,
        "newproxy" => true,
        "module" => true,
        "next" => true,
        "pairs" => true,
        "pcall" => true,
        "print" => true,
        "rawequal" => true,
        "rawget" => true,
        "rawlen" => true,
        "rawset" => true,
        "require" => true,
        "select" => true,
        "setfenv" => true,
        "setmetatable" => true,
        "tonumber" => true,
        "tostring" => true,
        "type" => true,
        "_VERSION" => true,
        "xpcall" => true,
        _ => false,
    }
}

pub fn hover_std_description(type_name: &str, member_name: Option<&str>) -> String {
    meta_std(type_name, member_name)
}

pub fn is_std_by_path(db: &DbIndex, file_id: FileId) -> Option<bool> {
    let module_info = db.get_module_index().get_module(file_id)?;
    Some(module_info.workspace_id.is_std())
}
