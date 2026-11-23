//! Read-only capture metadata: WIC query readers for images, the ISO BMFF box reader for
//! MP4/MOV and the Windows Property System as a fallback. Never writes to the file.
//!
//! A `Prober` owns COM and the WIC factory for the worker thread that created it.
use super::super::clock;
use super::wide_path;
use powerkit_core::media_renamer::{
    Kind, bmff,
    exif::{self, ExifTag, Raw},
    plan::{Metadata, Probe},
    time::{self, Capture, TimeSource},
};
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, PROPERTYKEY, WINCODEC_ERR_COMPONENTNOTFOUND},
        Graphics::Imaging::{
            CLSID_WICImagingFactory, IWICBitmapDecoder, IWICImagingFactory,
            IWICMetadataQueryReader, WICDecodeMetadataCacheOnDemand,
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
                StructuredStorage::{PROPVARIANT, PropVariantClear},
            },
            Variant::{
                VARENUM, VT_BLOB, VT_BSTR, VT_FILETIME, VT_I1, VT_I2, VT_I4, VT_I8, VT_LPSTR,
                VT_LPWSTR, VT_R4, VT_R8, VT_UI1, VT_UI2, VT_UI4, VT_UI8, VT_UNKNOWN, VT_VECTOR,
            },
        },
        UI::Shell::PropertiesSystem::{
            GPS_DEFAULT, IPropertyStore, PDFF_DEFAULT, PSFormatForDisplayAlloc,
            PSGetNameFromPropertyKey, SHGetPropertyStoreFromParsingName,
        },
    },
    core::{GUID, Interface, PCWSTR, PWSTR},
};

/// `System.Media.DateEncoded`.
const DATE_ENCODED: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x2e4b640d_5019_46d8_8881_55414cc5caa0),
    pid: 100,
};
/// EXIF IFD containers, by format: JPEG APP1, TIFF-based RAW/HEIF, PNG/WebP chunks.
const IFD_ROOTS: [&str; 4] = ["/app1/ifd", "/ifd", "/exif", "/eXIf"];
const XMP_ORIGINAL: [&str; 2] = ["/xmp/exif:DateTimeOriginal", "/xmp/xmp:CreateDate"];

struct ComGuard(bool);
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful CoInitializeEx on this thread.
            unsafe { CoUninitialize() };
        }
    }
}
pub(super) struct Prober {
    // Declaration order is drop order: the factory is released before CoUninitialize.
    factory: Option<IWICImagingFactory>,
    _com: ComGuard,
}
impl Prober {
    /// Initializes COM for the calling worker thread. Only that thread may use the prober.
    pub(super) fn new() -> Self {
        // SAFETY: called on a dedicated worker with no other COM users.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let com = ComGuard(hr.0 == 0);
        // SAFETY: the factory stays on this thread and is released before COM uninitializes.
        let factory = hr
            .is_ok()
            .then(|| unsafe {
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()
            })
            .flatten();
        Self { factory, _com: com }
    }
    pub(super) fn probe(&self, path: &Path, kind: Kind) -> Probe {
        match kind {
            Kind::Sidecar => Probe::Read(Metadata::default()),
            Kind::Video => match video(path) {
                Some(boxes) if boxes.capture.is_some() => Probe::Read(boxes),
                Some(boxes) => Probe::Read(Metadata {
                    capture: encoded(path).and_then(|metadata| metadata.capture),
                    ..boxes
                }),
                None => encoded(path).map_or(Probe::Unreadable, Probe::Read),
            },
            Kind::Avi => encoded(path).map_or(Probe::Unreadable, Probe::Read),
            _ => self.image(path, kind),
        }
    }
    fn image(&self, path: &Path, kind: Kind) -> Probe {
        let Some(factory) = &self.factory else {
            return Probe::Unreadable;
        };
        let wide = wide_path(path);
        // SAFETY: `wide` is a live NUL-terminated path; the decoder is released on return.
        let decoder = unsafe {
            factory.CreateDecoderFromFilename(
                PCWSTR(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
        };
        let decoder = match decoder {
            Ok(decoder) => decoder,
            Err(error) if error.code() == WINCODEC_ERR_COMPONENTNOTFOUND => {
                return kind.codec().map_or(Probe::Unreadable, Probe::CodecMissing);
            }
            Err(_) => return Probe::Unreadable,
        };
        Probe::Read(read_image(&readers_of(&decoder)))
    }
    /// Every metadata value of `path`, for the Details drawer. Never writes. Online-only
    /// files must not be passed: reading them downloads them.
    pub(super) fn dump(&self, path: &Path, kind: Kind) -> Vec<ExifTag> {
        let tags = match kind {
            Kind::Sidecar => Vec::new(),
            Kind::Video | Kind::Avi => properties(path),
            _ => self.image_tags(path),
        };
        exif::finish(tags)
    }
    fn image_tags(&self, path: &Path) -> Vec<ExifTag> {
        let Some(factory) = &self.factory else {
            return Vec::new();
        };
        let wide = wide_path(path);
        // SAFETY: `wide` is a live NUL-terminated path; the decoder is released on return.
        let decoder = unsafe {
            factory.CreateDecoderFromFilename(
                PCWSTR(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
        };
        let Ok(decoder) = decoder else {
            return Vec::new();
        };
        let mut tags = Vec::new();
        let mut budget = MAX_ITEMS;
        for reader in readers_of(&decoder) {
            // WIC flattens sub-IFDs when a parent block is enumerated, which loses which
            // block a tag came from. Ask for each known block by path instead.
            for block in KNOWN_BLOCKS {
                let Some(child) = block_reader(&reader, block) else {
                    continue;
                };
                walk(&child, block, MAX_DEPTH, &mut budget, &mut tags, false);
            }
            walk(&reader, "", 0, &mut budget, &mut tags, true);
        }
        tags
    }
}

/// Metadata blocks asked for by path so each tag keeps its block: JPEG and TIFF-style
/// EXIF, then XMP.
const KNOWN_BLOCKS: [&str; 11] = [
    "/app1/ifd",
    "/app1/ifd/exif",
    "/app1/ifd/exif/interop",
    "/app1/ifd/gps",
    "/app1/ifd/thumb",
    "/ifd",
    "/ifd/exif",
    "/ifd/exif/interop",
    "/ifd/gps",
    "/ifd/thumb",
    "/xmp",
];
/// Root blocks left out of the root enumeration: the ones `KNOWN_BLOCKS` already covered
/// (their flattened tags would be listed a second time under the wrong block) and JPEG
/// quantization tables, which are bulk numbers rather than information about the photo.
const SKIPPED_ROOTS: [&str; 5] = ["/app1", "/ifd", "/xmp", "/luminance", "/chrominance"];

/// The nested metadata block at `path`, if the file has one.
fn block_reader(reader: &IWICMetadataQueryReader, path: &str) -> Option<IWICMetadataQueryReader> {
    let query: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let mut value = PROPVARIANT::default();
    // SAFETY: the query name is live; the variant is cleared before returning, and the
    // child reader taken from it is its own reference.
    unsafe {
        let found = reader
            .GetMetadataByName(PCWSTR(query.as_ptr()), &mut value)
            .ok()
            .and_then(|()| variant_reader(&value));
        let _ = PropVariantClear(&mut value);
        found
    }
}
/// The frame's and the container's metadata readers, when the decoder has them.
fn readers_of(decoder: &IWICBitmapDecoder) -> Vec<IWICMetadataQueryReader> {
    // SAFETY: decoder and frame are owned COM pointers used only on this thread.
    unsafe {
        [
            decoder
                .GetFrame(0)
                .and_then(|frame| frame.GetMetadataQueryReader())
                .ok(),
            decoder.GetMetadataQueryReader().ok(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Deepest metadata block followed, and most items read from one file.
const MAX_DEPTH: usize = 6;
const MAX_ITEMS: usize = 4000;
/// Most elements read from one vector value.
const MAX_ELEMENTS: usize = 4096;

/// Enumerates `reader` and the blocks below it, describing every value found. `prefix` is
/// the query path of `reader`; `budget` counts the items still allowed.
fn walk(
    reader: &IWICMetadataQueryReader,
    prefix: &str,
    depth: usize,
    budget: &mut usize,
    out: &mut Vec<ExifTag>,
    skip_roots: bool,
) {
    // SAFETY: the reader is a live COM pointer owned by the calling worker thread.
    let Ok(names) = (unsafe { reader.GetEnumerator() }) else {
        return;
    };
    while *budget > 0 {
        let mut slot = [PWSTR::null()];
        let mut fetched = 0u32;
        // SAFETY: `slot` and `fetched` outlive the call; one name is requested.
        let result = unsafe { names.Next(&mut slot, Some(&mut fetched)) };
        if result.0 != 0 || fetched == 0 {
            break;
        }
        // SAFETY: a successful Next fills the slot with a CoTaskMem string that this
        // function owns and frees once, after copying it.
        let name = unsafe {
            let name = slot[0].to_string().ok();
            CoTaskMemFree(Some(slot[0].0.cast_const().cast()));
            name
        };
        let Some(name) = name else {
            continue;
        };
        if skip_roots
            && depth == 0
            && SKIPPED_ROOTS
                .iter()
                .any(|root| name.eq_ignore_ascii_case(root))
        {
            continue;
        }
        *budget -= 1;
        let path = format!("{prefix}{name}");
        let query: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut value = PROPVARIANT::default();
        // SAFETY: the query name is live; the variant is read and cleared before the next
        // loop turn, and a child reader is its own reference, so clearing is safe.
        unsafe {
            if reader
                .GetMetadataByName(PCWSTR(query.as_ptr()), &mut value)
                .is_ok()
            {
                if let Some(child) = variant_reader(&value) {
                    if depth < MAX_DEPTH {
                        walk(&child, &path, depth + 1, budget, out, skip_roots);
                    }
                } else if let Some(raw) = variant_raw(&value) {
                    out.extend(exif::describe(&path, &raw));
                }
            }
            let _ = PropVariantClear(&mut value);
        }
    }
}

/// The nested metadata block a variant holds, when it holds one.
///
/// # Safety
/// `value` must be a PROPVARIANT filled by a COM call and not yet cleared.
unsafe fn variant_reader(value: &PROPVARIANT) -> Option<IWICMetadataQueryReader> {
    // SAFETY: the tag selects the valid union member; the interface is queried, not taken.
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        if inner.vt != VT_UNKNOWN {
            return None;
        }
        inner
            .Anonymous
            .punkVal
            .as_ref()?
            .cast::<IWICMetadataQueryReader>()
            .ok()
    }
}

/// A scalar or vector metadata value as [`Raw`]. WIC reports EXIF RATIONAL as `VT_UI8`
/// (numerator in the low 32 bits) and SRATIONAL as `VT_I8`.
///
/// # Safety
/// `value` must be a PROPVARIANT filled by a COM call and not yet cleared.
unsafe fn variant_raw(value: &PROPVARIANT) -> Option<Raw> {
    // SAFETY: each union member is read only when the tag says it is valid, and vector
    // lengths are capped before slices are formed.
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        let data = &inner.Anonymous;
        let vt = inner.vt;
        if vt.0 & VT_VECTOR.0 != 0 {
            return match VARENUM(vt.0 & !VT_VECTOR.0) {
                VT_UI1 => Some(Raw::Bytes(
                    elements(data.caub.pElems, data.caub.cElems).to_vec(),
                )),
                VT_UI2 => Some(list(elements(data.caui.pElems, data.caui.cElems), |v| {
                    Raw::Unsigned(u64::from(*v))
                })),
                VT_I2 => Some(list(elements(data.cai.pElems, data.cai.cElems), |v| {
                    Raw::Signed(i64::from(*v))
                })),
                VT_UI4 => Some(list(elements(data.caul.pElems, data.caul.cElems), |v| {
                    Raw::Unsigned(u64::from(*v))
                })),
                VT_I4 => Some(list(elements(data.cal.pElems, data.cal.cElems), |v| {
                    Raw::Signed(i64::from(*v))
                })),
                VT_UI8 => Some(list(elements(data.cauh.pElems, data.cauh.cElems), |v| {
                    rational(*v)
                })),
                VT_I8 => Some(list(elements(data.cah.pElems, data.cah.cElems), |v| {
                    signed_rational(*v)
                })),
                _ => None,
            };
        }
        match vt {
            VT_LPSTR | VT_LPWSTR | VT_BSTR => variant_text(value).map(Raw::Text),
            VT_UI1 => Some(Raw::Unsigned(u64::from(data.bVal))),
            VT_I1 => Some(Raw::Signed(i64::from(data.cVal))),
            VT_UI2 => Some(Raw::Unsigned(u64::from(data.uiVal))),
            VT_I2 => Some(Raw::Signed(i64::from(data.iVal))),
            VT_UI4 => Some(Raw::Unsigned(u64::from(data.ulVal))),
            VT_I4 => Some(Raw::Signed(i64::from(data.lVal))),
            VT_UI8 => Some(rational(data.uhVal)),
            VT_I8 => Some(signed_rational(data.hVal)),
            VT_R4 => Some(Raw::Float(f64::from(data.fltVal))),
            VT_R8 => Some(Raw::Float(data.dblVal)),
            VT_BLOB => Some(Raw::Bytes(
                elements(data.blob.pBlobData, data.blob.cbSize).to_vec(),
            )),
            _ => None,
        }
    }
}

/// # Safety
/// `pointer` must be null or point to `count` readable elements that outlive the borrow.
unsafe fn elements<'a, T>(pointer: *const T, count: u32) -> &'a [T] {
    if pointer.is_null() {
        return &[];
    }
    // SAFETY: guaranteed by the caller; the count is capped.
    unsafe { std::slice::from_raw_parts(pointer, (count as usize).min(MAX_ELEMENTS)) }
}
fn list<T>(items: &[T], convert: impl Fn(&T) -> Raw) -> Raw {
    Raw::List(items.iter().map(convert).collect())
}
fn rational(packed: u64) -> Raw {
    Raw::Rational(packed as u32, (packed >> 32) as u32)
}
fn signed_rational(packed: i64) -> Raw {
    Raw::SignedRational(packed as i32, (packed >> 32) as i32)
}

/// Windows Property System values (duration, frame size, bit rate...) in display form,
/// for formats that carry no EXIF block.
fn properties(path: &Path) -> Vec<ExifTag> {
    let wide = wide_path(path);
    let mut tags = Vec::new();
    // SAFETY: `wide` is live for the call; every string and variant is released here.
    unsafe {
        let store: Result<IPropertyStore, _> =
            SHGetPropertyStoreFromParsingName(PCWSTR(wide.as_ptr()), None, GPS_DEFAULT);
        let Ok(store) = store else {
            return tags;
        };
        let count = store.GetCount().unwrap_or(0).min(MAX_ITEMS as u32);
        for index in 0..count {
            let mut key = PROPERTYKEY::default();
            if store.GetAt(index, &mut key).is_err() {
                continue;
            }
            let Ok(mut value) = store.GetValue(&key) else {
                continue;
            };
            let name = take_string(PSGetNameFromPropertyKey(&key).ok());
            let shown = take_string(PSFormatForDisplayAlloc(&key, &value, PDFF_DEFAULT).ok());
            let _ = PropVariantClear(&mut value);
            if let (Some(name), Some(shown)) = (name, shown) {
                tags.extend(exif::property(&name, &shown));
            }
        }
    }
    tags
}

/// Copies and frees a CoTaskMem string returned by a shell call.
///
/// # Safety
/// `string` must be null or a CoTaskMem allocation that nothing else frees.
unsafe fn take_string(string: Option<PWSTR>) -> Option<String> {
    let string = string.filter(|string| !string.is_null())?;
    // SAFETY: the allocation is valid until freed here, once.
    unsafe {
        let text = string.to_string().ok();
        CoTaskMemFree(Some(string.0.cast_const().cast()));
        text
    }
}
/// EXIF tags: date, sub-seconds and offset for the original and digitized times.
const ORIGINAL: [&str; 3] = [
    "/exif/{ushort=36867}",
    "/exif/{ushort=37521}",
    "/exif/{ushort=36881}",
];
const DIGITIZED: [&str; 3] = [
    "/exif/{ushort=36868}",
    "/exif/{ushort=37522}",
    "/exif/{ushort=36882}",
];
fn read_image(readers: &[IWICMetadataQueryReader]) -> Metadata {
    let find = |paths: &[String]| {
        readers
            .iter()
            .find_map(|reader| paths.iter().find_map(|path| text(reader, path)))
            .filter(|value| !value.is_empty())
    };
    let tag = |suffix: &str| {
        let paths: Vec<String> = IFD_ROOTS
            .iter()
            .map(|root| format!("{root}{suffix}"))
            .collect();
        find(&paths)
    };
    let exif = |[date, subsec, offset]: [&str; 3], source| {
        let time = time::parse_exif(&tag(date)?)?;
        let millis = time
            .millis
            .or_else(|| tag(subsec).and_then(|text| time::parse_subsec(&text)));
        Some(Capture {
            time: time.with_millis(millis),
            offset: tag(offset).and_then(|text| time::parse_offset(&text)),
            source,
        })
    };
    let xmp = || {
        let paths: Vec<String> = XMP_ORIGINAL.iter().map(|path| path.to_string()).collect();
        let text = find(&paths)?;
        let (time, offset) =
            time::parse_iso(&text).or_else(|| time::parse_exif(&text).map(|time| (time, None)))?;
        Some(Capture {
            time,
            offset,
            source: TimeSource::ExifOriginal,
        })
    };
    Metadata {
        capture: exif(ORIGINAL, TimeSource::ExifOriginal)
            .or_else(|| exif(DIGITIZED, TimeSource::ExifDigitized))
            .or_else(xmp),
        make: tag("/{ushort=271}"),
        model: tag("/{ushort=272}"),
    }
}
/// A string metadata value at `path`, trimmed of EXIF padding.
fn text(reader: &IWICMetadataQueryReader, path: &str) -> Option<String> {
    let name: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let mut value = PROPVARIANT::default();
    // SAFETY: the query name is live; the variant is read and then cleared before return.
    unsafe {
        let found = reader
            .GetMetadataByName(PCWSTR(name.as_ptr()), &mut value)
            .is_ok();
        let result = found.then(|| variant_text(&value)).flatten();
        let _ = PropVariantClear(&mut value);
        result
    }
    .map(|text| {
        text.trim_matches(|c: char| c == '\0' || c.is_whitespace())
            .to_string()
    })
}
/// # Safety
/// `value` must be a PROPVARIANT filled by a COM call and not yet cleared.
unsafe fn variant_text(value: &PROPVARIANT) -> Option<String> {
    // SAFETY: the tag selects which union member is valid; string pointers stay valid
    // until the caller clears the variant.
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        match inner.vt {
            VT_LPSTR if !inner.Anonymous.pszVal.is_null() => {
                let bytes = inner.Anonymous.pszVal.as_bytes();
                Some(String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).into_owned())
            }
            VT_LPWSTR if !inner.Anonymous.pwszVal.is_null() => {
                inner.Anonymous.pwszVal.to_string().ok()
            }
            VT_BSTR => Some(inner.Anonymous.bstrVal.to_string()),
            _ => None,
        }
    }
}
fn video(path: &Path) -> Option<Metadata> {
    let mut file = std::fs::File::open(path).ok()?;
    let meta = bmff::read(&mut file).ok()?;
    let apple = meta
        .apple_created
        .as_deref()
        .and_then(time::parse_iso)
        .map(|(time, offset)| Capture {
            time,
            offset,
            source: TimeSource::QuickTimeLocal,
        });
    let capture = apple.or_else(|| {
        let (started, moved_back) = meta.started()?;
        let ticks = clock::unix_to_ticks(time::quicktime_to_unix(started)?)?;
        Some(Capture {
            time: clock::local_wall_clock(ticks)?,
            offset: None,
            source: if moved_back {
                TimeSource::QuickTimeUtcStart
            } else {
                TimeSource::QuickTimeUtc
            },
        })
    });
    Some(Metadata {
        capture,
        make: meta.make,
        model: meta.model,
    })
}
/// `System.Media.DateEncoded` through the Property System, converted from UTC.
fn encoded(path: &Path) -> Option<Metadata> {
    let wide = wide_path(path);
    // SAFETY: `wide` is live for the call; the store and variant are released here.
    let ticks = unsafe {
        let store: IPropertyStore =
            SHGetPropertyStoreFromParsingName(PCWSTR(wide.as_ptr()), None, GPS_DEFAULT).ok()?;
        let mut value = store.GetValue(&DATE_ENCODED).ok()?;
        let inner = &value.Anonymous.Anonymous;
        let ticks = (inner.vt == VT_FILETIME).then(|| {
            let time = inner.Anonymous.filetime;
            clock::file_time((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime))
        });
        let _ = PropVariantClear(&mut value);
        ticks?
    };
    Some(Metadata {
        capture: Some(Capture {
            time: clock::local_wall_clock(ticks)?,
            offset: None,
            source: TimeSource::Encoded,
        }),
        ..Metadata::default()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use powerkit_core::media_renamer::{bmff::fixtures, time::WallClock};
    use std::path::PathBuf;
    use windows::{
        Win32::{
            Foundation::GENERIC_WRITE,
            Graphics::Imaging::{
                GUID_ContainerFormatJpeg, GUID_WICPixelFormat24bppBGR, WICBitmapEncoderNoCache,
            },
            System::Com::CoTaskMemAlloc,
        },
        core::PSTR,
    };
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "powerkit-rename-probe-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    /// A 4x4 synthetic JPEG with the given EXIF string tags.
    fn jpeg(prober: &Prober, path: &Path, tags: &[(&str, &str)]) {
        let factory = prober.factory.as_ref().unwrap();
        let wide = wide_path(path);
        // SAFETY: every COM object stays on this thread. Each variant string is allocated
        // with CoTaskMemAlloc, so PropVariantClear may free it after the write.
        unsafe {
            let stream = factory.CreateStream().unwrap();
            stream
                .InitializeFromFilename(PCWSTR(wide.as_ptr()), GENERIC_WRITE.0)
                .unwrap();
            let encoder = factory
                .CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())
                .unwrap();
            encoder
                .Initialize(&stream, WICBitmapEncoderNoCache)
                .unwrap();
            let (mut frame, mut options) = (None, None);
            encoder.CreateNewFrame(&mut frame, &mut options).unwrap();
            let frame = frame.unwrap();
            frame.Initialize(options.as_ref()).unwrap();
            frame.SetSize(4, 4).unwrap();
            let mut format = GUID_WICPixelFormat24bppBGR;
            frame.SetPixelFormat(&mut format).unwrap();
            let writer = frame.GetMetadataQueryWriter().unwrap();
            for (query, text) in tags {
                let name: Vec<u16> = query.encode_utf16().chain(Some(0)).collect();
                let buffer = CoTaskMemAlloc(text.len() + 1).cast::<u8>();
                std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len());
                *buffer.add(text.len()) = 0;
                let mut value = PROPVARIANT::default();
                let header = &mut *value.Anonymous.Anonymous;
                header.vt = VT_LPSTR;
                header.Anonymous.pszVal = PSTR(buffer);
                writer
                    .SetMetadataByName(PCWSTR(name.as_ptr()), &value)
                    .unwrap();
                PropVariantClear(&mut value).unwrap();
            }
            let pixels = [0x80u8; 4 * 4 * 3];
            frame.WritePixels(4, 12, &pixels).unwrap();
            frame.Commit().unwrap();
            encoder.Commit().unwrap();
        }
    }
    #[test]
    fn exif_original_time_sub_seconds_offset_and_model_are_read() {
        let dir = temp("jpeg");
        let prober = Prober::new();
        let path = dir.join("a.jpg");
        jpeg(
            &prober,
            &path,
            &[
                ("/app1/ifd/exif/{ushort=36867}", "2024:05:01 14:03:22"),
                ("/app1/ifd/exif/{ushort=37521}", "457"),
                ("/app1/ifd/exif/{ushort=36881}", "+03:00"),
                ("/app1/ifd/exif/{ushort=36868}", "2023:01:01 00:00:00"),
                ("/app1/ifd/{ushort=272}", "Synthetic Cam"),
            ],
        );
        let Probe::Read(metadata) = prober.probe(&path, Kind::Jpeg) else {
            panic!("synthetic JPEG should be readable");
        };
        let capture = metadata.capture.unwrap();
        assert_eq!(capture.source, TimeSource::ExifOriginal);
        assert_eq!(
            capture.time,
            WallClock::new(2024, 5, 1, 14, 3, 22)
                .unwrap()
                .with_millis(Some(457))
        );
        assert_eq!(capture.offset, Some(180));
        assert_eq!(metadata.model.as_deref(), Some("Synthetic Cam"));

        let digitized = dir.join("b.jpg");
        jpeg(
            &prober,
            &digitized,
            &[("/app1/ifd/exif/{ushort=36868}", "2023:02:03 04:05:06")],
        );
        let Probe::Read(metadata) = prober.probe(&digitized, Kind::Jpeg) else {
            panic!("synthetic JPEG should be readable");
        };
        assert_eq!(
            metadata.capture.map(|c| c.source),
            Some(TimeSource::ExifDigitized)
        );
        let bare = dir.join("c.jpg");
        jpeg(&prober, &bare, &[]);
        assert!(matches!(
            prober.probe(&bare, Kind::Jpeg),
            Probe::Read(Metadata { capture: None, .. })
        ));
        std::fs::write(dir.join("broken.jpg"), b"not a jpeg").unwrap();
        assert_eq!(
            prober.probe(&dir.join("broken.jpg"), Kind::Jpeg),
            Probe::Unreadable
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn dump_lists_every_tag_with_its_section_and_name() {
        let dir = temp("dump");
        let prober = Prober::new();
        let path = dir.join("a.jpg");
        jpeg(
            &prober,
            &path,
            &[
                ("/app1/ifd/exif/{ushort=36867}", "2024:05:01 14:03:22"),
                ("/app1/ifd/{ushort=271}", "Synthetic Maker"),
                ("/app1/ifd/{ushort=272}", "Synthetic Cam"),
                ("/app1/ifd/gps/{ushort=1}", "N"),
            ],
        );
        let tags = prober.dump(&path, Kind::Jpeg);
        let find = |section: &str, name: &str| {
            tags.iter()
                .find(|tag| tag.section == section && tag.name == name)
                .map(|tag| tag.value.as_str())
        };
        assert_eq!(find("GPS", "GPSLatitudeRef"), Some("N"), "{tags:?}");
        assert!(
            tags.iter()
                .all(|tag| tag.section != "luminance" && tag.section != "chrominance"),
            "{tags:?}"
        );
        assert_eq!(find("Image", "Make"), Some("Synthetic Maker"), "{tags:?}");
        assert_eq!(find("Image", "Model"), Some("Synthetic Cam"), "{tags:?}");
        assert_eq!(
            find("EXIF", "DateTimeOriginal"),
            Some("2024:05:01 14:03:22"),
            "{tags:?}"
        );
        let position = |section: &str| tags.iter().position(|tag| tag.section == section);
        assert!(position("Image") < position("EXIF"));

        std::fs::write(dir.join("broken.jpg"), b"not a jpeg").unwrap();
        assert!(prober.dump(&dir.join("broken.jpg"), Kind::Jpeg).is_empty());
        assert!(prober.dump(&path, Kind::Sidecar).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn video_boxes_give_utc_or_apple_local_times() {
        let dir = temp("video");
        let prober = Prober::new();
        // 2024-05-01 12:00:00 UTC in QuickTime seconds.
        let quicktime: u32 = 1_714_564_800 + 2_082_844_800;
        let plain = dir.join("a.mp4");
        std::fs::write(&plain, fixtures::movie(&[fixtures::mvhd(quicktime)])).unwrap();
        let Probe::Read(metadata) = prober.probe(&plain, Kind::Video) else {
            panic!("synthetic MP4 should be readable");
        };
        let capture = metadata.capture.unwrap();
        assert_eq!(capture.source, TimeSource::QuickTimeUtc);
        assert_eq!(
            Some(capture.time),
            clock::local_wall_clock(clock::unix_to_ticks(1_714_564_800).unwrap())
        );
        // An Android recording of 90 s: the stored time is its end.
        let android = dir.join("c.mp4");
        std::fs::write(
            &android,
            fixtures::movie(&[
                fixtures::mvhd_lasting(quicktime, 1000, 90_000),
                fixtures::android_meta(),
            ]),
        )
        .unwrap();
        let Probe::Read(metadata) = prober.probe(&android, Kind::Video) else {
            panic!("synthetic Android MP4 should be readable");
        };
        let capture = metadata.capture.unwrap();
        assert_eq!(capture.source, TimeSource::QuickTimeUtcStart);
        assert_eq!(
            Some(capture.time),
            clock::local_wall_clock(clock::unix_to_ticks(1_714_564_800 - 90).unwrap())
        );
        let apple = dir.join("b.mov");
        std::fs::write(
            &apple,
            fixtures::movie(&[
                fixtures::mvhd(quicktime),
                fixtures::apple_meta(&[
                    ("creationdate", "2024-05-01T15:00:00+0300"),
                    ("model", "Synthetic Phone"),
                ]),
            ]),
        )
        .unwrap();
        let Probe::Read(metadata) = prober.probe(&apple, Kind::Video) else {
            panic!("synthetic MOV should be readable");
        };
        let capture = metadata.capture.unwrap();
        assert_eq!(capture.source, TimeSource::QuickTimeLocal);
        assert_eq!(capture.time, WallClock::new(2024, 5, 1, 15, 0, 0).unwrap());
        assert_eq!(capture.offset, Some(180));
        assert_eq!(metadata.model.as_deref(), Some("Synthetic Phone"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
