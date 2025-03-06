pub fn meta_keyword(key: &str) -> String {
    t!(format!("keywords.{}", key)).to_string()
}

#[allow(unused)]
pub fn meta_builtin_std(key: &str) -> String {
    t!(format!("builtin_std.{}", key)).to_string()
}

pub fn meta_std(type_name: &str, member_name: Option<&str>) -> String {
    let key = if let Some(member_name) = member_name {
        format!("std.{}.{}", type_name, member_name)
    } else {
        format!("std.{}", type_name)
    };
    let s = t!(key).to_string();
    if s == key { // 临时处理, 因为我们还未完成所有翻译
        "".to_string()
    } else {
        s
    }
}

pub fn meta_doc_tag(key: &str) -> String {
    t!(format!("tags.{}", key)).to_string()
}
