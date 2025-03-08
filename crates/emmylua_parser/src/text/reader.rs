use std::{iter::Peekable, str::Chars};

use super::text_range::SourceRange;
pub const EOF: char = '\0';

#[derive(Debug, Clone)]
pub struct Reader<'a> {
    text: &'a str,
    valid_range: SourceRange,
    chars: Peekable<Chars<'a>>,
    save_buffer_byte_pos: usize,
    save_buffer_byte_len: usize,
    current_char_len: usize,
    current: char,
    start: bool,
}

impl<'a> Reader<'a> {
    pub fn new(text: &'a str) -> Self {
        Self::new_with_range(text, SourceRange::new(0, text.len()))
    }

    pub fn new_with_range(text: &'a str, range: SourceRange) -> Self {
        let text = text[range.start_offset..range.end_offset()].as_ref();
        Self {
            text,
            valid_range: range,
            chars: text.chars().peekable(),
            save_buffer_byte_pos: 0,
            save_buffer_byte_len: 0,
            current_char_len: 0,
            current: EOF,
            start: false,
        }
    }

    pub fn bump(&mut self) {
        if let Some(c) = self.chars.next() {
            self.current = c;
            self.save_buffer_byte_len += self.current_char_len;
            self.current_char_len = c.len_utf8();
        } else {
            self.current = EOF;
            if self.current_char_len > 0 {
                self.save_buffer_byte_len += self.current_char_len;
                self.current_char_len = 0;
            }
        }
    }

    pub fn reset_buff(&mut self) {
        self.save_buffer_byte_pos += self.save_buffer_byte_len;
        self.save_buffer_byte_len = 0;
        if !self.start {
            self.start = true;
            self.bump();
        }
    }

    pub fn is_eof(&self) -> bool {
        self.current == EOF && self.start
    }

    pub fn is_start_of_line(&self) -> bool {
        self.save_buffer_byte_pos == 0
    }

    pub fn current_char(&self) -> char {
        self.current
    }

    pub fn next_char(&mut self) -> char {
        self.chars.peek().cloned().unwrap_or(EOF)
    }

    pub fn saved_range(&self) -> SourceRange {
        SourceRange::new(
            self.valid_range.start_offset + self.save_buffer_byte_pos,
            self.save_buffer_byte_len,
        )
    }

    pub fn current_saved_text(&self) -> &str {
        &self.text
            [self.save_buffer_byte_pos..(self.save_buffer_byte_pos + self.save_buffer_byte_len)]
    }

    pub fn eat_when(&mut self, ch: char) -> usize {
        let mut count = 0;
        while !self.is_eof() && self.current_char() == ch {
            count += 1;
            self.bump();
        }
        count
    }

    pub fn eat_while<F>(&mut self, func: F) -> usize
    where
        F: Fn(char) -> bool,
    {
        let mut count = 0;
        while !self.is_eof() && func(self.current_char()) {
            count += 1;
            self.bump();
        }
        count
    }

    pub fn get_source_text(&self) -> &str {
        self.text
    }

    pub fn get_current_end_pos(&self) -> usize {
        self.save_buffer_byte_pos + self.save_buffer_byte_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_reader() {
        let text = "Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        assert_eq!(reader.current_char(), 'H');
    }

    #[test]
    fn test_bump() {
        let text = "Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        reader.bump();
        assert_eq!(reader.current_char(), 'e');
    }

    #[test]
    fn test_reset_buff() {
        let text = "Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        reader.bump();
        reader.reset_buff();
        assert_eq!(reader.current_char(), 'e');
        assert_eq!(reader.is_start_of_line(), false);
        assert_eq!(reader.is_eof(), false);
    }

    #[test]
    fn test_is_eof() {
        let text = "H";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        assert_eq!(reader.is_eof(), false);
        reader.bump();
        assert_eq!(reader.is_eof(), true);
    }

    #[test]
    fn test_next_char() {
        let text = "Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        assert_eq!(reader.next_char(), 'e');
    }

    #[test]
    fn test_saved_range() {
        let text = "Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        reader.bump();
        let range = reader.saved_range();
        assert_eq!(range.start_offset, 0);
        assert_eq!(range.length, 1);

        reader.reset_buff();
        reader.bump();
        let range2 = reader.saved_range();
        assert_eq!(range2.start_offset, 1);
        assert_eq!(range2.length, 1);
    }

    #[test]
    fn test_current_saved_text() {
        let text = "Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        reader.bump();
        assert_eq!(reader.current_saved_text(), "H");
    }

    #[test]
    fn test_eat_when() {
        let text = "aaaHello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        let count = reader.eat_when('a');
        assert_eq!(count, 3);
        assert_eq!(reader.current_char(), 'H');
        assert_eq!(reader.current_saved_text(), "aaa");
    }

    #[test]
    fn test_eat_while() {
        let text = "12345Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        let count = reader.eat_while(|c| c.is_digit(10));
        assert_eq!(count, 5);
        assert_eq!(reader.current_char(), 'H');
    }
}
