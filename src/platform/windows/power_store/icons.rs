//! Package logos from local `DisplayIcon` paths and optional HTTP(S) icon URLs.
//!
//! Extraction/download/encoding run only on a worker thread through [`prefetch`];
//! the UI thread reads finished bytes with [`cached`] and never touches files,
//! COM or the network.
use super::super::wide;
use crate::power_store::PackageSummary;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use windows::{
    Foundation::Uri,
    Win32::{
        Foundation::HGLOBAL,
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
            IWICBitmapSource, IWICImagingFactory, WICBitmapEncoderNoCache,
            WICDecodeMetadataCacheOnDemand,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, STREAM_SEEK_CUR, STREAM_SEEK_SET,
            StructuredStorage::CreateStreamOnHGlobal,
        },
        UI::WindowsAndMessaging::HICON,
    },
    core::HSTRING,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, PrivateExtractIconsW};

/// Pixel size of the extracted icon; the table shows it at 24 DIP, so this stays sharp to 200%.
const ICON_PIXELS: i32 = 48;
/// A 48×48 BGRA PNG is far below this; larger output means a broken encoder or source.
const MAX_PNG_BYTES: u64 = 256 * 1024;
const MAX_DOWNLOAD_BYTES: usize = 512 * 1024;

type Cache = Mutex<HashMap<String, Option<Arc<[u8]>>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Finished PNG bytes for an icon location, if [`prefetch`] already produced them.
pub fn cached(location: &str) -> Option<Arc<[u8]>> {
    cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(location).cloned().flatten())
}

/// Extracts or downloads every not-yet-cached icon. Call from a worker thread only.
pub fn prefetch(packages: &[PackageSummary]) {
    let pending: Vec<String> = {
        let Ok(cache) = cache().lock() else {
            return;
        };
        let mut seen = std::collections::HashSet::new();
        packages
            .iter()
            .filter_map(|package| package.icon.as_deref())
            .filter(|location| !location.is_empty())
            .filter(|location| !cache.contains_key(*location) && seen.insert(*location))
            .map(str::to_string)
            .collect()
    };
    if pending.is_empty() {
        return;
    }
    let Some(encoder) = PngEncoder::new() else {
        return;
    };
    for location in pending {
        let png = resolve_png(&encoder, &location).map(Arc::<[u8]>::from);
        if let Ok(mut cache) = cache().lock() {
            cache.insert(location, png);
        }
    }
}

/// Downloads an HTTPS/HTTP body for icon database refresh. Worker thread only.
pub fn download_url(url: &str) -> Option<Vec<u8>> {
    download_http(url)
}

fn resolve_png(encoder: &PngEncoder, location: &str) -> Option<Vec<u8>> {
    if is_http_url(location) {
        let bytes = download_http(location)?;
        return encoder.encode_image_bytes(&bytes);
    }
    parse_location(location)
        .and_then(|(path, index)| extract_icon(&path, index))
        .and_then(|icon| encoder.encode_hicon(&icon))
}

pub(super) fn is_http_url(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("https://") || trimmed.starts_with("http://")
}

fn download_http(url: &str) -> Option<Vec<u8>> {
    if !is_http_url(url) {
        return None;
    }
    // WinRT HttpClient on the catalog worker; poll instead of blocking the apartment.
    let client = windows::Web::Http::HttpClient::new().ok()?;
    let uri = Uri::CreateUri(&HSTRING::from(url)).ok()?;
    let op = client.GetBufferAsync(&uri).ok()?;
    let buffer = loop {
        match op.Status() {
            Ok(windows_future::AsyncStatus::Started) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(windows_future::AsyncStatus::Completed) => break op.GetResults().ok()?,
            _ => return None,
        }
    };
    let length = buffer.Length().ok()? as usize;
    if length == 0 || length > MAX_DOWNLOAD_BYTES {
        return None;
    }
    let reader = windows::Storage::Streams::DataReader::FromBuffer(&buffer).ok()?;
    let mut bytes = vec![0u8; length];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(bytes)
}

/// Splits a `DisplayIcon` value into an existing file path and icon index.
fn parse_location(raw: &str) -> Option<(String, i32)> {
    let trimmed = raw.trim().trim_matches('"').trim();
    let (path, index) = match trimmed.rsplit_once(',') {
        Some((path, index)) => match index.trim().parse::<i32>() {
            Ok(index) => (path, index),
            Err(_) => (trimmed, 0),
        },
        None => (trimmed, 0),
    };
    let path = path.trim().trim_matches('"').trim();
    (!path.is_empty() && Path::new(path).is_file()).then(|| (path.to_string(), index))
}

/// Owned icon handle; destroyed on drop.
struct Icon(HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: the handle came from PrivateExtractIconsW and is destroyed exactly once.
        unsafe {
            DestroyIcon(self.0.0);
        }
    }
}

fn extract_icon(path: &str, index: i32) -> Option<Icon> {
    let path = wide(path);
    let mut icon = std::ptr::null_mut();
    let mut id = 0u32;
    // SAFETY: `path` is NUL-terminated UTF-16; one handle slot and one id slot are provided.
    let count = unsafe {
        PrivateExtractIconsW(
            path.as_ptr(),
            index,
            ICON_PIXELS,
            ICON_PIXELS,
            &mut icon,
            &mut id,
            1,
            0,
        )
    };
    (count != 0 && count != u32::MAX && !icon.is_null()).then(|| Icon(HICON(icon)))
}

/// Balances a successful `CoInitializeEx` on the same thread.
struct ComGuard(bool);

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: only set after CoInitializeEx succeeded on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// WIC factory bound to the current worker thread's COM apartment.
struct PngEncoder {
    // Declaration order is drop order: release the factory before CoUninitialize.
    factory: IWICImagingFactory,
    _com: ComGuard,
}

impl PngEncoder {
    fn new() -> Option<Self> {
        // SAFETY: called on the catalog worker. RPC_E_CHANGED_MODE keeps the thread's
        // existing apartment, which WIC also supports; only a successful init is balanced.
        let com = ComGuard(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok());
        // SAFETY: the factory is used and released on this thread only.
        let factory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
                .ok()?;
        Some(Self { factory, _com: com })
    }

    fn encode_hicon(&self, icon: &Icon) -> Option<Vec<u8>> {
        // SAFETY: every COM object below lives on this thread and is released before
        // returning; `icon` stays valid for the CreateBitmapFromHICON call.
        unsafe {
            let bitmap = self.factory.CreateBitmapFromHICON(icon.0).ok()?;
            self.encode_source(&bitmap)
        }
    }

    fn encode_image_bytes(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() as u64 <= MAX_PNG_BYTES {
            return Some(bytes.to_vec());
        }
        // SAFETY: stream and decoder stay on this thread; `bytes` outlives the decode.
        unsafe {
            let stream = self.factory.CreateStream().ok()?;
            stream.InitializeFromMemory(bytes).ok()?;
            let decoder = self
                .factory
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
                .ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            self.encode_source(&frame)
        }
    }

    unsafe fn encode_source(&self, source: &IWICBitmapSource) -> Option<Vec<u8>> {
        // SAFETY: caller keeps `source` alive; all COM objects are released before return.
        unsafe {
            let (mut width, mut height) = (0u32, 0u32);
            source.GetSize(&mut width, &mut height).ok()?;
            if width == 0 || height == 0 {
                return None;
            }
            let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).ok()?;
            let encoder = self
                .factory
                .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
                .ok()?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
            let mut frame = None;
            let mut options = None;
            encoder.CreateNewFrame(&mut frame, &mut options).ok()?;
            let frame = frame?;
            frame.Initialize(options.as_ref()).ok()?;
            frame.SetSize(width, height).ok()?;
            let mut format = GUID_WICPixelFormat32bppBGRA;
            frame.SetPixelFormat(&mut format).ok()?;
            frame.WriteSource(source, std::ptr::null()).ok()?;
            frame.Commit().ok()?;
            encoder.Commit().ok()?;

            let mut length = 0u64;
            stream.Seek(0, STREAM_SEEK_CUR, Some(&mut length)).ok()?;
            if length == 0 || length > MAX_PNG_BYTES {
                return None;
            }
            stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
            let mut bytes = vec![0u8; length as usize];
            let mut read = 0u32;
            stream
                .Read(bytes.as_mut_ptr().cast(), length as u32, Some(&mut read))
                .ok()
                .ok()?;
            bytes.truncate(read as usize);
            (!bytes.is_empty()).then_some(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_icon_locations_parse_quotes_and_indices() {
        let exe = std::env::current_exe().unwrap();
        let exe = exe.to_string_lossy();
        assert_eq!(parse_location(&exe), Some((exe.to_string(), 0)));
        assert_eq!(
            parse_location(&format!("\"{exe}\",-101")),
            Some((exe.to_string(), -101))
        );
        assert_eq!(
            parse_location(&format!("{exe}, 2")),
            Some((exe.to_string(), 2))
        );
        assert_eq!(parse_location("C:\\missing\\nothing.exe,0"), None);
        assert_eq!(parse_location(""), None);
    }

    #[test]
    fn system_icon_is_encoded_as_png_and_cached() {
        use crate::power_store::{ManagerKind, PackageKey};
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let location = format!("{windows}\\System32\\shell32.dll,3");
        let package = PackageSummary {
            key: PackageKey::new(ManagerKind::WinGet, "local", "Synthetic.Icon"),
            name: "Synthetic".into(),
            version: Some("1.0".into()),
            available_version: None,
            update_available: false,
            scope: None,
            install_date: None,
            updated_at: None,
            icon: Some(location.clone()),
        };
        prefetch(std::slice::from_ref(&package));
        let png = cached(&location).expect("shell32 icon should encode");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(cached("C:\\missing\\nothing.exe,0").is_none());
    }
}
