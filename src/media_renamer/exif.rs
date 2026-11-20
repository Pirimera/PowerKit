//! Describes raw file metadata for the Details drawer: EXIF tag names, section grouping and
//! value formatting. Pure; the Windows layer enumerates the values and hands them over as
//! [`Raw`].

/// Most tags kept for one file, so a pathological file cannot flood the drawer.
pub const MAX_TAGS: usize = 1000;
/// Longest text value shown, in characters.
const MAX_TEXT: usize = 512;
/// Most list elements shown for one tag.
const MAX_LIST: usize = 32;
/// Most bytes of a binary value shown as hex.
const MAX_HEX: usize = 16;

/// Section label for values read from the Windows Property System (videos).
pub const PROPERTIES_SECTION: &str = "Properties";
/// Section labels the UI translates; the rest (EXIF, GPS, XMP, format blocks) are shown as is.
pub const IMAGE_SECTION: &str = "Image";
pub const THUMBNAIL_SECTION: &str = "Thumbnail";

/// One metadata value as the reader found it, before formatting.
#[derive(Clone, Debug, PartialEq)]
pub enum Raw {
    Text(String),
    Unsigned(u64),
    Signed(i64),
    /// EXIF RATIONAL: numerator, denominator.
    Rational(u32, u32),
    /// EXIF SRATIONAL: numerator, denominator.
    SignedRational(i32, i32),
    Float(f64),
    Bytes(Vec<u8>),
    List(Vec<Raw>),
}

/// A described tag: the section it belongs to, its name and its display value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExifTag {
    pub section: String,
    pub name: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Image,
    Exif,
    Gps,
    Interop,
    Thumbnail,
    Xmp,
    Other,
}
impl Block {
    fn label(self) -> &'static str {
        match self {
            Self::Image => IMAGE_SECTION,
            Self::Exif => "EXIF",
            Self::Gps => "GPS",
            Self::Interop => "Interop",
            Self::Thumbnail => THUMBNAIL_SECTION,
            Self::Xmp => "XMP",
            Self::Other => "Other",
        }
    }
}

const IMAGE_TAGS: &[(u32, &str)] = &[
    (254, "NewSubfileType"),
    (256, "ImageWidth"),
    (257, "ImageLength"),
    (258, "BitsPerSample"),
    (259, "Compression"),
    (262, "PhotometricInterpretation"),
    (270, "ImageDescription"),
    (271, "Make"),
    (272, "Model"),
    (273, "StripOffsets"),
    (274, "Orientation"),
    (277, "SamplesPerPixel"),
    (278, "RowsPerStrip"),
    (279, "StripByteCounts"),
    (282, "XResolution"),
    (283, "YResolution"),
    (284, "PlanarConfiguration"),
    (296, "ResolutionUnit"),
    (305, "Software"),
    (306, "DateTime"),
    (315, "Artist"),
    (318, "WhitePoint"),
    (319, "PrimaryChromaticities"),
    (513, "JPEGInterchangeFormat"),
    (514, "JPEGInterchangeFormatLength"),
    (529, "YCbCrCoefficients"),
    (530, "YCbCrSubSampling"),
    (531, "YCbCrPositioning"),
    (532, "ReferenceBlackWhite"),
    (33432, "Copyright"),
];
const EXIF_TAGS: &[(u32, &str)] = &[
    (33434, "ExposureTime"),
    (33437, "FNumber"),
    (34850, "ExposureProgram"),
    (34852, "SpectralSensitivity"),
    (34855, "ISOSpeedRatings"),
    (34856, "OECF"),
    (34864, "SensitivityType"),
    (34866, "RecommendedExposureIndex"),
    (36864, "ExifVersion"),
    (36867, "DateTimeOriginal"),
    (36868, "DateTimeDigitized"),
    (36880, "OffsetTime"),
    (36881, "OffsetTimeOriginal"),
    (36882, "OffsetTimeDigitized"),
    (37121, "ComponentsConfiguration"),
    (37122, "CompressedBitsPerPixel"),
    (37377, "ShutterSpeedValue"),
    (37378, "ApertureValue"),
    (37379, "BrightnessValue"),
    (37380, "ExposureBiasValue"),
    (37381, "MaxApertureValue"),
    (37382, "SubjectDistance"),
    (37383, "MeteringMode"),
    (37384, "LightSource"),
    (37385, "Flash"),
    (37386, "FocalLength"),
    (37396, "SubjectArea"),
    (37500, "MakerNote"),
    (37510, "UserComment"),
    (37520, "SubSecTime"),
    (37521, "SubSecTimeOriginal"),
    (37522, "SubSecTimeDigitized"),
    (40960, "FlashpixVersion"),
    (40961, "ColorSpace"),
    (40962, "PixelXDimension"),
    (40963, "PixelYDimension"),
    (41486, "FocalPlaneXResolution"),
    (41487, "FocalPlaneYResolution"),
    (41488, "FocalPlaneResolutionUnit"),
    (41492, "SubjectLocation"),
    (41493, "ExposureIndex"),
    (41495, "SensingMethod"),
    (41728, "FileSource"),
    (41729, "SceneType"),
    (41730, "CFAPattern"),
    (41985, "CustomRendered"),
    (41986, "ExposureMode"),
    (41987, "WhiteBalance"),
    (41988, "DigitalZoomRatio"),
    (41989, "FocalLengthIn35mmFilm"),
    (41990, "SceneCaptureType"),
    (41991, "GainControl"),
    (41992, "Contrast"),
    (41993, "Saturation"),
    (41994, "Sharpness"),
    (41995, "DeviceSettingDescription"),
    (41996, "SubjectDistanceRange"),
    (42016, "ImageUniqueID"),
    (42032, "CameraOwnerName"),
    (42033, "BodySerialNumber"),
    (42034, "LensSpecification"),
    (42035, "LensMake"),
    (42036, "LensModel"),
    (42037, "LensSerialNumber"),
];
const GPS_TAGS: &[(u32, &str)] = &[
    (0, "GPSVersionID"),
    (1, "GPSLatitudeRef"),
    (2, "GPSLatitude"),
    (3, "GPSLongitudeRef"),
    (4, "GPSLongitude"),
    (5, "GPSAltitudeRef"),
    (6, "GPSAltitude"),
    (7, "GPSTimeStamp"),
    (8, "GPSSatellites"),
    (9, "GPSStatus"),
    (10, "GPSMeasureMode"),
    (11, "GPSDOP"),
    (12, "GPSSpeedRef"),
    (13, "GPSSpeed"),
    (14, "GPSTrackRef"),
    (15, "GPSTrack"),
    (16, "GPSImgDirectionRef"),
    (17, "GPSImgDirection"),
    (18, "GPSMapDatum"),
    (19, "GPSDestLatitudeRef"),
    (20, "GPSDestLatitude"),
    (21, "GPSDestLongitudeRef"),
    (22, "GPSDestLongitude"),
    (23, "GPSDestBearingRef"),
    (24, "GPSDestBearing"),
    (25, "GPSDestDistanceRef"),
    (26, "GPSDestDistance"),
    (27, "GPSProcessingMethod"),
    (28, "GPSAreaInformation"),
    (29, "GPSDateStamp"),
    (30, "GPSDifferential"),
    (31, "GPSHPositioningError"),
];
const INTEROP_TAGS: &[(u32, &str)] =
    &[(1, "InteroperabilityIndex"), (2, "InteroperabilityVersion")];

fn tag_name(block: Block, id: u32) -> Option<&'static str> {
    let table = match block {
        Block::Image | Block::Thumbnail => IMAGE_TAGS,
        Block::Exif => EXIF_TAGS,
        Block::Gps => GPS_TAGS,
        Block::Interop => INTEROP_TAGS,
        Block::Xmp | Block::Other => return None,
    };
    table
        .iter()
        .find(|(tag, _)| *tag == id)
        .map(|(_, name)| *name)
}

/// The block a query path lives in, and a label for blocks this module does not know.
fn block_of(parents: &[&str]) -> (Block, Option<String>) {
    let is = |segment: &str, name: &str| segment.eq_ignore_ascii_case(name);
    if parents.iter().any(|segment| is(segment, "xmp")) {
        return (Block::Xmp, None);
    }
    if parents.iter().any(|segment| is(segment, "thumb")) {
        return (Block::Thumbnail, None);
    }
    for segment in parents.iter().rev() {
        let block = match segment.to_ascii_lowercase().as_str() {
            "gps" => Block::Gps,
            "exif" => Block::Exif,
            "interop" => Block::Interop,
            "ifd" => Block::Image,
            _ => continue,
        };
        return (block, None);
    }
    let label = parents
        .first()
        .map(|segment| segment.trim_matches(|c| c == '{' || c == '}').to_string())
        .filter(|label| !label.is_empty());
    (Block::Other, label)
}

/// `{ushort=271}` gives 271; other segments have no numeric id.
fn tag_id(segment: &str) -> Option<u32> {
    let inner = segment.strip_prefix('{')?.strip_suffix('}')?;
    let (kind, id) = inner.split_once('=')?;
    matches!(kind, "ushort" | "uint" | "short")
        .then(|| id.parse().ok())
        .flatten()
}

/// The readable part of a tag segment that has no known name: `{wstr=Author}` gives `Author`.
fn segment_text(segment: &str) -> String {
    match segment
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    {
        Some(inner) => inner.split_once('=').map_or(inner, |(_, text)| text).into(),
        None => segment.into(),
    }
}

/// Describes the value found at a WIC query `path` such as `/app1/ifd/exif/{ushort=36867}`.
/// Returns `None` for empty values and for paths without a tag segment.
pub fn describe(path: &str, raw: &Raw) -> Option<ExifTag> {
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let (tag, parents) = segments.split_last()?;
    let (block, label) = block_of(parents);
    let value = format_raw(raw)?;
    let name = tag_id(tag)
        .map(|id| tag_name(block, id).map_or_else(|| format!("Tag {id}"), str::to_string))
        .unwrap_or_else(|| segment_text(tag));
    Some(ExifTag {
        section: label.unwrap_or_else(|| block.label().to_string()),
        name,
        value,
    })
}

/// A tag read from the Windows Property System, which already has a display value.
pub fn property(name: &str, value: &str) -> Option<ExifTag> {
    let value = value.trim();
    (!value.is_empty()).then(|| ExifTag {
        section: PROPERTIES_SECTION.into(),
        name: name.strip_prefix("System.").unwrap_or(name).into(),
        value: truncate(value),
    })
}

fn section_rank(section: &str) -> u8 {
    match section {
        IMAGE_SECTION => 0,
        "EXIF" => 1,
        "GPS" => 2,
        "Interop" => 3,
        THUMBNAIL_SECTION => 4,
        "XMP" => 5,
        PROPERTIES_SECTION => 6,
        _ => 7,
    }
}

/// Orders sections the way an EXIF reader lists them, drops exact duplicates (the frame
/// and the container reader can report the same tag) and caps the count.
pub fn finish(tags: Vec<ExifTag>) -> Vec<ExifTag> {
    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<ExifTag> = tags
        .into_iter()
        .filter(|tag| seen.insert((tag.section.clone(), tag.name.clone(), tag.value.clone())))
        .collect();
    unique.sort_by_key(|tag| section_rank(&tag.section));
    unique.truncate(MAX_TAGS);
    unique
}

fn truncate(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(MAX_TEXT).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn clean(text: &str) -> String {
    truncate(text.trim_matches(|c: char| c == '\0' || c.is_whitespace()))
}

/// Up to four decimals, without trailing zeros: `0.5`, `2`, `0.0040`→`0.004`.
fn decimal(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn ratio(numerator: f64, denominator: f64, written: String) -> String {
    if denominator == 0.0 {
        written
    } else if (numerator / denominator).fract() == 0.0 {
        decimal(numerator / denominator)
    } else {
        format!("{written} ({})", decimal(numerator / denominator))
    }
}

fn printable(bytes: &[u8]) -> bool {
    let body = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    !body.is_empty() && body.iter().all(|b| b.is_ascii_graphic() || *b == b' ')
}

fn hex(bytes: &[u8]) -> String {
    let shown: Vec<String> = bytes
        .iter()
        .take(MAX_HEX)
        .map(|byte| format!("{byte:02X}"))
        .collect();
    if bytes.len() > MAX_HEX {
        format!("{} … ({} bytes)", shown.join(" "), bytes.len())
    } else {
        shown.join(" ")
    }
}

/// A list element on its own line of a tag value: rationals read as decimals.
fn format_element(raw: &Raw) -> String {
    match raw {
        Raw::Rational(n, d) if *d != 0 => decimal(f64::from(*n) / f64::from(*d)),
        Raw::SignedRational(n, d) if *d != 0 => decimal(f64::from(*n) / f64::from(*d)),
        other => format_raw(other).unwrap_or_default(),
    }
}

fn format_raw(raw: &Raw) -> Option<String> {
    let text = match raw {
        Raw::Text(text) => clean(text),
        Raw::Unsigned(value) => value.to_string(),
        Raw::Signed(value) => value.to_string(),
        Raw::Rational(n, d) => ratio(f64::from(*n), f64::from(*d), format!("{n}/{d}")),
        Raw::SignedRational(n, d) => ratio(f64::from(*n), f64::from(*d), format!("{n}/{d}")),
        Raw::Float(value) => decimal(*value),
        Raw::Bytes(bytes) if bytes.is_empty() => String::new(),
        Raw::Bytes(bytes) if printable(bytes) => clean(&String::from_utf8_lossy(bytes)),
        Raw::Bytes(bytes) => hex(bytes),
        Raw::List(items) => {
            let mut shown: Vec<String> = items.iter().take(MAX_LIST).map(format_element).collect();
            if items.len() > MAX_LIST {
                shown.push(format!("… ({} values)", items.len()));
            }
            shown.join(", ")
        }
    };
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(path: &str, raw: Raw) -> ExifTag {
        describe(path, &raw).expect("tag is described")
    }

    #[test]
    fn known_tags_get_their_names_and_sections() {
        let make = tag("/app1/ifd/{ushort=271}", Raw::Text("Contoso\0".into()));
        assert_eq!(
            make,
            ExifTag {
                section: "Image".into(),
                name: "Make".into(),
                value: "Contoso".into()
            }
        );
        let taken = tag(
            "/app1/ifd/exif/{ushort=36867}",
            Raw::Text("2024:05:01 10:00:00".into()),
        );
        assert_eq!(
            (taken.section.as_str(), taken.name.as_str()),
            ("EXIF", "DateTimeOriginal")
        );
        let lat = tag("/app1/ifd/gps/{ushort=2}", Raw::Unsigned(1));
        assert_eq!(
            (lat.section.as_str(), lat.name.as_str()),
            ("GPS", "GPSLatitude")
        );
        // The same id means different things in different blocks.
        let image_2 = tag("/app1/ifd/{ushort=2}", Raw::Unsigned(1));
        assert_eq!(image_2.name, "Tag 2");
    }

    #[test]
    fn unknown_ids_and_foreign_blocks_keep_a_readable_name() {
        assert_eq!(
            tag("/ifd/{ushort=60000}", Raw::Unsigned(7)).name,
            "Tag 60000"
        );
        let png = tag("/tEXt/{str=Comment}", Raw::Text("hello".into()));
        assert_eq!(
            (png.section.as_str(), png.name.as_str()),
            ("tEXt", "Comment")
        );
        let xmp = tag(
            "/xmp/{wstr=exif:DateTimeOriginal}",
            Raw::Text("2024-05-01".into()),
        );
        assert_eq!(
            (xmp.section.as_str(), xmp.name.as_str()),
            ("XMP", "exif:DateTimeOriginal")
        );
        let thumb = tag("/app1/thumb/{ushort=271}", Raw::Text("x".into()));
        assert_eq!(thumb.section, "Thumbnail");
    }

    #[test]
    fn empty_values_and_bare_paths_are_dropped() {
        assert_eq!(
            describe("/ifd/{ushort=271}", &Raw::Text(" \0".into())),
            None
        );
        assert_eq!(describe("/ifd/{ushort=271}", &Raw::Bytes(Vec::new())), None);
        assert_eq!(describe("/", &Raw::Unsigned(1)), None);
    }

    #[test]
    fn numbers_format_for_reading() {
        assert_eq!(format_raw(&Raw::Rational(1, 250)).unwrap(), "1/250 (0.004)");
        assert_eq!(format_raw(&Raw::Rational(28, 10)).unwrap(), "28/10 (2.8)");
        assert_eq!(format_raw(&Raw::Rational(72, 1)).unwrap(), "72");
        assert_eq!(format_raw(&Raw::Rational(5, 0)).unwrap(), "5/0");
        assert_eq!(
            format_raw(&Raw::SignedRational(-1, 3)).unwrap(),
            "-1/3 (-0.3333)"
        );
        assert_eq!(format_raw(&Raw::Signed(-4)).unwrap(), "-4");
        assert_eq!(format_raw(&Raw::Float(0.5)).unwrap(), "0.5");
    }

    #[test]
    fn lists_and_bytes_stay_short() {
        let coordinates = Raw::List(vec![
            Raw::Rational(52, 1),
            Raw::Rational(31, 1),
            Raw::Rational(125, 10),
        ]);
        assert_eq!(format_raw(&coordinates).unwrap(), "52, 31, 12.5");
        assert_eq!(format_raw(&Raw::Bytes(b"0230".to_vec())).unwrap(), "0230");
        let blob = Raw::Bytes((0..40).collect());
        let shown = format_raw(&blob).unwrap();
        assert!(shown.starts_with("00 01 02"), "{shown}");
        assert!(shown.ends_with("(40 bytes)"), "{shown}");
        let many = Raw::List((0..40).map(Raw::Unsigned).collect());
        assert!(format_raw(&many).unwrap().ends_with("… (40 values)"));
        let long = Raw::Text("a".repeat(MAX_TEXT + 10));
        assert_eq!(format_raw(&long).unwrap().chars().count(), MAX_TEXT + 1);
    }

    #[test]
    fn finish_orders_sections_and_drops_duplicates() {
        let make = |section: &str, name: &str| ExifTag {
            section: section.into(),
            name: name.into(),
            value: "v".into(),
        };
        let tags = finish(vec![
            make("XMP", "a"),
            make("GPS", "b"),
            make("EXIF", "c"),
            make("Image", "d"),
            make("EXIF", "c"),
            make("Image", "e"),
        ]);
        let order: Vec<&str> = tags.iter().map(|tag| tag.name.as_str()).collect();
        assert_eq!(order, ["d", "e", "c", "b", "a"]);
        let flood: Vec<ExifTag> = (0..MAX_TAGS + 50)
            .map(|index| make("EXIF", &format!("n{index}")))
            .collect();
        assert_eq!(finish(flood).len(), MAX_TAGS);
    }

    #[test]
    fn property_values_are_trimmed_and_named_without_the_namespace() {
        let duration = property("System.Media.Duration", " 0:00:12 ").unwrap();
        assert_eq!(duration.section, PROPERTIES_SECTION);
        assert_eq!(duration.name, "Media.Duration");
        assert_eq!(duration.value, "0:00:12");
        assert_eq!(property("System.Empty", "  "), None);
    }
}
