//! A closed JSON reader (specs/144 DOC2-03).
//!
//! Two adapters read a small JSON document from a host: the merge's dataset and the block description
//! a template is built from. One grammar, one place, so a host cannot find that one path accepts what
//! the other refuses.
//!
//! It accepts exactly what a host emits: objects, arrays, strings, numbers, booleans, and null. It is
//! not a general JSON parser and it is not allowed to become one.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(value) if *value >= 0.0 && value.fract() == 0.0 => Some(*value as u64),
            _ => None,
        }
    }

    /// One record: every value must be text, or a number/bool/null that a host widened for us.
    pub fn as_string_map(&self, what: &str) -> Result<BTreeMap<String, String>, String> {
        let Some(map) = self.as_object() else {
            return Err(format!("{what} must be an object."));
        };
        let mut out = BTreeMap::new();
        for (key, value) in map {
            let text = match value {
                Json::Text(text) => text.clone(),
                Json::Number(number) => {
                    // A host must send a decimal string for money; a float here is a host defect
                    // that would round a price, so it keeps every digit JavaScript printed.
                    if number.fract() == 0.0 && number.abs() < 9.0e15 {
                        format!("{}", *number as i64)
                    } else {
                        format!("{number}")
                    }
                }
                Json::Bool(flag) => flag.to_string(),
                Json::Null => String::new(),
                _ => {
                    return Err(format!(
                        "{what}.{key} must be text, a number, a boolean, or null."
                    ))
                }
            };
            out.insert(key.clone(), text);
        }
        Ok(out)
    }
}

/// Read one value. The grammar is closed and the depth is bounded, so a hostile payload cannot
/// recurse without limit.
pub fn parse_json(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.skip_space();
    let value = parser.value(0)?;
    parser.skip_space();
    if parser.at != parser.bytes.len() {
        return Err(format!(
            "Unexpected text at offset {}. The document is one JSON value.",
            parser.at
        ));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

const MAX_DEPTH: usize = 32;

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.at += 1;
        Some(byte)
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8, what: &str) -> Result<(), String> {
        match self.bump() {
            Some(found) if found == byte => Ok(()),
            _ => Err(format!(
                "Expected {what} at offset {}. The document is not valid JSON.",
                self.at
            )),
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return Err("The document is nested too deeply.".to_string());
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::Text(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.number(),
            _ => Err(format!(
                "Expected a value at offset {}. The document is not valid JSON.",
                self.at
            )),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(format!("Expected {word} at offset {}.", self.at))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'{', "\"{\"")?;
        let mut map = BTreeMap::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            self.skip_space();
            self.expect(b':', "\":\"")?;
            self.skip_space();
            let value = self.value(depth + 1)?;
            map.insert(key, value);
            self.skip_space();
            match self.bump() {
                Some(b',') => continue,
                Some(b'}') => return Ok(Json::Object(map)),
                _ => return Err(format!("Expected \",\" or \"}}\" at offset {}.", self.at)),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'[', "\"[\"")?;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth + 1)?);
            self.skip_space();
            match self.bump() {
                Some(b',') => continue,
                Some(b']') => return Ok(Json::Array(items)),
                _ => return Err(format!("Expected \",\" or \"]\" at offset {}.", self.at)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"', "\"\\\"\"")?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("A text value is not closed.".to_string()),
                Some(b'"') => return Ok(out),
                Some(b'\\') => match self.bump() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{8}'),
                    Some(b'f') => out.push('\u{c}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => out.push(self.unicode_escape()?),
                    _ => return Err(format!("Unknown escape at offset {}.", self.at)),
                },
                Some(byte) => {
                    // Multi-byte UTF-8 arrives one byte at a time; collect the continuation bytes.
                    let start = self.at - 1;
                    let width = utf8_width(byte);
                    if width > 1 {
                        self.at = start + width;
                        if self.at > self.bytes.len() {
                            return Err("A text value ends inside a character.".to_string());
                        }
                    }
                    let slice = &self.bytes[start..self.at];
                    match std::str::from_utf8(slice) {
                        Ok(text) => out.push_str(text),
                        Err(_) => return Err(format!("Invalid UTF-8 at offset {start}.")),
                    }
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let mut code = 0u32;
        for _ in 0..4 {
            let byte = self
                .bump()
                .ok_or_else(|| "A \\u escape is short.".to_string())?;
            let digit = (byte as char)
                .to_digit(16)
                .ok_or_else(|| format!("A \\u escape has a bad digit at offset {}.", self.at))?;
            code = code * 16 + digit;
        }
        char::from_u32(code).ok_or_else(|| "A \\u escape is not a character.".to_string())
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
            self.at += 1;
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.at += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or("");
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| format!("Invalid number at offset {start}."))
    }
}

fn utf8_width(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b11110 {
        4
    } else {
        1
    }
}
