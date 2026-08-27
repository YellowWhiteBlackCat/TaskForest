//! A small, std-only, fail-closed JSON reader for the helper contract.
//!
//! The escalation crate is documented zero-dependency (ADR-023), so the shared
//! JSON contract the privileged helper emits is parsed without `serde`. This
//! reader accepts the standard JSON grammar (objects, arrays, strings with
//! escapes including `\uXXXX` plus surrogate-pair joining, numbers, and
//! `true`/`false`/`null`) and rejects trailing garbage; any syntax error
//! propagates as `Err(())`, which the contract layer in the parent module turns
//! into `ParsedOutput::NotContract`. This fail-closed
//! behavior IS the honesty red line: anything that is not exactly the contract
//! becomes a typed denial, never a fabricated engine row.
//!
//! This is deliberately NOT a general-purpose JSON library. It exists only to
//! read the two fixed shapes the helper emits (SUCCESS with an `engines` array,
//! ERROR with a `status`), so it is kept private to the `polkit` module.

#![forbid(unsafe_code)]

/// A minimal JSON value tree. `Object` keeps insertion order as a `Vec` so field
/// lookup is deterministic and the crate stays free of `HashMap` (not needed for
/// the tiny contract objects).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// First value bound to `key`, or `None` (also when `self` is not an object).
    pub(super) fn get(&self, key: &str) -> Option<&Json> {
        if let Json::Object(entries) = self {
            entries.iter().find_map(|(k, v)| (k == key).then_some(v))
        } else {
            None
        }
    }

    pub(super) fn as_str(&self) -> Option<&str> {
        if let Json::String(s) = self {
            Some(s)
        } else {
            None
        }
    }

    pub(super) fn as_array(&self) -> Option<&[Json]> {
        if let Json::Array(a) = self {
            Some(a)
        } else {
            None
        }
    }
}

/// Maximum container nesting (objects/arrays) the reader will descend into.
/// The helper contract is a flat two-level object; 64 leaves orders of
/// magnitude of slack while bounding hostile `stdout` from ever overflowing
/// the stack (an unbounded `value` recursion aborts the whole process, which
/// no error type can catch). Deeper input is reported as `Err(())`, the
/// reader's existing failure vocabulary.
const MAX_NESTING_DEPTH: u32 = 64;

pub(super) struct JsonReader<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: u32,
}

impl<'a> JsonReader<'a> {
    /// Parse a complete JSON document. Trailing non-whitespace after the
    /// top-level value is an error (the helper must emit exactly one object).
    pub(super) fn parse(input: &'a str) -> Result<Json, ()> {
        let mut reader = Self {
            bytes: input.as_bytes(),
            pos: 0,
            depth: 0,
        };
        reader.skip_ws();
        let value = reader.value()?;
        reader.skip_ws();
        if reader.pos == reader.bytes.len() {
            Ok(value)
        } else {
            // Trailing garbage after the top-level value: not valid JSON.
            Err(())
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(byte) = self.peek() {
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn value(&mut self) -> Result<Json, ()> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.nested(Self::object),
            Some(b'[') => self.nested(Self::array),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') | Some(b'f') => self.boolean(),
            Some(b'n') => self.null(),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.number(),
            _ => Err(()),
        }
    }

    /// Run one container parser under the nesting guard. The depth counter is
    /// always restored (no early return between increment and decrement), so
    /// sibling containers after a rejection are still measured correctly.
    fn nested(&mut self, container: fn(&mut Self) -> Result<Json, ()>) -> Result<Json, ()> {
        if self.depth >= MAX_NESTING_DEPTH {
            return Err(());
        }
        self.depth += 1;
        let result = container(self);
        self.depth -= 1;
        result
    }

    fn object(&mut self) -> Result<Json, ()> {
        self.pos += 1; // consume '{'
        let mut entries = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(());
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(());
            }
            self.pos += 1; // consume ':'
            let val = self.value()?;
            entries.push((key, val));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(entries));
                }
                _ => return Err(()),
            }
        }
    }

    fn array(&mut self) -> Result<Json, ()> {
        self.pos += 1; // consume '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            let val = self.value()?;
            items.push(val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(()),
            }
        }
    }

    fn string(&mut self) -> Result<String, ()> {
        if self.peek() != Some(b'"') {
            return Err(());
        }
        self.pos += 1; // consume opening quote
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(()); // unterminated
            };
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(esc) = self.peek() else {
                        return Err(());
                    };
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            self.pos += 1; // consume 'u'
                            let cp = self.hex4()?;
                            // Combine a UTF-16 surrogate pair if present.
                            let decoded = if (0xD800..=0xDBFF).contains(&cp) {
                                let low = if self.peek() == Some(b'\\') {
                                    self.pos += 1;
                                    if self.peek() == Some(b'u') {
                                        self.pos += 1;
                                        self.hex4()?
                                    } else {
                                        return Err(());
                                    }
                                } else {
                                    return Err(());
                                };
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(());
                                }
                                0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                cp
                            };
                            match char::from_u32(decoded) {
                                Some(ch) => out.push(ch),
                                None => return Err(()),
                            }
                            continue; // hex4 already advanced past its 4 digits
                        }
                        _ => return Err(()),
                    }
                    self.pos += 1; // consume the simple escape char
                }
                0x00..=0x1F => return Err(()), // control chars must be escaped
                _ => {
                    // One UTF-8 codepoint: consume the whole encoding.
                    let start = self.pos;
                    self.pos += utf8_len(byte);
                    let slice = self.bytes.get(start..self.pos).ok_or(())?;
                    let chunk = std::str::from_utf8(slice).map_err(|_| ())?;
                    out.push_str(chunk);
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, ()> {
        let mut value = 0u32;
        for _ in 0..4 {
            let Some(byte) = self.peek() else {
                return Err(());
            };
            self.pos += 1;
            value = value * 16 + hex_digit(byte).ok_or(())?;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, ()> {
        let start = self.pos;
        // Accept the JSON number grammar: optional '-', integer part, optional
        // fraction, optional exponent.
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(byte) if (b'1'..=b'9').contains(&byte) => {
                while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => return Err(()),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(());
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(());
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text =
            std::str::from_utf8(self.bytes.get(start..self.pos).ok_or(())?).map_err(|_| ())?;
        text.parse::<f64>().map(Json::Number).map_err(|_| ())
    }

    fn boolean(&mut self) -> Result<Json, ()> {
        if self.consume_kw(b"true") {
            Ok(Json::Bool(true))
        } else if self.consume_kw(b"false") {
            Ok(Json::Bool(false))
        } else {
            Err(())
        }
    }

    fn null(&mut self) -> Result<Json, ()> {
        if self.consume_kw(b"null") {
            Ok(Json::Null)
        } else {
            Err(())
        }
    }

    fn consume_kw(&mut self, kw: &[u8]) -> bool {
        if self.bytes.get(self.pos..self.pos + kw.len()) == Some(kw) {
            self.pos += kw.len();
            true
        } else {
            false
        }
    }
}

fn hex_digit(byte: u8) -> Option<u32> {
    Some(match byte {
        b'0'..=b'9' => (byte - b'0') as u32,
        b'a'..=b'f' => (byte - b'a' + 10) as u32,
        b'A'..=b'F' => (byte - b'A' + 10) as u32,
        _ => return None,
    })
}

/// Length of the UTF-8 encoding starting from this leading byte.
fn utf8_len(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b11110 {
        4
    } else {
        // A malformed leading byte: take one so the caller's utf8 check fails.
        1
    }
}

#[cfg(test)]
#[path = "../../tests/headless/escalation_polkit_json_reader.rs"]
mod tests;
