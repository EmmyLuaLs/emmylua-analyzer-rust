use emmylua_code_analysis::{EmmyrcFilenameConvention, LuaType, ModuleInfo};

pub fn module_name_convert(
    module_info: &ModuleInfo,
    file_conversion: EmmyrcFilenameConvention,
) -> String {
    let mut module_name = module_info.name.to_string();

    match file_conversion {
        EmmyrcFilenameConvention::SnakeCase => {
            module_name = to_snake_case(&module_name);
        }
        EmmyrcFilenameConvention::CamelCase => {
            module_name = to_camel_case(&module_name);
        }
        EmmyrcFilenameConvention::PascalCase => {
            module_name = to_pascal_case(&module_name);
        }
        EmmyrcFilenameConvention::Keep => {}
        EmmyrcFilenameConvention::KeepClass => {
            if let Some(export_type) = &module_info.export_type
                && let LuaType::Def(id) = export_type
            {
                module_name = id.get_simple_name().to_string();
            }
        }
    }

    module_name
}

pub fn file_name_convert(
    key: &str,
    typ: &LuaType,
    file_conversion: EmmyrcFilenameConvention,
) -> String {
    let mut file_name = key.to_string();
    match file_conversion {
        EmmyrcFilenameConvention::SnakeCase => {
            file_name = to_snake_case(&file_name);
        }
        EmmyrcFilenameConvention::CamelCase => {
            file_name = to_camel_case(&file_name);
        }
        EmmyrcFilenameConvention::PascalCase => {
            file_name = to_pascal_case(&file_name);
        }
        EmmyrcFilenameConvention::Keep => {}
        EmmyrcFilenameConvention::KeepClass => {
            if let LuaType::Def(id) = typ {
                file_name = id.get_simple_name().to_string();
            }
        }
    }
    file_name
}

pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i != 0 {
            result.push('_');
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch.to_ascii_lowercase());
        }
    }
    result
}

pub fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut next_uppercase = false;
    for (i, ch) in s.chars().enumerate() {
        if ch == '_' || ch == '-' || ch == '.' {
            next_uppercase = true;
        } else if next_uppercase {
            result.push(ch.to_ascii_uppercase());
            next_uppercase = false;
        } else if i == 0 {
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

pub fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut next_uppercase = true;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == '.' {
            next_uppercase = true;
        } else if next_uppercase {
            result.push(ch.to_ascii_uppercase());
            next_uppercase = false;
        } else {
            result.push(ch.to_ascii_lowercase());
        }
    }
    result
}
