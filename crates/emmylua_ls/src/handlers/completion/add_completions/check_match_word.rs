pub fn check_match_word(key: &str, candidate_key: &str) -> bool {
    if key.is_empty() || candidate_key.is_empty() {
        return false; // 避免空字符串的情况
    }

    // 获取 key 的首字符并转换为小写
    let key_first_char = key.chars().next().unwrap().to_lowercase().next().unwrap();

    // 特殊情况：当搜索关键字是下划线时
    if key_first_char == '_' && candidate_key.starts_with('_') {
        return true;
    }
    
    let mut prev_char = '\0'; // 用于跟踪上一个字符
    
    for (i, curr_char) in candidate_key.chars().enumerate() {
        // 判断当前字符是否是词的开头
        let is_word_start = 
            // 首字符（除非是下划线）
            (i == 0 && curr_char != '_') ||
            // 下划线后的字符
            (prev_char == '_') ||
            // 大写字母前面是小写字母（驼峰式）
            (curr_char.is_uppercase() && prev_char.is_lowercase()) ||
            // ASCII和非ASCII字符的边界, 中英文
            (curr_char.is_ascii_alphabetic() != prev_char.is_ascii_alphabetic() && i > 0);
            
        // 如果当前字符是词的开头，检查是否匹配
        if is_word_start {
            let curr_lowercase = curr_char.to_lowercase().next().unwrap();
            if curr_lowercase == key_first_char {
                return true;
            }
        }
        
        prev_char = curr_char;
    }

    false // 没有找到匹配
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
        assert_eq!(check_match_word("", "if"), false);
        assert_eq!(check_match_word("i", ""), false);
        assert_eq!(check_match_word("", ""), false);
    }
}
