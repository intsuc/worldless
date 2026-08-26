use std::rc::Rc;

use crate::exceptions::{BUILT_IN_EXCEPTIONS, BuiltInExceptionProvider, CommandSyntaxException};

const SYNTAX_ESCAPE: u16 = b'\\' as u16;
const SYNTAX_DOUBLE_QUOTE: u16 = b'"' as u16;
const SYNTAX_SINGLE_QUOTE: u16 = b'\'' as u16;

pub trait ImmutableStringReader {
    fn string(&self) -> &str;
    fn utf16(&self) -> &[u16];
    fn remaining_length(&self) -> i32;
    fn total_length(&self) -> usize;
    fn cursor(&self) -> usize;
    fn read_so_far(&self) -> String;
    fn read_so_far_utf16(&self) -> &[u16];
    fn remaining(&self) -> String;
    fn remaining_utf16(&self) -> &[u16];
    fn can_read_n(&self, length: usize) -> bool;
    fn can_read(&self) -> bool;
    fn peek(&self) -> u16;
    fn peek_offset(&self, offset: usize) -> u16;
}

#[derive(Clone, Debug)]
pub struct StringReader {
    string: Rc<str>,
    units: Rc<[u16]>,
    cursor: usize,
}

impl ImmutableStringReader for StringReader {
    fn string(&self) -> &str {
        StringReader::string(self)
    }

    fn utf16(&self) -> &[u16] {
        StringReader::utf16(self)
    }

    fn remaining_length(&self) -> i32 {
        StringReader::remaining_length(self)
    }

    fn total_length(&self) -> usize {
        StringReader::total_length(self)
    }

    fn cursor(&self) -> usize {
        StringReader::cursor(self)
    }

    fn read_so_far(&self) -> String {
        StringReader::read_so_far(self)
    }

    fn read_so_far_utf16(&self) -> &[u16] {
        StringReader::read_so_far_utf16(self)
    }

    fn remaining(&self) -> String {
        StringReader::remaining(self)
    }

    fn remaining_utf16(&self) -> &[u16] {
        StringReader::remaining_utf16(self)
    }

    fn can_read_n(&self, length: usize) -> bool {
        StringReader::can_read_n(self, length)
    }

    fn can_read(&self) -> bool {
        StringReader::can_read(self)
    }

    fn peek(&self) -> u16 {
        StringReader::peek(self)
    }

    fn peek_offset(&self, offset: usize) -> u16 {
        StringReader::peek_offset(self, offset)
    }
}

impl StringReader {
    pub fn new(string: impl Into<String>) -> Self {
        let string = string.into();
        let units = Rc::<[u16]>::from(string.encode_utf16().collect::<Vec<_>>());
        Self {
            string: Rc::from(string),
            units,
            cursor: 0,
        }
    }

    pub fn from_utf16(units: impl Into<Vec<u16>>) -> Self {
        let units = units.into();
        Self {
            string: Rc::from(String::from_utf16_lossy(&units)),
            units: Rc::from(units),
            cursor: 0,
        }
    }

    pub fn string(&self) -> &str {
        &self.string
    }

    pub fn utf16(&self) -> &[u16] {
        &self.units
    }

    pub fn substring_utf16(&self, start: usize, end: usize) -> Vec<u16> {
        self.units[start..end].to_vec()
    }

    pub fn substring(&self, start: usize, end: usize) -> String {
        String::from_utf16_lossy(&self.units[start..end])
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;
    }

    pub fn remaining_length(&self) -> i32 {
        (self.units.len() as i32).wrapping_sub(self.cursor as i32)
    }

    pub fn total_length(&self) -> usize {
        self.units.len()
    }

    pub fn read_so_far(&self) -> String {
        String::from_utf16_lossy(self.read_so_far_utf16())
    }

    pub fn read_so_far_utf16(&self) -> &[u16] {
        &self.units[..self.cursor]
    }

    pub fn remaining(&self) -> String {
        String::from_utf16_lossy(self.remaining_utf16())
    }

    pub fn remaining_utf16(&self) -> &[u16] {
        &self.units[self.cursor..]
    }

    pub fn can_read_n(&self, length: usize) -> bool {
        self.cursor
            .checked_add(length)
            .is_some_and(|end| end <= self.units.len())
    }

    pub fn can_read(&self) -> bool {
        self.can_read_n(1)
    }

    pub fn peek(&self) -> u16 {
        self.peek_offset(0)
    }

    pub fn peek_offset(&self, offset: usize) -> u16 {
        self.units[self.cursor + offset]
    }

    pub fn read(&mut self) -> u16 {
        let result = self.peek();
        self.cursor += 1;
        result
    }

    pub fn skip(&mut self) {
        self.cursor += 1;
    }

    pub const fn is_allowed_number(character: u16) -> bool {
        matches!(character, 0x30..=0x39 | 0x2e | 0x2d)
    }

    pub const fn is_quoted_string_start(character: u16) -> bool {
        character == SYNTAX_DOUBLE_QUOTE || character == SYNTAX_SINGLE_QUOTE
    }

    pub fn skip_whitespace(&mut self) {
        while self.can_read() && is_java_whitespace(self.peek()) {
            self.skip();
        }
    }

    pub fn read_int(&mut self) -> Result<i32, CommandSyntaxException> {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = self.slice(start, self.cursor);
        if number.is_empty() {
            return Err(BUILT_IN_EXCEPTIONS
                .reader_expected_int()
                .create_with_context(self));
        }
        number.parse::<i32>().map_err(|_| {
            self.cursor = start;
            BUILT_IN_EXCEPTIONS
                .reader_invalid_int()
                .create_with_context(self, number)
        })
    }

    pub fn read_long(&mut self) -> Result<i64, CommandSyntaxException> {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = self.slice(start, self.cursor);
        if number.is_empty() {
            return Err(BUILT_IN_EXCEPTIONS
                .reader_expected_long()
                .create_with_context(self));
        }
        number.parse::<i64>().map_err(|_| {
            self.cursor = start;
            BUILT_IN_EXCEPTIONS
                .reader_invalid_long()
                .create_with_context(self, number)
        })
    }

    pub fn read_double(&mut self) -> Result<f64, CommandSyntaxException> {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = self.slice(start, self.cursor);
        if number.is_empty() {
            return Err(BUILT_IN_EXCEPTIONS
                .reader_expected_double()
                .create_with_context(self));
        }
        number.parse::<f64>().map_err(|_| {
            self.cursor = start;
            BUILT_IN_EXCEPTIONS
                .reader_invalid_double()
                .create_with_context(self, number)
        })
    }

    pub fn read_float(&mut self) -> Result<f32, CommandSyntaxException> {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = self.slice(start, self.cursor);
        if number.is_empty() {
            return Err(BUILT_IN_EXCEPTIONS
                .reader_expected_float()
                .create_with_context(self));
        }
        number.parse::<f32>().map_err(|_| {
            self.cursor = start;
            BUILT_IN_EXCEPTIONS
                .reader_invalid_float()
                .create_with_context(self, number)
        })
    }

    pub const fn is_allowed_in_unquoted_string(character: u16) -> bool {
        matches!(
            character,
            0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a | 0x5f | 0x2d | 0x2e | 0x2b
        )
    }

    pub fn read_unquoted_string(&mut self) -> String {
        String::from_utf16_lossy(&self.read_unquoted_string_utf16())
    }

    pub fn read_unquoted_string_utf16(&mut self) -> Vec<u16> {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_in_unquoted_string(self.peek()) {
            self.skip();
        }
        self.substring_utf16(start, self.cursor)
    }

    pub fn read_quoted_string(&mut self) -> Result<String, CommandSyntaxException> {
        self.read_quoted_string_utf16()
            .map(|result| String::from_utf16_lossy(&result))
    }

    pub fn read_quoted_string_utf16(&mut self) -> Result<Vec<u16>, CommandSyntaxException> {
        if !self.can_read() {
            return Ok(Vec::new());
        }
        let next = self.peek();
        if !Self::is_quoted_string_start(next) {
            return Err(BUILT_IN_EXCEPTIONS
                .reader_expected_start_of_quote()
                .create_with_context(self));
        }
        self.skip();
        self.read_string_until_utf16(next)
    }

    pub fn read_string_until(&mut self, terminator: u16) -> Result<String, CommandSyntaxException> {
        self.read_string_until_utf16(terminator)
            .map(|result| String::from_utf16_lossy(&result))
    }

    pub fn read_string_until_utf16(
        &mut self,
        terminator: u16,
    ) -> Result<Vec<u16>, CommandSyntaxException> {
        let mut result = Vec::new();
        let mut escaped = false;
        while self.can_read() {
            let character = self.read();
            if escaped {
                if character == terminator || character == SYNTAX_ESCAPE {
                    result.push(character);
                    escaped = false;
                } else {
                    self.cursor -= 1;
                    return Err(BUILT_IN_EXCEPTIONS
                        .reader_invalid_escape()
                        .create_with_context(self, utf16_character(character)));
                }
            } else if character == SYNTAX_ESCAPE {
                escaped = true;
            } else if character == terminator {
                return Ok(result);
            } else {
                result.push(character);
            }
        }
        Err(BUILT_IN_EXCEPTIONS
            .reader_expected_end_of_quote()
            .create_with_context(self))
    }

    pub fn read_string(&mut self) -> Result<String, CommandSyntaxException> {
        self.read_string_utf16()
            .map(|result| String::from_utf16_lossy(&result))
    }

    pub fn read_string_utf16(&mut self) -> Result<Vec<u16>, CommandSyntaxException> {
        if !self.can_read() {
            return Ok(Vec::new());
        }
        let next = self.peek();
        if Self::is_quoted_string_start(next) {
            self.skip();
            self.read_string_until_utf16(next)
        } else {
            Ok(self.read_unquoted_string_utf16())
        }
    }

    pub fn read_boolean(&mut self) -> Result<bool, CommandSyntaxException> {
        let start = self.cursor;
        let value = self.read_string()?;
        if value.is_empty() {
            return Err(BUILT_IN_EXCEPTIONS
                .reader_expected_bool()
                .create_with_context(self));
        }
        match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => {
                self.cursor = start;
                Err(BUILT_IN_EXCEPTIONS
                    .reader_invalid_bool()
                    .create_with_context(self, value))
            }
        }
    }

    pub fn expect(&mut self, expected: u16) -> Result<(), CommandSyntaxException> {
        if !self.can_read() || self.peek() != expected {
            return Err(BUILT_IN_EXCEPTIONS
                .reader_expected_symbol()
                .create_with_context(self, utf16_character(expected)));
        }
        self.skip();
        Ok(())
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.substring(start, end)
    }
}

fn utf16_character(character: u16) -> String {
    String::from_utf16_lossy(&[character])
}

const fn is_java_whitespace(character: u16) -> bool {
    matches!(
        character,
        0x0009..=0x000d
            | 0x001c..=0x0020
            | 0x1680
            | 0x2000..=0x2006
            | 0x2008..=0x200a
            | 0x2028..=0x2029
            | 0x205f
            | 0x3000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_error {
        ($result:expr, $exception_type:expr, $cursor:expr) => {{
            let error = $result.expect_err("parse must fail");
            assert!(error.is_type(&$exception_type));
            assert_eq!(error.cursor(), $cursor);
        }};
    }

    #[test]
    fn can_read() {
        let mut reader = StringReader::new("abc");
        assert!(reader.can_read());
        reader.skip();
        assert!(reader.can_read());
        reader.skip();
        assert!(reader.can_read());
        reader.skip();
        assert!(!reader.can_read());
    }

    #[test]
    fn get_remaining_length() {
        let mut reader = StringReader::new("abc");
        assert_eq!(reader.remaining_length(), 3);
        reader.set_cursor(1);
        assert_eq!(reader.remaining_length(), 2);
        reader.set_cursor(2);
        assert_eq!(reader.remaining_length(), 1);
        reader.set_cursor(3);
        assert_eq!(reader.remaining_length(), 0);
    }

    #[test]
    fn cursor_and_remaining_length_follow_java_signed_arithmetic() {
        let mut reader = StringReader::new("");
        reader.skip();
        assert_eq!(reader.cursor(), 1);
        assert_eq!(reader.remaining_length(), -1);

        reader.set_cursor(3);
        assert_eq!(reader.cursor(), 3);
        assert_eq!(reader.remaining_length(), -3);
    }

    #[test]
    fn can_read_length() {
        let reader = StringReader::new("abc");
        assert!(reader.can_read_n(1));
        assert!(reader.can_read_n(2));
        assert!(reader.can_read_n(3));
        assert!(!reader.can_read_n(4));
        assert!(!reader.can_read_n(5));
    }

    #[test]
    fn peek() {
        let mut reader = StringReader::new("abc");
        assert_eq!(reader.peek(), b'a' as u16);
        assert_eq!(reader.cursor(), 0);
        reader.set_cursor(2);
        assert_eq!(reader.peek(), b'c' as u16);
        assert_eq!(reader.cursor(), 2);
    }

    #[test]
    fn peek_length() {
        let mut reader = StringReader::new("abc");
        assert_eq!(reader.peek_offset(0), b'a' as u16);
        assert_eq!(reader.peek_offset(2), b'c' as u16);
        assert_eq!(reader.cursor(), 0);
        reader.set_cursor(1);
        assert_eq!(reader.peek_offset(1), b'c' as u16);
        assert_eq!(reader.cursor(), 1);
    }

    #[test]
    fn read() {
        let mut reader = StringReader::new("abc");
        assert_eq!(reader.read(), b'a' as u16);
        assert_eq!(reader.read(), b'b' as u16);
        assert_eq!(reader.read(), b'c' as u16);
        assert_eq!(reader.cursor(), 3);
    }

    #[test]
    fn skip() {
        let mut reader = StringReader::new("abc");
        reader.skip();
        assert_eq!(reader.cursor(), 1);
    }

    #[test]
    fn get_remaining() {
        let mut reader = StringReader::new("Hello!");
        assert_eq!(reader.remaining(), "Hello!");
        reader.set_cursor(3);
        assert_eq!(reader.remaining(), "lo!");
        reader.set_cursor(6);
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn get_read() {
        let mut reader = StringReader::new("Hello!");
        assert_eq!(reader.read_so_far(), "");
        reader.set_cursor(3);
        assert_eq!(reader.read_so_far(), "Hel");
        reader.set_cursor(6);
        assert_eq!(reader.read_so_far(), "Hello!");
    }

    #[test]
    fn skip_whitespace_none() {
        let mut reader = StringReader::new("Hello!");
        reader.skip_whitespace();
        assert_eq!(reader.cursor(), 0);
    }

    #[test]
    fn skip_whitespace_mixed() {
        let mut reader = StringReader::new(" \t \t\nHello!");
        reader.skip_whitespace();
        assert_eq!(reader.cursor(), 5);
    }

    #[test]
    fn skip_whitespace_empty() {
        let mut reader = StringReader::new("");
        reader.skip_whitespace();
        assert_eq!(reader.cursor(), 0);
    }

    #[test]
    fn read_unquoted_string() {
        let mut reader = StringReader::new("hello world");
        assert_eq!(reader.read_unquoted_string(), "hello");
        assert_eq!(reader.read_so_far(), "hello");
        assert_eq!(reader.remaining(), " world");
    }

    #[test]
    fn read_unquoted_string_empty() {
        let mut reader = StringReader::new("");
        assert_eq!(reader.read_unquoted_string(), "");
        assert_eq!(reader.read_so_far(), "");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_unquoted_string_empty_with_remaining() {
        let mut reader = StringReader::new(" hello world");
        assert_eq!(reader.read_unquoted_string(), "");
        assert_eq!(reader.read_so_far(), "");
        assert_eq!(reader.remaining(), " hello world");
    }

    #[test]
    fn read_quoted_string() {
        let mut reader = StringReader::new("\"hello world\"");
        assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
        assert_eq!(reader.read_so_far(), "\"hello world\"");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_single_quoted_string() {
        let mut reader = StringReader::new("'hello world'");
        assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
        assert_eq!(reader.read_so_far(), "'hello world'");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_mixed_quoted_string_double_inside_single() {
        let mut reader = StringReader::new("'hello \"world\"'");
        assert_eq!(reader.read_quoted_string().unwrap(), "hello \"world\"");
        assert_eq!(reader.read_so_far(), "'hello \"world\"'");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_mixed_quoted_string_single_inside_double() {
        let mut reader = StringReader::new("\"hello 'world'\"");
        assert_eq!(reader.read_quoted_string().unwrap(), "hello 'world'");
        assert_eq!(reader.read_so_far(), "\"hello 'world'\"");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_quoted_string_empty() {
        let mut reader = StringReader::new("");
        assert_eq!(reader.read_quoted_string().unwrap(), "");
        assert_eq!(reader.read_so_far(), "");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_quoted_string_empty_quoted() {
        let mut reader = StringReader::new("\"\"");
        assert_eq!(reader.read_quoted_string().unwrap(), "");
        assert_eq!(reader.read_so_far(), "\"\"");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_quoted_string_empty_quoted_with_remaining() {
        let mut reader = StringReader::new("\"\" hello world");
        assert_eq!(reader.read_quoted_string().unwrap(), "");
        assert_eq!(reader.read_so_far(), "\"\"");
        assert_eq!(reader.remaining(), " hello world");
    }

    #[test]
    fn read_quoted_string_with_escaped_quote() {
        let mut reader = StringReader::new("\"hello \\\"world\\\"\"");
        assert_eq!(reader.read_quoted_string().unwrap(), "hello \"world\"");
        assert_eq!(reader.read_so_far(), "\"hello \\\"world\\\"\"");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_quoted_string_with_escaped_escapes() {
        let mut reader = StringReader::new("\"\\\\o/\"");
        assert_eq!(reader.read_quoted_string().unwrap(), "\\o/");
        assert_eq!(reader.read_so_far(), "\"\\\\o/\"");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_quoted_string_with_remaining() {
        let mut reader = StringReader::new("\"hello world\" foo bar");
        assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
        assert_eq!(reader.read_so_far(), "\"hello world\"");
        assert_eq!(reader.remaining(), " foo bar");
    }

    #[test]
    fn read_quoted_string_with_immediate_remaining() {
        let mut reader = StringReader::new("\"hello world\"foo bar");
        assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
        assert_eq!(reader.read_so_far(), "\"hello world\"");
        assert_eq!(reader.remaining(), "foo bar");
    }

    #[test]
    fn utf16_string_reading_preserves_unpaired_surrogates() {
        let mut reader = StringReader::from_utf16(vec![b'"' as u16, 0xd800, b'"' as u16]);
        assert_eq!(reader.read_quoted_string_utf16().unwrap(), [0xd800]);
        assert_eq!(
            reader.read_so_far_utf16(),
            [b'"' as u16, 0xd800, b'"' as u16]
        );
        assert!(reader.remaining_utf16().is_empty());
    }

    #[test]
    fn read_quoted_string_no_open() {
        let mut reader = StringReader::new("hello world\"");
        assert_error!(
            reader.read_quoted_string(),
            BUILT_IN_EXCEPTIONS.reader_expected_start_of_quote(),
            0
        );
    }

    #[test]
    fn read_quoted_string_no_close() {
        let mut reader = StringReader::new("\"hello world");
        assert_error!(
            reader.read_quoted_string(),
            BUILT_IN_EXCEPTIONS.reader_expected_end_of_quote(),
            12
        );
    }

    #[test]
    fn read_quoted_string_invalid_escape() {
        let mut reader = StringReader::new("\"hello\\nworld\"");
        assert_error!(
            reader.read_quoted_string(),
            BUILT_IN_EXCEPTIONS.reader_invalid_escape(),
            7
        );
    }

    #[test]
    fn read_quoted_string_invalid_quote_escape() {
        let mut reader = StringReader::new("'hello\\\"'world");
        assert_error!(
            reader.read_quoted_string(),
            BUILT_IN_EXCEPTIONS.reader_invalid_escape(),
            7
        );
    }

    #[test]
    fn read_string_no_quotes() {
        let mut reader = StringReader::new("hello world");
        assert_eq!(reader.read_string().unwrap(), "hello");
        assert_eq!(reader.read_so_far(), "hello");
        assert_eq!(reader.remaining(), " world");
    }

    #[test]
    fn read_string_single_quotes() {
        let mut reader = StringReader::new("'hello world'");
        assert_eq!(reader.read_string().unwrap(), "hello world");
        assert_eq!(reader.read_so_far(), "'hello world'");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_string_double_quotes() {
        let mut reader = StringReader::new("\"hello world\"");
        assert_eq!(reader.read_string().unwrap(), "hello world");
        assert_eq!(reader.read_so_far(), "\"hello world\"");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_int() {
        let mut reader = StringReader::new("1234567890");
        assert_eq!(reader.read_int().unwrap(), 1_234_567_890);
        assert_eq!(reader.read_so_far(), "1234567890");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_int_negative() {
        let mut reader = StringReader::new("-1234567890");
        assert_eq!(reader.read_int().unwrap(), -1_234_567_890);
        assert_eq!(reader.read_so_far(), "-1234567890");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_int_invalid() {
        let mut reader = StringReader::new("12.34");
        assert_error!(
            reader.read_int(),
            BUILT_IN_EXCEPTIONS.reader_invalid_int(),
            0
        );
    }

    #[test]
    fn read_int_none() {
        let mut reader = StringReader::new("");
        assert_error!(
            reader.read_int(),
            BUILT_IN_EXCEPTIONS.reader_expected_int(),
            0
        );
    }

    #[test]
    fn read_int_with_remaining() {
        let mut reader = StringReader::new("1234567890 foo bar");
        assert_eq!(reader.read_int().unwrap(), 1_234_567_890);
        assert_eq!(reader.read_so_far(), "1234567890");
        assert_eq!(reader.remaining(), " foo bar");
    }

    #[test]
    fn read_int_with_remaining_immediate() {
        let mut reader = StringReader::new("1234567890foo bar");
        assert_eq!(reader.read_int().unwrap(), 1_234_567_890);
        assert_eq!(reader.read_so_far(), "1234567890");
        assert_eq!(reader.remaining(), "foo bar");
    }

    #[test]
    fn read_long() {
        let mut reader = StringReader::new("1234567890");
        assert_eq!(reader.read_long().unwrap(), 1_234_567_890_i64);
        assert_eq!(reader.read_so_far(), "1234567890");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_long_negative() {
        let mut reader = StringReader::new("-1234567890");
        assert_eq!(reader.read_long().unwrap(), -1_234_567_890_i64);
        assert_eq!(reader.read_so_far(), "-1234567890");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_long_invalid() {
        let mut reader = StringReader::new("12.34");
        assert_error!(
            reader.read_long(),
            BUILT_IN_EXCEPTIONS.reader_invalid_long(),
            0
        );
    }

    #[test]
    fn read_long_none() {
        let mut reader = StringReader::new("");
        assert_error!(
            reader.read_long(),
            BUILT_IN_EXCEPTIONS.reader_expected_long(),
            0
        );
    }

    #[test]
    fn read_long_with_remaining() {
        let mut reader = StringReader::new("1234567890 foo bar");
        assert_eq!(reader.read_long().unwrap(), 1_234_567_890_i64);
        assert_eq!(reader.read_so_far(), "1234567890");
        assert_eq!(reader.remaining(), " foo bar");
    }

    #[test]
    fn read_long_with_remaining_immediate() {
        let mut reader = StringReader::new("1234567890foo bar");
        assert_eq!(reader.read_long().unwrap(), 1_234_567_890_i64);
        assert_eq!(reader.read_so_far(), "1234567890");
        assert_eq!(reader.remaining(), "foo bar");
    }

    #[test]
    fn read_double() {
        let mut reader = StringReader::new("123");
        assert_eq!(reader.read_double().unwrap(), 123.0);
        assert_eq!(reader.read_so_far(), "123");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_double_with_decimal() {
        let mut reader = StringReader::new("12.34");
        assert_eq!(reader.read_double().unwrap(), 12.34);
        assert_eq!(reader.read_so_far(), "12.34");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_double_negative() {
        let mut reader = StringReader::new("-123");
        assert_eq!(reader.read_double().unwrap(), -123.0);
        assert_eq!(reader.read_so_far(), "-123");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_double_invalid() {
        let mut reader = StringReader::new("12.34.56");
        assert_error!(
            reader.read_double(),
            BUILT_IN_EXCEPTIONS.reader_invalid_double(),
            0
        );
    }

    #[test]
    fn read_double_none() {
        let mut reader = StringReader::new("");
        assert_error!(
            reader.read_double(),
            BUILT_IN_EXCEPTIONS.reader_expected_double(),
            0
        );
    }

    #[test]
    fn read_double_with_remaining() {
        let mut reader = StringReader::new("12.34 foo bar");
        assert_eq!(reader.read_double().unwrap(), 12.34);
        assert_eq!(reader.read_so_far(), "12.34");
        assert_eq!(reader.remaining(), " foo bar");
    }

    #[test]
    fn read_double_with_remaining_immediate() {
        let mut reader = StringReader::new("12.34foo bar");
        assert_eq!(reader.read_double().unwrap(), 12.34);
        assert_eq!(reader.read_so_far(), "12.34");
        assert_eq!(reader.remaining(), "foo bar");
    }

    #[test]
    fn read_float() {
        let mut reader = StringReader::new("123");
        assert_eq!(reader.read_float().unwrap(), 123.0_f32);
        assert_eq!(reader.read_so_far(), "123");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_float_with_decimal() {
        let mut reader = StringReader::new("12.34");
        assert_eq!(reader.read_float().unwrap(), 12.34_f32);
        assert_eq!(reader.read_so_far(), "12.34");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_float_negative() {
        let mut reader = StringReader::new("-123");
        assert_eq!(reader.read_float().unwrap(), -123.0_f32);
        assert_eq!(reader.read_so_far(), "-123");
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn read_float_invalid() {
        let mut reader = StringReader::new("12.34.56");
        assert_error!(
            reader.read_float(),
            BUILT_IN_EXCEPTIONS.reader_invalid_float(),
            0
        );
    }

    #[test]
    fn read_float_none() {
        let mut reader = StringReader::new("");
        assert_error!(
            reader.read_float(),
            BUILT_IN_EXCEPTIONS.reader_expected_float(),
            0
        );
    }

    #[test]
    fn read_float_with_remaining() {
        let mut reader = StringReader::new("12.34 foo bar");
        assert_eq!(reader.read_float().unwrap(), 12.34_f32);
        assert_eq!(reader.read_so_far(), "12.34");
        assert_eq!(reader.remaining(), " foo bar");
    }

    #[test]
    fn read_float_with_remaining_immediate() {
        let mut reader = StringReader::new("12.34foo bar");
        assert_eq!(reader.read_float().unwrap(), 12.34_f32);
        assert_eq!(reader.read_so_far(), "12.34");
        assert_eq!(reader.remaining(), "foo bar");
    }

    #[test]
    fn expect_correct() {
        let mut reader = StringReader::new("abc");
        reader.expect(b'a' as u16).unwrap();
        assert_eq!(reader.cursor(), 1);
    }

    #[test]
    fn expect_incorrect() {
        let mut reader = StringReader::new("bcd");
        assert_error!(
            reader.expect(b'a' as u16),
            BUILT_IN_EXCEPTIONS.reader_expected_symbol(),
            0
        );
    }

    #[test]
    fn expect_none() {
        let mut reader = StringReader::new("");
        assert_error!(
            reader.expect(b'a' as u16),
            BUILT_IN_EXCEPTIONS.reader_expected_symbol(),
            0
        );
    }

    #[test]
    fn read_boolean_correct() {
        let mut reader = StringReader::new("true");
        assert!(reader.read_boolean().unwrap());
        assert_eq!(reader.read_so_far(), "true");
    }

    #[test]
    fn read_boolean_incorrect() {
        let mut reader = StringReader::new("tuesday");
        assert_error!(
            reader.read_boolean(),
            BUILT_IN_EXCEPTIONS.reader_invalid_bool(),
            0
        );
    }

    #[test]
    fn read_boolean_none() {
        let mut reader = StringReader::new("");
        assert_error!(
            reader.read_boolean(),
            BUILT_IN_EXCEPTIONS.reader_expected_bool(),
            0
        );
    }

    #[test]
    fn cursor_counts_java_utf16_code_units() {
        let mut reader = StringReader::new("a😀b");
        assert_eq!(reader.total_length(), 4);
        assert_eq!(reader.read(), b'a' as u16);
        assert_eq!(reader.read(), 0xd83d);
        assert_eq!(reader.cursor(), 2);
        assert_eq!(reader.read(), 0xde00);
        assert_eq!(reader.substring_utf16(1, 3), vec![0xd83d, 0xde00]);
        assert_eq!(reader.substring(1, 3), "😀");
        assert_eq!(reader.remaining(), "b");
    }
}
