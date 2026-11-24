//! Minimal zero-dependency JSON parser, serializer, and value tree.

use crate::error::{AxonError, Result};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        self.as_object().and_then(|o| o.get(key))
    }

    pub fn parse(input: &str) -> Result<Json> {
        let mut parser = Parser::new(input);
        let val = parser.parse_value()?;
        parser.skip_ws();
        if !parser.is_eof() {
            return Err(AxonError::ParseError(
                "Trailing characters after JSON value".into(),
            ));
        }
        Ok(val)
    }

    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        self.fmt_pretty(&mut out, 0);
        out
    }

    fn fmt_pretty(&self, out: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        let child_pad = "  ".repeat(indent + 1);
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            Json::String(s) => write_escaped_str(out, s),
            Json::Array(arr) => {
                if arr.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for (i, item) in arr.iter().enumerate() {
                    out.push_str(&child_pad);
                    item.fmt_pretty(out, indent + 1);
                    if i + 1 < arr.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push(']');
            }
            Json::Object(map) => {
                if map.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (k, v)) in map.iter().enumerate() {
                    out.push_str(&child_pad);
                    write_escaped_str(out, k);
                    out.push_str(": ");
                    v.fmt_pretty(out, indent + 1);
                    if i + 1 < map.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push('}');
            }
        }
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Json::Null => write!(f, "null"),
            Json::Bool(b) => write!(f, "{}", b),
            Json::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Json::String(s) => {
                let mut buf = String::new();
                write_escaped_str(&mut buf, s);
                write!(f, "{}", buf)
            }
            Json::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Json::Object(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    let mut kb = String::new();
                    write_escaped_str(&mut kb, k);
                    write!(f, "{}:{}", kb, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

pub fn write_escaped_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            src: input.as_bytes(),
            pos: 0,
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self) -> Result<Json> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_null(),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(b'"') => self.parse_string().map(Json::String),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(c) => Err(AxonError::ParseError(format!(
                "Unexpected byte '{}' at position {}",
                c as char, self.pos
            ))),
            None => Err(AxonError::ParseError("Unexpected EOF".into())),
        }
    }

    fn parse_null(&mut self) -> Result<Json> {
        if self.src.get(self.pos..self.pos + 4) == Some(b"null") {
            self.pos += 4;
            Ok(Json::Null)
        } else {
            Err(AxonError::ParseError("Invalid literal".into()))
        }
    }

    fn parse_bool(&mut self) -> Result<Json> {
        if self.src.get(self.pos..self.pos + 4) == Some(b"true") {
            self.pos += 4;
            Ok(Json::Bool(true))
        } else if self.src.get(self.pos..self.pos + 5) == Some(b"false") {
            self.pos += 5;
            Ok(Json::Bool(false))
        } else {
            Err(AxonError::ParseError("Invalid boolean".into()))
        }
    }

    fn parse_string(&mut self) -> Result<String> {
        if self.next() != Some(b'"') {
            return Err(AxonError::ParseError("Expected double quote".into()));
        }
        let mut out = String::new();
        while let Some(b) = self.next() {
            match b {
                b'"' => return Ok(out),
                b'\\' => match self.next() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{0008}'),
                    Some(b'f') => out.push('\u{000c}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => {
                        let hex = self
                            .src
                            .get(self.pos..self.pos + 4)
                            .ok_or_else(|| AxonError::ParseError("Incomplete unicode escape".into()))?;
                        let hex_str = std::str::from_utf8(hex)
                            .map_err(|_| AxonError::ParseError("Invalid unicode escape".into()))?;
                        let code = u32::from_str_radix(hex_str, 16)
                            .map_err(|_| AxonError::ParseError("Invalid hex in unicode escape".into()))?;
                        if let Some(ch) = char::from_u32(code) {
                            out.push(ch);
                        }
                        self.pos += 4;
                    }
                    _ => return Err(AxonError::ParseError("Invalid escape sequence".into())),
                },
                _ => {
                    // Decode UTF-8 sequence starting at pos - 1
                    let start = self.pos - 1;
                    let remaining = &self.src[start..];
                    let s = std::str::from_utf8(remaining).unwrap_or_else(|e| {
                        let valid = e.valid_up_to();
                        std::str::from_utf8(&remaining[..valid]).unwrap_or("")
                    });
                    if let Some(ch) = s.chars().next() {
                        out.push(ch);
                        self.pos = start + ch.len_utf8();
                    } else {
                        return Err(AxonError::ParseError("Invalid UTF-8 in string".into()));
                    }
                }
            }
        }
        Err(AxonError::ParseError("Unterminated string".into()))
    }

    fn parse_number(&mut self) -> Result<Json> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let num_str = std::str::from_utf8(&self.src[start..self.pos])
            .map_err(|_| AxonError::ParseError("Invalid number".into()))?;
        let val: f64 = num_str
            .parse()
            .map_err(|_| AxonError::ParseError(format!("Invalid float '{}'", num_str)))?;
        Ok(Json::Number(val))
    }

    fn parse_array(&mut self) -> Result<Json> {
        self.next(); // '['
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.next();
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.next() {
                Some(b',') => continue,
                Some(b']') => break,
                _ => return Err(AxonError::ParseError("Expected ',' or ']'".into())),
            }
        }
        Ok(Json::Array(items))
    }

    fn parse_object(&mut self) -> Result<Json> {
        self.next(); // '{'
        self.skip_ws();
        let mut map = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.next();
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.next() != Some(b':') {
                return Err(AxonError::ParseError("Expected ':' in object".into()));
            }
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.next() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err(AxonError::ParseError("Expected ',' or '}'".into())),
            }
        }
        Ok(Json::Object(map))
    }
}
