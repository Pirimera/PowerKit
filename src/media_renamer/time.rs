//! Capture times as local wall-clock values, parsed from EXIF and QuickTime text.

/// A local calendar time. EXIF stores camera wall-clock time; it is never converted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WallClock {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: Option<u16>,
}
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}
impl WallClock {
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Option<Self> {
        let valid = (1900..=2999).contains(&year)
            && (1..=12).contains(&month)
            && (1..=days_in_month(year, month)).contains(&day)
            && hour < 24
            && minute < 60
            && second < 60;
        valid.then_some(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            millis: None,
        })
    }
    pub fn with_millis(mut self, millis: Option<u16>) -> Self {
        self.millis = millis.filter(|value| *value < 1000);
        self
    }
    /// "2026-05-01 14:03:22" or "2026-05-01 14:03:22.457".
    pub fn display(&self) -> String {
        let base = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        );
        match self.millis {
            Some(millis) => format!("{base}.{millis:03}"),
            None => base,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeSource {
    ExifOriginal,
    ExifDigitized,
    QuickTimeLocal,
    QuickTimeUtc,
    /// Windows media property `System.Media.DateEncoded` (UTC), for AVI and as a
    /// fallback when the box reader finds nothing.
    Encoded,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    pub time: WallClock,
    /// Minutes east of UTC, when the file recorded one.
    pub offset: Option<i16>,
    pub source: TimeSource,
}

fn number(text: &str) -> Option<u16> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}
/// EXIF `YYYY:MM:DD HH:MM:SS`; blank and all-zero placeholders mean no date.
pub fn parse_exif(text: &str) -> Option<WallClock> {
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let (date, clock) = text.split_once([' ', 'T'])?;
    let date: Vec<&str> = date.split([':', '-']).collect();
    let clock: Vec<&str> = clock.trim().split(':').collect();
    if date.len() != 3 || clock.len() != 3 {
        return None;
    }
    let (second, fraction) = match clock[2].split_once('.') {
        Some((second, fraction)) => (second, Some(fraction)),
        None => (clock[2], None),
    };
    WallClock::new(
        number(date[0])?,
        u8::try_from(number(date[1])?).ok()?,
        u8::try_from(number(date[2])?).ok()?,
        u8::try_from(number(clock[0])?).ok()?,
        u8::try_from(number(clock[1])?).ok()?,
        u8::try_from(number(second)?).ok()?,
    )
    .map(|time| time.with_millis(fraction.and_then(parse_subsec)))
}
/// EXIF `SubSecTime*` digits as milliseconds: "4" is 400, "4571" is 457.
pub fn parse_subsec(text: &str) -> Option<u16> {
    let digits = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut padded: String = digits.chars().take(3).collect();
    while padded.len() < 3 {
        padded.push('0');
    }
    padded.parse().ok()
}
/// `+03:00`, `-0530`, `+03` or `Z`, as minutes east of UTC.
pub fn parse_offset(text: &str) -> Option<i16> {
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    if text == "Z" {
        return Some(0);
    }
    let sign = match text.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let rest = text[1..].replace(':', "");
    let (hours, minutes) = match rest.len() {
        2 => (number(&rest)?, 0),
        4 => (number(&rest[..2])?, number(&rest[2..])?),
        _ => return None,
    };
    (hours <= 14 && minutes < 60).then(|| sign * (hours * 60 + minutes) as i16)
}
pub fn format_offset(minutes: i16) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let minutes = minutes.unsigned_abs();
    format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}
/// ISO 8601 local time with an optional offset, as Apple's
/// `com.apple.quicktime.creationdate` writes it (`2026-05-01T14:03:22+0300`).
pub fn parse_iso(text: &str) -> Option<(WallClock, Option<i16>)> {
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let t = text.find('T')?;
    let tail = &text[t + 1..];
    let zone = tail.find(['+', '-', 'Z']).map(|index| t + 1 + index);
    let (local, offset) = match zone {
        Some(index) => (&text[..index], Some(parse_offset(&text[index..])?)),
        None => (text, None),
    };
    Some((parse_exif(local)?, offset))
}

/// Seconds between 1904-01-01 (QuickTime epoch) and 1970-01-01.
const QUICKTIME_TO_UNIX: u64 = 2_082_844_800;
/// `mvhd` creation seconds as Unix seconds; zero and pre-1970 values are missing.
pub fn quicktime_to_unix(seconds: u64) -> Option<i64> {
    seconds
        .checked_sub(QUICKTIME_TO_UNIX)
        .filter(|unix| *unix > 0)
        .and_then(|unix| i64::try_from(unix).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exif_dates_parse_and_reject_placeholders() {
        let time = parse_exif("2026:05:01 14:03:22\0").unwrap();
        assert_eq!(time.display(), "2026-05-01 14:03:22");
        assert_eq!(parse_exif("0000:00:00 00:00:00"), None);
        assert_eq!(parse_exif("    :  :     :  :  "), None);
        assert_eq!(parse_exif("2026:02:30 10:00:00"), None);
        assert_eq!(parse_exif("2024:02:29 23:59:59").unwrap().day, 29);
        assert_eq!(parse_exif("2026:05:01 24:00:00"), None);
        assert_eq!(parse_exif("garbage"), None);
    }
    #[test]
    fn subseconds_pad_and_truncate() {
        assert_eq!(parse_subsec("4"), Some(400));
        assert_eq!(parse_subsec("45"), Some(450));
        assert_eq!(parse_subsec("457\0"), Some(457));
        assert_eq!(parse_subsec("4571"), Some(457));
        assert_eq!(parse_subsec(""), None);
        assert_eq!(parse_subsec("4a"), None);
    }
    #[test]
    fn offsets_parse_both_shapes() {
        assert_eq!(parse_offset("+03:00"), Some(180));
        assert_eq!(parse_offset("-0530"), Some(-330));
        assert_eq!(parse_offset("+09"), Some(540));
        assert_eq!(parse_offset("Z"), Some(0));
        assert_eq!(parse_offset("03:00"), None);
        assert_eq!(parse_offset("+15:00"), None);
        assert_eq!(format_offset(-330), "-05:30");
        assert_eq!(format_offset(180), "+03:00");
    }
    #[test]
    fn apple_creation_dates_keep_local_time() {
        let (time, offset) = parse_iso("2026-05-01T14:03:22+0300").unwrap();
        assert_eq!(time.display(), "2026-05-01 14:03:22");
        assert_eq!(offset, Some(180));
        let (time, offset) = parse_iso("2026-05-01T14:03:22.25-07:00").unwrap();
        assert_eq!(time.millis, Some(250));
        assert_eq!(offset, Some(-420));
        let (_, offset) = parse_iso("2026-05-01T14:03:22Z").unwrap();
        assert_eq!(offset, Some(0));
        assert_eq!(parse_iso("2026-05-01T14:03:22").unwrap().1, None);
        assert_eq!(parse_iso("2026-05-01 14:03:22"), None);
    }
    #[test]
    fn quicktime_epoch_rejects_zero_and_old_values() {
        assert_eq!(quicktime_to_unix(0), None);
        assert_eq!(quicktime_to_unix(QUICKTIME_TO_UNIX), None);
        assert_eq!(quicktime_to_unix(QUICKTIME_TO_UNIX + 60), Some(60));
    }
}
