use super::text_range::SourceRange;
use crate::LuaTokenKind;
use crate::parser::{MarkEvent, MarkerEventContainer};
use std::mem::forget;
use std::ops::{Deref, DerefMut};
use std::str::Chars;

pub const EOF: char = '\0';

/// Reader with look-ahead and look-behind methods.
///
/// As you read text, the part that you've read is accumulated
/// in `current_range`. The part that you haven't seen yet is available
/// in `tail_range`:
///
/// ```text
/// valid range: a b c d e f g
///                ^^^          - current range
///                    ^^^^^^^  - tail range
///                  ^          - prev char
///                    ^        - current char
///                      ^      - next char
/// ```
///
/// Once you call `reset_buff`, current range is advanced to start
/// at the current char, and shrunk to zero length:
///
/// ```text
/// valid range: a b c d e f g
///                    .       - current range (empty, starts at `d`)
///                    ^^^^^^  - tail range
///                  ^         - prev char
///                    ^       - current char
///                      ^     - next char
/// ```
///
/// The workflow in roughly this:
///
/// - you read characters, they're put into `saved_range`;
/// - once you're at a token boundary, you emit a token with `saved_range`,
///   then call `reset_buff`,
/// - you continue onto the next token.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    text: &'a str,
    valid_range: SourceRange,
    chars: Chars<'a>,
    current_buffer_byte_pos: usize,
    current_buffer_byte_len: usize,
    next: char,
    current: char,
    prev: char,
}

impl<'a> Reader<'a> {
    pub fn new(text: &'a str) -> Self {
        Self::new_with_range(text, SourceRange::new(0, text.len()))
    }

    pub fn new_with_range(text: &'a str, range: SourceRange) -> Self {
        assert_eq!(text.len(), range.length);
        let mut res = Self {
            text,
            valid_range: range,
            chars: text.chars(),
            current_buffer_byte_pos: 0,
            current_buffer_byte_len: 0,
            next: EOF,
            current: EOF,
            prev: EOF,
        };

        res.current = res.chars.next().unwrap_or(EOF);
        res.next = res.chars.next().unwrap_or(EOF);

        res
    }

    pub fn bump(&mut self) {
        if self.current != EOF {
            self.current_buffer_byte_len += self.current.len_utf8();
            self.prev = self.current;
            self.current = self.next;
            self.next = self.chars.next().unwrap_or(EOF);
        }
    }

    pub fn reset_buff(&mut self) {
        self.current_buffer_byte_pos += self.current_buffer_byte_len;
        self.current_buffer_byte_len = 0;
    }

    pub fn reset_buff_into_sub_reader(&mut self) -> Reader<'a> {
        let mut reader = Reader::new_with_range(self.current_text(), self.current_range());
        if let Some(prev) = &self.text[..self.current_buffer_byte_pos]
            .chars()
            .next_back()
        {
            reader.prev = *prev;
        }
        self.reset_buff();
        reader
    }

    pub fn is_eof(&self) -> bool {
        self.current == EOF
    }

    pub fn is_start_of_line(&self) -> bool {
        self.current_buffer_byte_pos == 0
    }

    pub fn prev_char(&self) -> char {
        self.prev
    }

    pub fn current_char(&self) -> char {
        self.current
    }

    pub fn next_char(&mut self) -> char {
        self.next
    }

    pub fn current_range(&self) -> SourceRange {
        SourceRange::new(
            self.valid_range.start_offset + self.current_buffer_byte_pos,
            self.current_buffer_byte_len,
        )
    }

    pub fn tail_range(&self) -> SourceRange {
        self.valid_range
            .moved(self.current_buffer_byte_pos + self.current_buffer_byte_len)
    }

    pub fn current_text(&self) -> &'a str {
        &self.text[self.current_buffer_byte_pos
            ..(self.current_buffer_byte_pos + self.current_buffer_byte_len)]
    }

    pub fn tail_text(&self) -> &'a str {
        &self.text[self.current_buffer_byte_pos + self.current_buffer_byte_len..]
    }

    pub fn eat_when(&mut self, ch: char) -> usize {
        let mut count = 0;
        while !self.is_eof() && self.current_char() == ch {
            count += 1;
            self.bump();
        }
        count
    }

    pub fn consume_char_n_times(&mut self, ch: char, count: usize) -> usize {
        let mut eaten = 0;
        while !self.is_eof() && self.current_char() == ch && eaten < count {
            eaten += 1;
            self.bump();
        }
        eaten
    }

    pub fn consume_n_times<F>(&mut self, func: F, count: usize) -> usize
    where
        F: Fn(char) -> bool,
    {
        let mut eaten = 0;
        while !self.is_eof() && func(self.current_char()) && eaten < count {
            eaten += 1;
            self.bump();
        }
        eaten
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

    pub fn get_source_text(&self) -> &'a str {
        self.text
    }

    pub fn get_current_end_pos(&self) -> usize {
        self.current_buffer_byte_pos + self.current_buffer_byte_len
    }
}

impl<'a> Default for Reader<'a> {
    fn default() -> Self {
        Self::new("")
    }
}

/// Reader that can emit token marks and events into the given container.
///
/// Used for backtracking.
#[derive(Debug)]
pub struct ReaderWithMarks<'a> {
    reader: Reader<'a>,
    events: &'a mut Vec<MarkEvent>,
}

#[allow(dead_code)]
impl<'a> ReaderWithMarks<'a> {
    pub fn new(reader: Reader<'a>, events: &'a mut Vec<MarkEvent>) -> Self {
        Self { reader, events }
    }

    pub fn bump(&mut self) {
        self.reader.bump()
    }

    pub fn reset_buff(&mut self) {
        self.reader.reset_buff()
    }

    pub fn reset_buff_into_sub_reader(&mut self) -> Reader<'a> {
        self.reader.reset_buff_into_sub_reader()
    }

    pub fn emit(&mut self, kind: LuaTokenKind) {
        let range = self.current_range();
        if !range.is_empty() {
            self.get_events().push(MarkEvent::EatToken { kind, range });
        }
        self.reset_buff();
    }

    pub fn is_eof(&self) -> bool {
        self.reader.is_eof()
    }

    pub fn is_start_of_line(&self) -> bool {
        self.reader.is_start_of_line()
    }

    pub fn prev_char(&self) -> char {
        self.reader.prev_char()
    }

    pub fn current_char(&self) -> char {
        self.reader.current_char()
    }

    pub fn next_char(&mut self) -> char {
        self.reader.next_char()
    }

    pub fn current_range(&self) -> SourceRange {
        self.reader.current_range()
    }

    pub fn tail_range(&self) -> SourceRange {
        self.reader.tail_range()
    }

    pub fn current_text(&self) -> &'a str {
        self.reader.current_text()
    }

    pub fn tail_text(&self) -> &'a str {
        self.reader.tail_text()
    }

    pub fn eat_when(&mut self, ch: char) -> usize {
        self.reader.eat_when(ch)
    }

    pub fn consume_char_n_times(&mut self, ch: char, count: usize) -> usize {
        self.reader.consume_char_n_times(ch, count)
    }

    pub fn consume_n_times<F>(&mut self, func: F, count: usize) -> usize
    where
        F: Fn(char) -> bool,
    {
        self.reader.consume_n_times(func, count)
    }

    pub fn eat_while<F>(&mut self, func: F) -> usize
    where
        F: Fn(char) -> bool,
    {
        self.reader.eat_while(func)
    }

    pub fn get_source_text(&self) -> &'a str {
        self.reader.get_source_text()
    }

    pub fn get_current_end_pos(&self) -> usize {
        self.reader.get_current_end_pos()
    }

    pub fn backtrack_point<'s>(&'s mut self) -> ReaderBacktrackPoint<'s, 'a> {
        ReaderBacktrackPoint::new(self)
    }
}

impl<'a> MarkerEventContainer for ReaderWithMarks<'a> {
    fn get_mark_level(&self) -> usize {
        0
    }

    fn incr_mark_level(&mut self) {
        // Nothing
    }

    fn decr_mark_level(&mut self) {
        // Nothing
    }

    fn get_events(&mut self) -> &mut Vec<MarkEvent> {
        &mut self.events
    }
}

/// A backtrack point.
///
/// Used to easily backtrack any events or tokens that were read/emitted
/// via [`ReaderWithMarks`].
#[must_use = "backtrack points use RAII and should be saved on the stack"]
pub struct ReaderBacktrackPoint<'a, 'b> {
    // Note: we rely on the fact that `Reader` is trivially destructible,
    // and forgetting `ReaderBacktrackPoint` without running its destructor
    // will not leak memory. This saves us from storing an additional flag
    // that tracks whether this point was committed or not. See `Drop` impl
    // for more info.
    reader: &'a mut ReaderWithMarks<'b>,
    prev_state: Reader<'b>,
    events_pos: usize,
}

impl<'a, 'b> ReaderBacktrackPoint<'a, 'b> {
    pub fn new(reader: &'a mut ReaderWithMarks<'b>) -> Self {
        // Note: even though `Reader` doesn't implement `copy`, it's still
        // very cheap to clone as it doesn't own any memory.
        let prev_state = reader.reader.clone();
        let mark_pos = reader.events.len();
        Self {
            reader,
            prev_state,
            events_pos: mark_pos,
        }
    }

    pub fn commit(self) {
        debug_assert!(
            !std::mem::needs_drop::<Reader>(),
            "reader should be trivially destructible, \
            otherwise this code may leak memory"
        );
        forget(self); // Don't run drop glue to avoid rollback.
    }

    pub fn rollback(self) {
        drop(self); // Rollback happens automatically on drop.
    }
}

impl<'a, 'b> Drop for ReaderBacktrackPoint<'a, 'b> {
    fn drop(&mut self) {
        self.reader.events.truncate(self.events_pos);
        self.reader.reader = self.prev_state.clone();
    }
}

impl<'a, 'b> Deref for ReaderBacktrackPoint<'a, 'b> {
    type Target = ReaderWithMarks<'b>;

    fn deref(&self) -> &Self::Target {
        self.reader
    }
}

impl<'a, 'b> DerefMut for ReaderBacktrackPoint<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.reader
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
        assert!(!reader.is_start_of_line());
        assert!(!reader.is_eof());
    }

    #[test]
    fn test_is_eof() {
        let text = "H";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        assert!(!reader.is_eof());
        reader.bump();
        assert!(reader.is_eof());
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
        let range = reader.current_range();
        assert_eq!(range.start_offset, 0);
        assert_eq!(range.length, 1);

        reader.reset_buff();
        reader.bump();
        let range2 = reader.current_range();
        assert_eq!(range2.start_offset, 1);
        assert_eq!(range2.length, 1);
    }

    #[test]
    fn test_current_saved_text() {
        let text = "Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        reader.bump();
        assert_eq!(reader.current_text(), "H");
    }

    #[test]
    fn test_eat_when() {
        let text = "aaaHello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        let count = reader.eat_when('a');
        assert_eq!(count, 3);
        assert_eq!(reader.current_char(), 'H');
        assert_eq!(reader.current_text(), "aaa");
    }

    #[test]
    fn test_eat_while() {
        let text = "12345Hello, world!";
        let mut reader = Reader::new(text);
        reader.reset_buff();
        let count = reader.eat_while(|c| c.is_ascii_digit());
        assert_eq!(count, 5);
        assert_eq!(reader.current_char(), 'H');
    }

    #[test]
    fn test_backtrack_commit() {
        let text = "Hello, world!";
        let mut events = Vec::new();
        let mut reader = ReaderWithMarks::new(Reader::new(text), &mut events);

        reader.eat_while(|c| c != ' ');
        assert_eq!(reader.current_text(), "Hello,");
        {
            let mut reader = reader.backtrack_point();
            reader.eat_while(|_| true);
            assert_eq!(reader.current_text(), "Hello, world!");
            reader.commit();
        }
        assert_eq!(reader.current_text(), "Hello, world!");
    }

    #[test]
    fn test_backtrack_rollback() {
        let text = "Hello, world!";
        let mut events = Vec::new();
        let mut reader = ReaderWithMarks::new(Reader::new(text), &mut events);

        reader.eat_while(|c| c != ' ');
        assert_eq!(reader.current_text(), "Hello,");
        {
            let mut reader = reader.backtrack_point();
            reader.eat_while(|_| true);
            assert_eq!(reader.current_text(), "Hello, world!");
            reader.rollback();
        }
        assert_eq!(reader.current_text(), "Hello,");
    }

    #[test]
    fn test_backtrack_auto_rollback() {
        let text = "Hello, world!";
        let mut events = Vec::new();
        let mut reader = ReaderWithMarks::new(Reader::new(text), &mut events);

        reader.eat_while(|c| c != ' ');
        assert_eq!(reader.current_text(), "Hello,");
        {
            let mut reader = reader.backtrack_point();
            reader.eat_while(|_| true);
            assert_eq!(reader.current_text(), "Hello, world!");
        }
        assert_eq!(reader.current_text(), "Hello,");
    }
}
