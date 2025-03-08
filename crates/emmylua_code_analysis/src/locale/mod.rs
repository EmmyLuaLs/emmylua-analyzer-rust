pub fn get_locale_code(locale: &str) -> String {
    let mut locale = locale.to_string();
    // If the passed `locale` contains '-', convert '-' to '_' and convert the following letters to uppercase
    if locale.contains("-") {
        let parts = locale.split("-").collect::<Vec<&str>>();
        if parts.len() == 2 {
            locale = format!("{}_{}", parts[0], parts[1].to_uppercase());
        }
    }
    match locale.as_str() {
        "zh_TW" => "zh_HK".to_string(),
        "en_US" => "en".to_string(),
        "en_GB" => "en".to_string(),
        _ => locale,
    }
}
