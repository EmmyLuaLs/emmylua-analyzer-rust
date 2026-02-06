use url::Url;

use crate::LuaTypeDeclId;

#[derive(Debug, Clone)]
pub enum JsonSchemaFile {
    NeedResolve,
    BadUrl,
    Resolved(LuaTypeDeclId),
}

pub fn get_schema_short_name(url: &Url) -> String {
    const MAX_LEN: usize = 128;

    let url_str = url.as_str().to_string();
    let mut new_name = String::new();
    for c in url_str.chars().rev() {
        if c == '/' || c == '#' || c == '?' || c == '&' || c == '=' {
            new_name.push('_');
        } else {
            new_name.push(c);
        }
        if new_name.len() >= MAX_LEN {
            break;
        }
    }

    new_name.chars().rev().collect()
}
