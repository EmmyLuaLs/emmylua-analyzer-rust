pub fn check_match_word(key: &str, candidate_key: &str) -> bool {
    if key.is_empty() {
        return true; // 关键词为空, 一般是在空白处主动触发补全
    }
    if candidate_key.is_empty() {
        return false;
    }

    // Get the first character of the key and convert it to lowercase
    let key_first_char = key.chars().next().unwrap().to_lowercase().next().unwrap();

    // Special case: when the search keyword is an underscore
    if key_first_char == '_' && candidate_key.starts_with('_') {
        return true;
    }

    let mut prev_char = '\0'; // Used to track the previous character

    for (i, curr_char) in candidate_key.chars().enumerate() {
        // Determine if the current character is the start of a word
        let is_word_start = (i == 0 && curr_char != '_') ||
            // Character after an underscore
            (prev_char == '_') ||
            // Uppercase letter preceded by a lowercase letter (camel case)
            (curr_char.is_uppercase() && prev_char.is_lowercase()) ||
            // Boundary between ASCII and non-ASCII characters, Chinese and English
            (curr_char.is_ascii_alphabetic() != prev_char.is_ascii_alphabetic() && i > 0);

        // If the current character is the start of a word, check if it matches
        if is_word_start {
            let curr_lowercase = curr_char.to_lowercase().next().unwrap();
            if curr_lowercase == key_first_char {
                return true;
            }
        }

        prev_char = curr_char;
    }

    false // No match found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_keyword_english() {
        assert_eq!(check_match_word("_", "_VERSION"), true);
        assert_eq!(check_match_word("local", "local_aa"), true);
        assert_eq!(check_match_word("i", "if"), true);
        assert_eq!(check_match_word("i", "_if"), true);
        assert_eq!(check_match_word("i", "notIf"), true);
        assert_eq!(check_match_word("i", "this_if"), true);
        assert_eq!(check_match_word("i", "this_not"), false);
        assert_eq!(check_match_word("I", "If"), true);
        assert_eq!(check_match_word("I", "if"), true);
        assert_eq!(check_match_word("i", "IF"), true);
        assert_eq!(check_match_word("n", "not"), true);
        assert_eq!(check_match_word("t", "this"), true);
        assert_eq!(check_match_word("f", "functionName"), true);
        assert_eq!(check_match_word("n", "functionName"), true);
        assert_eq!(check_match_word("g", "_G"), true);
        assert_eq!(check_match_word("u", "___multiple___underscores___"), true);
    }

    #[test]
    fn test_match_keyword_chinese() {
        assert_eq!(check_match_word("如", "_如果"), true);
        assert_eq!(check_match_word("如", "_______如果"), true);
        assert_eq!(check_match_word("_", "_______如果"), true);
        assert_eq!(check_match_word("如", "如果"), true);
        assert_eq!(check_match_word("如", "Not如果"), true);
        assert_eq!(check_match_word("n", "Not如果"), true);
        assert_eq!(check_match_word("如", "This_如果"), true);
        assert_eq!(check_match_word("R", "如果"), false);
        assert_eq!(check_match_word("r", "如果"), false);
        assert_eq!(check_match_word("如", "如果If"), true);
        assert_eq!(check_match_word("果", "水果"), false);
    }

    #[test]
    fn test_match_keyword_mixed() {
        assert_eq!(check_match_word("i", "如果If"), true);
        assert_eq!(check_match_word("r", "Not如果"), false);
        assert_eq!(check_match_word("t", "This_如果"), true);
        assert_eq!(check_match_word("n", "not如果"), true);
        assert_eq!(check_match_word("f", "Function如果"), true);
        assert_eq!(check_match_word("果", "Function如果"), false);
    }

    #[test]
    fn test_match_keyword_empty_input() {
        assert_eq!(check_match_word("", "if"), true);
        assert_eq!(check_match_word("i", ""), false);
        assert_eq!(check_match_word("", ""), true);
    }
}
