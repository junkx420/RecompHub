//! Valve's key-value formats: binary VDF (`shortcuts.vdf`) and text VDF
//! (`loginusers.vdf`).
//!
//! The binary reader refuses anything it does not fully understand (unknown
//! type bytes, invalid UTF-8, trailing data), so a file is only rewritten
//! when [`write`] reproduces it exactly. Keys keep their order.

use crate::error::{AppError, Result};

const MAP: u8 = 0x00;
const STRING: u8 = 0x01;
const INT32: u8 = 0x02;
const FLOAT32: u8 = 0x03;
const UINT64: u8 = 0x07;
const END: u8 = 0x08;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Map(Map),
    Str(String),
    Int(i32),
    Float(f32),
    U64(u64),
}

/// Ordered key-value pairs; Valve files rely on order (e.g. "0", "1", ...).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Map(pub Vec<(String, Value)>);

impl Map {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    /// Set `key`, keeping its position if it exists, else appending.
    pub fn set(&mut self, key: &str, value: Value) {
        match self.0.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some((_, v)) => *v = value,
            None => self.0.push((key.to_owned(), value)),
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn map(&self, key: &str) -> Option<&Map> {
        match self.get(key)? {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn map_mut(&mut self, key: &str) -> Option<&mut Map> {
        match self
            .0
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))?
        {
            (_, Value::Map(m)) => Some(m),
            _ => None,
        }
    }
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput {
        message: format!("VDF: {}", message.into()),
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or_else(|| invalid("unexpected end"))?;
        self.pos += 1;
        Ok(b)
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let bytes = self
            .data
            .get(self.pos..self.pos + N)
            .ok_or_else(|| invalid("unexpected end"))?;
        self.pos += N;
        Ok(bytes.try_into().expect("slice has length N"))
    }

    fn cstr(&mut self) -> Result<String> {
        let rest = &self.data[self.pos..];
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("unterminated string"))?;
        let s = std::str::from_utf8(&rest[..len])
            .map_err(|_| invalid("string is not UTF-8"))?
            .to_owned();
        self.pos += len + 1;
        Ok(s)
    }

    fn map(&mut self) -> Result<Map> {
        let mut map = Map::default();
        loop {
            let kind = self.byte()?;
            if kind == END {
                return Ok(map);
            }
            let key = self.cstr()?;
            let value = match kind {
                MAP => Value::Map(self.map()?),
                STRING => Value::Str(self.cstr()?),
                INT32 => Value::Int(i32::from_le_bytes(self.take()?)),
                FLOAT32 => Value::Float(f32::from_le_bytes(self.take()?)),
                UINT64 => Value::U64(u64::from_le_bytes(self.take()?)),
                other => return Err(invalid(format!("unsupported type 0x{other:02x}"))),
            };
            map.0.push((key, value));
        }
    }
}

/// Parse a binary VDF document (a map terminated by `0x08`).
pub fn parse(data: &[u8]) -> Result<Map> {
    let mut reader = Reader { data, pos: 0 };
    let map = reader.map()?;
    if reader.pos != data.len() {
        return Err(invalid("trailing data"));
    }
    Ok(map)
}

fn write_map(map: &Map, out: &mut Vec<u8>) {
    for (key, value) in &map.0 {
        let kind = match value {
            Value::Map(_) => MAP,
            Value::Str(_) => STRING,
            Value::Int(_) => INT32,
            Value::Float(_) => FLOAT32,
            Value::U64(_) => UINT64,
        };
        out.push(kind);
        out.extend_from_slice(key.as_bytes());
        out.push(0);
        match value {
            Value::Map(m) => write_map(m, out),
            Value::Str(s) => {
                out.extend_from_slice(s.as_bytes());
                out.push(0);
            }
            Value::Int(i) => out.extend_from_slice(&i.to_le_bytes()),
            Value::Float(f) => out.extend_from_slice(&f.to_le_bytes()),
            Value::U64(u) => out.extend_from_slice(&u.to_le_bytes()),
        }
    }
    out.push(END);
}

pub fn write(map: &Map) -> Vec<u8> {
    let mut out = Vec::new();
    write_map(map, &mut out);
    out
}

/// Text VDF: only strings and nested maps.
#[derive(Debug, Clone, PartialEq)]
pub enum Text {
    Str(String),
    Map(Vec<(String, Text)>),
}

impl Text {
    pub fn get(&self, key: &str) -> Option<&Text> {
        match self {
            Text::Map(entries) => entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Text::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Text::Str(s) => Some(s),
            Text::Map(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Text)] {
        match self {
            Text::Map(entries) => entries,
            Text::Str(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next().ok_or_else(|| invalid("unterminated string"))? {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => return Err(invalid("unterminated string")),
                        },
                        other => s.push(other),
                    }
                }
                tokens.push(Token::Str(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            other => return Err(invalid(format!("unexpected character {other:?}"))),
        }
    }
    Ok(tokens)
}

/// Parse a text VDF document into a map of its top-level keys.
pub fn parse_text(text: &str) -> Result<Text> {
    fn entries(
        tokens: &mut std::vec::IntoIter<Token>,
        nested: bool,
    ) -> Result<Vec<(String, Text)>> {
        let mut out = Vec::new();
        loop {
            let key = match tokens.next() {
                Some(Token::Str(key)) => key,
                Some(Token::Close) if nested => return Ok(out),
                None if !nested => return Ok(out),
                other => return Err(invalid(format!("expected key, got {other:?}"))),
            };
            let value = match tokens.next() {
                Some(Token::Str(s)) => Text::Str(s),
                Some(Token::Open) => Text::Map(entries(tokens, true)?),
                other => return Err(invalid(format!("expected value, got {other:?}"))),
            };
            out.push((key, value));
        }
    }
    let mut tokens = tokenize(text.trim_start_matches('\u{feff}'))?.into_iter();
    Ok(Text::Map(entries(&mut tokens, false)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut entry = Map::default();
        entry.set("appid", Value::Int(-1_234_567));
        entry.set("AppName", Value::Str("Zelda 64: Recompiled".into()));
        entry.set("Exe", Value::Str("\"C:\\Games\\z64.exe\"".into()));
        entry.set("LastPlayTime", Value::Int(1_700_000_000));
        entry.set("speed", Value::Float(1.5));
        entry.set("big", Value::U64(u64::MAX));
        entry.set(
            "tags",
            Value::Map(Map(vec![("0".into(), Value::Str("N64".into()))])),
        );
        let mut shortcuts = Map::default();
        shortcuts.set("0", Value::Map(entry));
        let mut root = Map::default();
        root.set("shortcuts", Value::Map(shortcuts));
        write(&root)
    }

    #[test]
    fn binary_roundtrip_is_exact() {
        let bytes = sample();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(write(&parsed), bytes);
        let entry = parsed.map("shortcuts").unwrap().map("0").unwrap();
        assert_eq!(entry.str("appname"), Some("Zelda 64: Recompiled"));
    }

    #[test]
    fn binary_rejects_what_it_cannot_reproduce() {
        let mut bytes = sample();
        bytes.push(0);
        assert!(parse(&bytes).is_err(), "trailing data");
        let bytes = [0x05, b'k', 0, b'v', 0, 0x08];
        assert!(parse(&bytes).is_err(), "wide string type");
        let bytes = [0x01, b'k', 0, 0xff, 0xfe, 0, 0x08];
        assert!(parse(&bytes).is_err(), "invalid UTF-8");
        let bytes = sample();
        assert!(parse(&bytes[..bytes.len() - 3]).is_err(), "truncated");
    }

    #[test]
    fn empty_shortcuts_file() {
        let bytes = b"\x00shortcuts\x00\x08\x08";
        let parsed = parse(bytes).unwrap();
        assert_eq!(parsed.map("shortcuts"), Some(&Map::default()));
        assert_eq!(write(&parsed), bytes);
    }

    #[test]
    fn text_vdf_like_loginusers() {
        let text = "\u{feff}\"users\"\n{\n\t\"76561198000000001\"\n\t{\n\t\t\"AccountName\"\t\t\"me\"\n\t\t\"PersonaName\"\t\t\"Me \\\"Q\\\"\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t}\n\t// comment\n\t\"76561198000000002\" { \"AccountName\" \"other\" }\n}\n";
        let doc = parse_text(text).unwrap();
        let users = doc.get("users").unwrap();
        assert_eq!(users.entries().len(), 2);
        let me = users.get("76561198000000001").unwrap();
        assert_eq!(me.get("PersonaName").unwrap().as_str(), Some("Me \"Q\""));
        assert_eq!(me.get("mostrecent").unwrap().as_str(), Some("1"));
        assert!(parse_text("\"a\" {").is_err());
    }
}
