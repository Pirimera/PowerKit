//! Read-only capture metadata: WIC query readers for images, the ISO BMFF box reader for
//! MP4/MOV and the Windows Property System as a fallback. Never writes to the file.
//!
//! A `Prober` owns COM and the WIC factory for the worker thread that created it.
use super::super::clock;
use crate::media_renamer::{
    Kind, bmff,
    plan::{Metadata, Probe},
    time::{self, Capture, TimeSource},
};
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, PROPERTYKEY, WINCODEC_ERR_COMPONENTNOTFOUND},
        Graphics::Imaging::{
            CLSID_WICImagingFactory, IWICImagingFactory, IWICMetadataQueryReader,
            WICDecodeMetadataCacheOnDemand,
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
                StructuredStorage::{PROPVARIANT, PropVariantClear},
            },
            Variant::{VT_BSTR, VT_FILETIME, VT_LPSTR, VT_LPWSTR},
        },
        UI::Shell::PropertiesSystem::{
            GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName,
        },
    },
    core::{GUID, PCWSTR},
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
        // SAFETY: decoder and frame are owned COM pointers used only on this thread.
        let readers: Vec<IWICMetadataQueryReader> = unsafe {
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
        };
        Probe::Read(read_image(&readers))
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
        let ticks = clock::unix_to_ticks(time::quicktime_to_unix(meta.created?)?)?;
        Some(Capture {
            time: clock::local_wall_clock(ticks)?,
            offset: None,
            source: TimeSource::QuickTimeUtc,
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
            ((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)) as i64
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
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_renamer::{bmff::fixtures, time::WallClock};
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
