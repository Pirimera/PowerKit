//! Minimal ISO BMFF (MP4/MOV) reader: `moov/mvhd` creation time and Apple `moov/meta` keys.
//! Walks box headers by seeking, so large `mdat` payloads are never read.
use std::io::{Read, Seek, SeekFrom};

const MAX_BOXES: usize = 4096;
const MAX_META: u64 = 1 << 20;
const MAX_KEYS: u32 = 1024;
const MAX_VALUE: usize = 1024;
const APPLE: &str = "com.apple.quicktime.";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VideoMeta {
    /// `mvhd` creation time, seconds since 1904-01-01 UTC.
    pub created: Option<u64>,
    /// Apple `creationdate`: local ISO 8601 time with offset.
    pub apple_created: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Malformed;
impl From<std::io::Error> for Malformed {
    fn from(_: std::io::Error) -> Self {
        Self
    }
}

struct Header {
    kind: [u8; 4],
    start: u64,
    end: u64,
}
fn read_u32<R: Read>(reader: &mut R) -> Result<u32, Malformed> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}
fn header<R: Read + Seek>(
    reader: &mut R,
    at: u64,
    limit: u64,
) -> Result<Option<Header>, Malformed> {
    if at >= limit {
        return Ok(None);
    }
    if limit - at < 8 {
        return Err(Malformed);
    }
    reader.seek(SeekFrom::Start(at))?;
    let size = read_u32(reader)?;
    let mut kind = [0u8; 4];
    reader.read_exact(&mut kind)?;
    let (header_len, size) = match size {
        0 => (8, limit - at),
        1 => {
            if limit - at < 16 {
                return Err(Malformed);
            }
            let mut large = [0u8; 8];
            reader.read_exact(&mut large)?;
            (16, u64::from_be_bytes(large))
        }
        size => (8, u64::from(size)),
    };
    let end = at.checked_add(size).ok_or(Malformed)?;
    if size < header_len || end > limit {
        return Err(Malformed);
    }
    Ok(Some(Header {
        kind,
        start: at + header_len,
        end,
    }))
}
fn plausible(kind: [u8; 4]) -> bool {
    kind.iter().all(|b| b.is_ascii_alphanumeric() || *b == b' ')
}

pub fn read<R: Read + Seek>(reader: &mut R) -> Result<VideoMeta, Malformed> {
    let len = reader.seek(SeekFrom::End(0))?;
    let mut at = 0;
    for count in 0..MAX_BOXES {
        let Some(top) = header(reader, at, len)? else {
            break;
        };
        if count == 0 && !plausible(top.kind) {
            return Err(Malformed);
        }
        if &top.kind == b"moov" {
            return read_moov(reader, &top);
        }
        at = top.end;
    }
    Err(Malformed)
}
fn read_moov<R: Read + Seek>(reader: &mut R, moov: &Header) -> Result<VideoMeta, Malformed> {
    let mut meta = VideoMeta::default();
    let mut at = moov.start;
    for _ in 0..MAX_BOXES {
        let Some(child) = header(reader, at, moov.end)? else {
            break;
        };
        let len = child.end - child.start;
        match &child.kind {
            b"mvhd" => {
                let mut bytes = vec![0u8; len.min(16) as usize];
                reader.seek(SeekFrom::Start(child.start))?;
                reader.read_exact(&mut bytes)?;
                meta.created = mvhd_created(&bytes);
            }
            b"meta" if len <= MAX_META => {
                let mut bytes = vec![0u8; len as usize];
                reader.seek(SeekFrom::Start(child.start))?;
                reader.read_exact(&mut bytes)?;
                // A damaged Apple meta box does not hide the mvhd time.
                let _ = read_meta(&bytes, &mut meta);
            }
            _ => {}
        }
        at = child.end;
    }
    Ok(meta)
}
fn mvhd_created(bytes: &[u8]) -> Option<u64> {
    let seconds = match bytes.first()? {
        0 => u64::from(u32::from_be_bytes(bytes.get(4..8)?.try_into().ok()?)),
        1 => u64::from_be_bytes(bytes.get(4..12)?.try_into().ok()?),
        _ => return None,
    };
    (seconds != 0).then_some(seconds)
}

type Child<'a> = ([u8; 4], &'a [u8]);

/// Child boxes of a byte slice as `(kind, payload)`.
fn boxes(bytes: &[u8]) -> Result<Vec<Child<'_>>, Malformed> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        if out.len() >= MAX_BOXES || bytes.len() - at < 8 {
            return Err(Malformed);
        }
        let size = u32::from_be_bytes(bytes[at..at + 4].try_into().map_err(|_| Malformed)?);
        let kind: [u8; 4] = bytes[at + 4..at + 8].try_into().map_err(|_| Malformed)?;
        let (header_len, size) = match size {
            0 => (8, bytes.len() - at),
            1 => {
                let large = bytes.get(at + 8..at + 16).ok_or(Malformed)?;
                let large = u64::from_be_bytes(large.try_into().map_err(|_| Malformed)?);
                (16, usize::try_from(large).map_err(|_| Malformed)?)
            }
            size => (8, size as usize),
        };
        let end = at.checked_add(size).ok_or(Malformed)?;
        if size < header_len || end > bytes.len() {
            return Err(Malformed);
        }
        out.push((kind, &bytes[at + header_len..end]));
        at = end;
    }
    Ok(out)
}
fn read_meta(bytes: &[u8], meta: &mut VideoMeta) -> Result<(), Malformed> {
    // QuickTime `meta` has no version/flags; the ISO full-box form does.
    let body = if bytes.get(4..8) == Some(b"hdlr") {
        bytes
    } else {
        bytes.get(4..).ok_or(Malformed)?
    };
    let children = boxes(body)?;
    let keys = children
        .iter()
        .find(|(kind, _)| kind == b"keys")
        .map(|(_, payload)| parse_keys(payload))
        .transpose()?
        .unwrap_or_default();
    let Some((_, ilst)) = children.iter().find(|(kind, _)| kind == b"ilst") else {
        return Ok(());
    };
    for (kind, item) in boxes(ilst)? {
        let index = u32::from_be_bytes(kind) as usize;
        let Some(name) = index.checked_sub(1).and_then(|i| keys.get(i)) else {
            continue;
        };
        let Some(field) = name.strip_prefix(APPLE) else {
            continue;
        };
        let slot = match field {
            "creationdate" => &mut meta.apple_created,
            "make" => &mut meta.make,
            "model" => &mut meta.model,
            _ => continue,
        };
        if let Some(value) = data_text(item)? {
            *slot = Some(value);
        }
    }
    Ok(())
}
fn parse_keys(payload: &[u8]) -> Result<Vec<String>, Malformed> {
    let count = u32::from_be_bytes(
        payload
            .get(4..8)
            .ok_or(Malformed)?
            .try_into()
            .map_err(|_| Malformed)?,
    );
    if count > MAX_KEYS {
        return Err(Malformed);
    }
    let mut keys = Vec::with_capacity(count as usize);
    let mut at = 8usize;
    for _ in 0..count {
        let size = u32::from_be_bytes(
            payload
                .get(at..at + 4)
                .ok_or(Malformed)?
                .try_into()
                .map_err(|_| Malformed)?,
        ) as usize;
        let end = at.checked_add(size).ok_or(Malformed)?;
        if size < 8 || end > payload.len() {
            return Err(Malformed);
        }
        keys.push(String::from_utf8_lossy(&payload[at + 8..end]).into_owned());
        at = end;
    }
    Ok(keys)
}
/// UTF-8 (type 1) or UTF-16 (type 2) text of an `ilst` item's `data` box.
fn data_text(item: &[u8]) -> Result<Option<String>, Malformed> {
    for (kind, payload) in boxes(item)? {
        if &kind != b"data" || payload.len() < 8 {
            continue;
        }
        let kind =
            u32::from_be_bytes(payload[..4].try_into().map_err(|_| Malformed)?) & 0x00ff_ffff;
        let value = &payload[8..payload.len().min(8 + MAX_VALUE)];
        let text = match kind {
            1 => String::from_utf8_lossy(value).into_owned(),
            2 => String::from_utf16_lossy(
                &value
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect::<Vec<_>>(),
            ),
            _ => continue,
        };
        let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
        return Ok((!text.is_empty()).then(|| text.to_string()));
    }
    Ok(None)
}

#[cfg(test)]
pub(crate) mod fixtures {
    /// A box with a 32-bit size.
    pub fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }
    pub fn mvhd(seconds: u32) -> Vec<u8> {
        let mut payload = vec![0u8; 4];
        payload.extend_from_slice(&seconds.to_be_bytes());
        payload.extend_from_slice(&seconds.to_be_bytes());
        payload.extend_from_slice(&[0u8; 88]);
        bx(b"mvhd", &payload)
    }
    pub fn apple_meta(items: &[(&str, &str)]) -> Vec<u8> {
        let mut keys = vec![0u8; 4];
        keys.extend_from_slice(&(items.len() as u32).to_be_bytes());
        let mut ilst = Vec::new();
        for (index, (name, value)) in items.iter().enumerate() {
            let key = format!("com.apple.quicktime.{name}");
            keys.extend_from_slice(&((key.len() + 8) as u32).to_be_bytes());
            keys.extend_from_slice(b"mdta");
            keys.extend_from_slice(key.as_bytes());
            let mut data = 1u32.to_be_bytes().to_vec();
            data.extend_from_slice(&[0u8; 4]);
            data.extend_from_slice(value.as_bytes());
            ilst.extend(bx(&((index + 1) as u32).to_be_bytes(), &bx(b"data", &data)));
        }
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"mdta");
        hdlr.extend_from_slice(&[0u8; 13]);
        let mut body = bx(b"hdlr", &hdlr);
        body.extend(bx(b"keys", &keys));
        body.extend(bx(b"ilst", &ilst));
        bx(b"meta", &body)
    }
    pub fn movie(moov_children: &[Vec<u8>]) -> Vec<u8> {
        let mut file = bx(b"ftyp", b"qt  \0\0\0\0qt  ");
        file.extend(bx(b"mdat", &[0u8; 32]));
        file.extend(bx(b"moov", &moov_children.concat()));
        file
    }
}

#[cfg(test)]
mod tests {
    use super::{fixtures::*, *};
    use std::io::Cursor;
    fn parse(bytes: Vec<u8>) -> Result<VideoMeta, Malformed> {
        read(&mut Cursor::new(bytes))
    }
    #[test]
    fn apple_keys_and_mvhd_are_read() {
        let meta = parse(movie(&[
            mvhd(3_900_000_000),
            apple_meta(&[
                ("make", "Apple"),
                ("model", "iPhone 15 Pro"),
                ("software", "17.4"),
                ("creationdate", "2026-05-01T14:03:22+0300"),
            ]),
        ]))
        .unwrap();
        assert_eq!(meta.created, Some(3_900_000_000));
        assert_eq!(
            meta.apple_created.as_deref(),
            Some("2026-05-01T14:03:22+0300")
        );
        assert_eq!(meta.make.as_deref(), Some("Apple"));
        assert_eq!(meta.model.as_deref(), Some("iPhone 15 Pro"));
    }
    #[test]
    fn version_one_mvhd_and_large_boxes_are_followed() {
        let mut mvhd = vec![1u8, 0, 0, 0];
        mvhd.extend_from_slice(&3_900_000_123u64.to_be_bytes());
        mvhd.extend_from_slice(&[0u8; 100]);
        let moov = bx(b"moov", &bx(b"mvhd", &mvhd));
        let mut file = bx(b"ftyp", b"isom\0\0\0\0");
        file.extend_from_slice(&1u32.to_be_bytes());
        file.extend_from_slice(b"mdat");
        file.extend_from_slice(&(16u64 + 4).to_be_bytes());
        file.extend_from_slice(&[0u8; 4]);
        file.extend(moov);
        assert_eq!(parse(file).unwrap().created, Some(3_900_000_123));
    }
    #[test]
    fn iso_full_box_meta_is_accepted() {
        let apple = apple_meta(&[("creationdate", "2026-05-01T14:03:22Z")]);
        let mut full = 0u32.to_be_bytes().to_vec();
        full.extend_from_slice(&apple[8..]);
        let meta = parse(movie(&[bx(b"meta", &full)])).unwrap();
        assert_eq!(meta.apple_created.as_deref(), Some("2026-05-01T14:03:22Z"));
        assert_eq!(meta.created, None);
    }
    #[test]
    fn malformed_input_is_rejected() {
        assert_eq!(parse(Vec::new()), Err(Malformed));
        assert_eq!(
            parse(b"\0\0\0\x08\xff\xfe\xfd\xfc".to_vec()),
            Err(Malformed)
        );
        let mut truncated = movie(&[mvhd(3_900_000_000)]);
        truncated.truncate(truncated.len() - 10);
        assert_eq!(parse(truncated), Err(Malformed));
        let mut oversized = bx(b"ftyp", b"isom");
        oversized.extend_from_slice(&u32::MAX.to_be_bytes());
        oversized.extend_from_slice(b"moov");
        assert_eq!(parse(oversized), Err(Malformed));
        let mut tiny = bx(b"ftyp", b"isom");
        tiny.extend_from_slice(&4u32.to_be_bytes());
        tiny.extend_from_slice(b"moov");
        assert_eq!(parse(tiny), Err(Malformed));
        assert_eq!(parse(bx(b"ftyp", b"isom")), Err(Malformed));
    }
    #[test]
    fn damaged_meta_keeps_mvhd_and_zero_is_missing() {
        let meta = apple_meta(&[("make", "Apple")]);
        let broken = bx(b"meta", &[0, 0, 0, 0, 0, 0, 0, 99, b'k', b'e', b'y', b's']);
        let parsed = parse(movie(&[mvhd(3_900_000_000), broken, meta])).unwrap();
        assert_eq!(parsed.created, Some(3_900_000_000));
        assert_eq!(parsed.make.as_deref(), Some("Apple"));
        assert_eq!(parse(movie(&[mvhd(0)])).unwrap().created, None);
    }
}
