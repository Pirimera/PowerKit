//! Token templates (`{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}`) and Windows-safe file stems.
use super::time::WallClock;

pub const DEFAULT: &str = "{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}";
pub const PRESETS: [&str; 4] = [
    DEFAULT,
    "{yyyy}{MM}{dd}_{HH}{mm}{ss}",
    "{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}_{model}",
    "{yyyy}-{MM}-{dd}_{original}",
];
/// Chip tokens in insertion order; labels are localized, token names are not.
pub const CHIPS: [&str; 12] = [
    "{yyyy}",
    "{yy}",
    "{MM}",
    "{dd}",
    "{HH}",
    "{mm}",
    "{ss}",
    "{fff}",
    "{make}",
    "{model}",
    "{original}",
    "{counter}",
];
const MAX_TEMPLATE: usize = 200;
/// A file name component, in UTF-16 units, including the extension.
pub const MAX_NAME_UNITS: usize = 255;
const SEPARATORS: [char; 4] = [' ', '_', '-', '.'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Year,
    YearShort,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Millis,
    Make,
    Model,
    Original,
    Counter(u8),
}
impl Token {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "yyyy" => Self::Year,
            "yy" => Self::YearShort,
            "MM" => Self::Month,
            "dd" => Self::Day,
            "HH" => Self::Hour,
            "mm" => Self::Minute,
            "ss" => Self::Second,
            "fff" => Self::Millis,
            "make" => Self::Make,
            "model" => Self::Model,
            "original" => Self::Original,
            "counter" => Self::Counter(3),
            other => {
                let digits = other.strip_prefix("counter:")?;
                let width = digits.parse::<u8>().ok()?;
                if digits.len() != 1 || !(1..=9).contains(&width) {
                    return None;
                }
                Self::Counter(width)
            }
        })
    }
    fn needs_time(self) -> bool {
        matches!(
            self,
            Self::Year
                | Self::YearShort
                | Self::Month
                | Self::Day
                | Self::Hour
                | Self::Minute
                | Self::Second
                | Self::Millis
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Literal(String),
    Token(Token),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    Empty,
    TooLong,
    /// A `{` without `}`, a nested `{` or a stray `}`.
    Unbalanced,
    Unknown(String),
    Control,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Fields<'a> {
    pub time: Option<&'a WallClock>,
    pub make: Option<&'a str>,
    pub model: Option<&'a str>,
    pub original: &'a str,
    pub counter: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    parts: Vec<Part>,
}
impl Template {
    pub fn parse(text: &str) -> Result<Self, TemplateError> {
        if text.trim().is_empty() {
            return Err(TemplateError::Empty);
        }
        if text.chars().count() > MAX_TEMPLATE {
            return Err(TemplateError::TooLong);
        }
        if text.chars().any(char::is_control) {
            return Err(TemplateError::Control);
        }
        let mut parts = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            match rest.find(['{', '}']) {
                None => {
                    parts.push(Part::Literal(rest.into()));
                    break;
                }
                Some(index) => {
                    if index > 0 {
                        parts.push(Part::Literal(rest[..index].into()));
                    }
                    if rest.as_bytes()[index] == b'}' {
                        return Err(TemplateError::Unbalanced);
                    }
                    let after = &rest[index + 1..];
                    let close = after.find('}').ok_or(TemplateError::Unbalanced)?;
                    let name = &after[..close];
                    if name.contains('{') {
                        return Err(TemplateError::Unbalanced);
                    }
                    let token =
                        Token::parse(name).ok_or_else(|| TemplateError::Unknown(name.into()))?;
                    parts.push(Part::Token(token));
                    rest = &after[close + 1..];
                }
            }
        }
        Ok(Self { parts })
    }
    fn tokens(&self) -> impl Iterator<Item = Token> + '_ {
        self.parts.iter().filter_map(|part| match part {
            Part::Token(token) => Some(*token),
            Part::Literal(_) => None,
        })
    }
    pub fn needs_time(&self) -> bool {
        self.tokens().any(Token::needs_time)
    }
    pub fn has_millis(&self) -> bool {
        self.tokens().any(|token| token == Token::Millis)
    }
    #[cfg(test)]
    pub fn has_counter(&self) -> bool {
        self.tokens()
            .any(|token| matches!(token, Token::Counter(_)))
    }
    /// Raw stem text; an empty token drops the separator directly before it.
    pub fn render(&self, fields: &Fields) -> String {
        let mut out = String::new();
        let mut skip_separator = false;
        for part in &self.parts {
            let value = match part {
                Part::Literal(text) => {
                    let mut text = text.as_str();
                    if skip_separator && text.starts_with(SEPARATORS) {
                        text = &text[1..];
                    }
                    skip_separator = false;
                    out.push_str(text);
                    continue;
                }
                Part::Token(token) => token_value(*token, fields),
            };
            if value.is_empty() {
                if out.ends_with(SEPARATORS) {
                    out.pop();
                } else if out.is_empty() {
                    skip_separator = true;
                }
            } else {
                skip_separator = false;
                out.push_str(&value);
            }
        }
        out
    }
}
fn token_value(token: Token, fields: &Fields) -> String {
    let time = fields.time;
    let two = |value: Option<u8>| value.map(|v| format!("{v:02}")).unwrap_or_default();
    match token {
        Token::Year => time.map(|t| format!("{:04}", t.year)).unwrap_or_default(),
        Token::YearShort => time
            .map(|t| format!("{:02}", t.year % 100))
            .unwrap_or_default(),
        Token::Month => two(time.map(|t| t.month)),
        Token::Day => two(time.map(|t| t.day)),
        Token::Hour => two(time.map(|t| t.hour)),
        Token::Minute => two(time.map(|t| t.minute)),
        Token::Second => two(time.map(|t| t.second)),
        Token::Millis => time
            .and_then(|t| t.millis)
            .map(|millis| format!("{millis:03}"))
            .unwrap_or_default(),
        Token::Make => fields.make.unwrap_or_default().trim().into(),
        Token::Model => fields.model.unwrap_or_default().trim().into(),
        Token::Original => fields.original.into(),
        Token::Counter(width) => format!("{:0width$}", fields.counter, width = width as usize),
    }
}

const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
fn is_reserved(base: &str) -> bool {
    let upper = base.trim_end().to_ascii_uppercase();
    if RESERVED.contains(&upper.as_str()) {
        return true;
    }
    let mut chars = upper.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let rest: String = chars.collect();
    (prefix == "COM" || prefix == "LPT")
        && matches!(
            rest.as_str(),
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
        )
}
fn truncate_units(text: &str, max_units: usize) -> &str {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        units += ch.len_utf16();
        if units > max_units {
            return &text[..index];
        }
    }
    text
}
fn trim_name(text: &str) -> &str {
    text.trim_start_matches(' ').trim_end_matches(['.', ' '])
}
/// A stem Windows accepts in any folder, at most `max_units` UTF-16 units.
pub fn sanitize_stem(raw: &str, max_units: usize) -> String {
    let replaced: String = raw
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let mut stem = trim_name(truncate_units(trim_name(&replaced), max_units.max(1))).to_string();
    if stem.is_empty() {
        stem = "media".into();
    }
    let base_end = stem.find('.').unwrap_or(stem.len());
    if is_reserved(&stem[..base_end]) {
        stem.insert(base_end, '_');
    }
    stem
}
pub fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time() -> WallClock {
        WallClock::new(2026, 5, 1, 14, 3, 22)
            .unwrap()
            .with_millis(Some(7))
    }
    fn render(template: &str, fields: Fields) -> String {
        Template::parse(template).unwrap().render(&fields)
    }
    #[test]
    fn default_and_presets_parse() {
        for preset in PRESETS {
            Template::parse(preset).unwrap();
        }
        let t = time();
        let fields = Fields {
            time: Some(&t),
            original: "IMG_1234",
            counter: 7,
            ..Fields::default()
        };
        assert_eq!(render(DEFAULT, fields), "2026-05-01_14-03-22");
        assert_eq!(render("{yy}{MM}{dd}-{fff}", fields), "260501-007");
        assert_eq!(render("{original}_{counter}", fields), "IMG_1234_007");
        assert_eq!(render("{counter:5}", fields), "00007");
        assert_eq!(render("trip {dd}", fields), "trip 01");
    }
    #[test]
    fn grammar_errors_are_reported() {
        assert_eq!(Template::parse(""), Err(TemplateError::Empty));
        assert_eq!(Template::parse("{yyyy"), Err(TemplateError::Unbalanced));
        assert_eq!(Template::parse("yyyy}"), Err(TemplateError::Unbalanced));
        assert_eq!(Template::parse("{{yyyy}"), Err(TemplateError::Unbalanced));
        assert_eq!(
            Template::parse("{YYYY}"),
            Err(TemplateError::Unknown("YYYY".into()))
        );
        assert_eq!(
            Template::parse("{counter:0}"),
            Err(TemplateError::Unknown("counter:0".into()))
        );
        assert_eq!(
            Template::parse("{counter:12}"),
            Err(TemplateError::Unknown("counter:12".into()))
        );
        assert_eq!(Template::parse("a\tb"), Err(TemplateError::Control));
        assert_eq!(
            Template::parse(&"a".repeat(201)),
            Err(TemplateError::TooLong)
        );
    }
    #[test]
    fn token_names_are_case_sensitive() {
        let t = time();
        let fields = Fields {
            time: Some(&t),
            ..Fields::default()
        };
        assert_eq!(render("{MM}{mm}", fields), "0503");
    }
    #[test]
    fn empty_tokens_drop_their_separator() {
        let t = time().with_millis(None);
        let fields = Fields {
            time: Some(&t),
            ..Fields::default()
        };
        assert_eq!(
            render("{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}_{model}", fields),
            "2026-05-01_14-03-22"
        );
        assert_eq!(render("{ss}.{fff}", fields), "22");
        assert_eq!(render("{make} {model}", fields), "");
        assert_eq!(
            render(
                "{make} {model}",
                Fields {
                    model: Some(" iPhone 15 "),
                    ..Fields::default()
                }
            ),
            "iPhone 15"
        );
    }
    #[test]
    fn time_requirements_follow_tokens() {
        assert!(Template::parse(DEFAULT).unwrap().needs_time());
        assert!(
            !Template::parse("{original}_{counter}")
                .unwrap()
                .needs_time()
        );
        assert!(Template::parse("{fff}").unwrap().has_millis());
        assert!(Template::parse("{counter:2}").unwrap().has_counter());
    }
    #[test]
    fn sanitizing_makes_windows_names() {
        assert_eq!(sanitize_stem("a:b/c*d?", 255), "a_b_c_d_");
        assert_eq!(sanitize_stem("  trip. . ", 255), "trip");
        assert_eq!(sanitize_stem("...", 255), "media");
        assert_eq!(sanitize_stem("", 255), "media");
        assert_eq!(sanitize_stem("con", 255), "con_");
        assert_eq!(sanitize_stem("NUL.backup", 255), "NUL_.backup");
        assert_eq!(sanitize_stem("COM1", 255), "COM1_");
        assert_eq!(sanitize_stem("COM10", 255), "COM10");
        assert_eq!(sanitize_stem("console", 255), "console");
        assert_eq!(sanitize_stem(&"x".repeat(300), 250).len(), 250);
        let emoji = "😀".repeat(200);
        assert_eq!(units(&sanitize_stem(&emoji, 255)), 254);
    }
}
