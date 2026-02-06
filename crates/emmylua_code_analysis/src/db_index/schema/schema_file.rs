use url::Url;

use crate::LuaTypeDeclId;

#[derive(Debug, Clone)]
pub enum JsonSchemaFile {
    NeedResolve,
    BadUrl,
    Resolved(LuaTypeDeclId),
}

pub fn get_schema_short_name(url: &Url) -> String {
    const MAX_LEN: usize = 64;

    let url_str = url.as_str();
    let mut new_name = String::new();
    for c in url_str.chars().rev() {
        if new_name.len() >= MAX_LEN {
            break;
        }

        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
            new_name.push(c);
        } else if !c.is_control() && c != ' ' {
            new_name.push('_');
        }
    }

    let mut result: String = new_name.chars().rev().collect();

    result = result.trim_matches(|c| c == '_' || c == '.').to_string();

    if result.is_empty() {
        return "schema".to_string();
    }

    result
}
