use crate::format::TokenExpected;

#[derive(Debug)]
pub struct FormatterContext {
    pub current_expected: Option<TokenExpected>,
    pub is_line_first_token: bool,
    pub text: String,
}

impl FormatterContext {
    pub fn new() -> Self {
        Self {
            current_expected: None,
            is_line_first_token: true,
            text: String::new(),
        }
    }

    pub fn reset_whitespace(&mut self) {
        while self.text.ends_with(' ') {
            self.text.pop();
        }
    }

    pub fn get_last_whitespace_count(&self) -> usize {
        let mut count = 0;
        for ch in self.text.chars().rev() {
            if ch == ' ' {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    pub fn reset_whitespace_to(&mut self, n: usize) {
        self.reset_whitespace();
        if n > 0 {
            self.text.push_str(&" ".repeat(n));
        }
    }
}
