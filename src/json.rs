//! Small JSON reader and string encoder for package-manager output, the PowerStore
//! export file and the icon database. No dependency; nesting is bounded so hostile
//! input cannot exhaust the stack. Numbers keep their source text.

/// Deeper documents are rejected; real package-manager output stays far below this.
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    /// Entries in document order; duplicate keys are kept, and lookups take the first.
    Object(Vec<(String, Value)>),
}

/// Parses one JSON document. Surrounding whitespace and a UTF-8 byte-order mark are
/// allowed; anything else after the value is an error.
pub fn parse(text: &str) -> Option<Value> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = Reader {
        bytes: text.as_bytes(),
        text,
        at: 0,
    };
    let value = reader.value(0)?;
    reader.skip_whitespace();
    (reader.at == reader.bytes.len()).then_some(value)
}

impl Value {
    /// The first value under `key` when this is an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries()?
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn entries(&self) -> Option<&[(String, Value)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Object elements of an array; other elements are skipped.
    pub fn objects(&self) -> impl Iterator<Item = &Value> {
        self.as_array()
            .unwrap_or_default()
            .iter()
            .filter(|value| matches!(value, Self::Object(_)))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::Number(text) => text.parse().ok(),
            _ => None,
        }
    }

    /// The string under `key`; `None` when it is missing or not a string.
    pub fn str_field(&self, key: &str) -> Option<String> {
        self.get(key)?.as_str().map(str::to_string)
    }

    /// The string under the first of `keys` that holds one.
    pub fn str_any(&self, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|key| self.str_field(key))
    }
}

/// `value` as a JSON string literal, quotes included.
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.skip_whitespace();
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Option<Value> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Some(value)
        } else {
            None
        }
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_whitespace();
        match *self.bytes.get(self.at)? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(Value::String),
            b't' => self.literal("true", Value::Bool(true)),
            b'f' => self.literal("false", Value::Bool(false)),
            b'n' => self.literal("null", Value::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn object(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut entries = Vec::new();
        if self.eat(b'}') {
            return Some(Value::Object(entries));
        }
        loop {
            self.skip_whitespace();
            if self.bytes.get(self.at) != Some(&b'"') {
                return None;
            }
            let key = self.string()?;
            if !self.eat(b':') {
                return None;
            }
            entries.push((key, self.value(depth + 1)?));
            if self.eat(b',') {
                continue;
            }
            return self.eat(b'}').then_some(Value::Object(entries));
        }
    }

    fn array(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut items = Vec::new();
        if self.eat(b']') {
            return Some(Value::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            if self.eat(b',') {
                continue;
            }
            return self.eat(b']').then_some(Value::Array(items));
        }
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.at += 1;
        }
        let text = &self.text[start..self.at];
        text.parse::<f64>()
            .ok()
            .map(|_| Value::Number(text.to_string()))
    }

    /// Reads a string literal starting at its opening quote.
    fn string(&mut self) -> Option<String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            // Copy the run up to the next quote or escape in one step.
            let run = self.text[self.at..].find(['"', '\\'])?;
            let chunk = &self.text[self.at..self.at + run];
            if chunk.chars().any(|c| (c as u32) < 0x20) {
                return None;
            }
            out.push_str(chunk);
            self.at += run;
            let byte = self.bytes[self.at];
            self.at += 1;
            if byte == b'"' {
                return Some(out);
            }
            let escape = *self.bytes.get(self.at)?;
            self.at += 1;
            out.push(match escape {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\u{8}',
                b'f' => '\u{c}',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => self.unicode_escape()?,
                _ => return None,
            });
        }
    }

    /// The character after `\u`, joining a surrogate pair; a lone surrogate becomes U+FFFD.
    fn unicode_escape(&mut self) -> Option<char> {
        let high = self.hex4()?;
        if !(0xD800..0xDC00).contains(&high) {
            return Some(char::from_u32(high).unwrap_or('\u{fffd}'));
        }
        if self.text[self.at..].starts_with("\\u") {
            let save = self.at;
            self.at += 2;
            let low = self.hex4()?;
            if (0xDC00..0xE000).contains(&low) {
                let code = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
                return char::from_u32(code);
            }
            self.at = save;
        }
        Some('\u{fffd}')
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = self.text.get(self.at..self.at + 4)?;
        let code = u32::from_str_radix(digits, 16).ok()?;
        self.at += 4;
        Some(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_parse_into_values() {
        let value = parse(
            "\u{feff} { \"a\": [1, -2.5e3, true, false, null], \"b\": {\"c\": \"d\"}, \"a\": 9 } ",
        )
        .unwrap();
        let a = value.get("a").unwrap().as_array().unwrap();
        assert_eq!(a[0].as_u32(), Some(1));
        assert_eq!(a[1], Value::Number("-2.5e3".into()));
        assert_eq!(a[2], Value::Bool(true));
        assert_eq!(a[4], Value::Null);
        assert_eq!(
            value.get("b").and_then(|b| b.str_field("c")).as_deref(),
            Some("d")
        );
        // The first duplicate wins, and a nested key is not a top-level one.
        assert!(value.get("a").unwrap().as_array().is_some());
        assert_eq!(value.get("c"), None);
    }

    #[test]
    fn strings_decode_every_escape() {
        let value = parse(r#""q\" s\\ \/ \b\f\n\r\t \u00e7 \ud83d\ude00 \ud800x""#).unwrap();
        assert_eq!(
            value.as_str(),
            Some("q\" s\\ / \u{8}\u{c}\n\r\t ç 😀 \u{fffd}x")
        );
        assert_eq!(
            parse(&quote("a\"b\\c\n\u{1}ç")).unwrap().as_str(),
            Some("a\"b\\c\n\u{1}ç")
        );
    }

    #[test]
    fn malformed_and_too_deep_input_is_rejected() {
        for bad in [
            "",
            "{",
            "{\"a\" 1}",
            "{\"a\":1,}",
            "[1 2]",
            "\"open",
            "\"bad \\q\"",
            "\"raw\nnewline\"",
            "tru",
            "{} trailing",
            "-",
            "{'a': 1}",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        let deep = "[".repeat(MAX_DEPTH + 2) + &"]".repeat(MAX_DEPTH + 2);
        assert_eq!(parse(&deep), None);
        let fine = "[".repeat(MAX_DEPTH) + &"]".repeat(MAX_DEPTH);
        assert!(parse(&fine).is_some());
    }

    #[test]
    fn helpers_skip_values_of_the_wrong_type() {
        let value =
            parse(r#"{"list": [{"n": "x"}, 3, "s", {"n": 4}], "name": 5, "Name": "y"}"#).unwrap();
        let names: Vec<_> = value
            .get("list")
            .unwrap()
            .objects()
            .map(|object| object.str_field("n"))
            .collect();
        assert_eq!(names, [Some("x".to_string()), None]);
        assert_eq!(value.str_any(&["name", "Name"]).as_deref(), Some("y"));
        assert_eq!(Value::Null.objects().count(), 0);
    }
}
