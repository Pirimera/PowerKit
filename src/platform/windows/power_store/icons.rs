//! Package logos from local `DisplayIcon` paths and optional HTTP(S) icon URLs.
//!
//! Extraction/download/encoding run only on worker threads through [`prefetch_local`]
//! and [`prefetch_remote`]; the UI thread reads finished bytes with [`cached`] and never
//! touches files, COM or the network. Downloaded logos are scaled down and kept in
//! `%LOCALAPPDATA%\PowerKit\icons` so later starts need no network.
use super::super::wide;
use crate::power_store::PackageSummary;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::{
    Foundation::Uri,
    Web::Http::HttpClient,
    Win32::{
        Foundation::HGLOBAL,
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
            GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory,
            WICBitmapEncoderNoCache, WICBitmapInterpolationModeFant, WICConvertBitmapSource,
            WICDecodeMetadataCacheOnDemand,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, STREAM_SEEK_CUR, STREAM_SEEK_SET,
            StructuredStorage::CreateStreamOnHGlobal,
        },
        UI::WindowsAndMessaging::HICON,
    },
    core::{HSTRING, Interface},
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, PrivateExtractIconsW};

/// Pixel size of the extracted icon; the table shows it at 24 DIP, so this stays sharp to 200%.
const ICON_PIXELS: i32 = 48;
/// Downloaded logos are scaled down to this longest side before they are cached.
const REMOTE_ICON_PIXELS: u32 = 96;
/// A small BGRA PNG is far below this; larger output means a broken encoder or source.
const MAX_PNG_BYTES: u64 = 256 * 1024;
const MAX_ICON_DOWNLOAD_BYTES: usize = 2 * 1024 * 1024;
const ICON_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// Larger bodies (the icon database) get more time.
const BULK_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// A failed lookup is not repeated for this long (UniGetUI retries on a similar interval).
const FAILED_RETRY: Duration = Duration::from_secs(10 * 60);
const DOWNLOAD_WORKERS: usize = 6;
const USER_AGENT: &str = "PowerKit";

enum Entry {
    Ready(Arc<[u8]>),
    /// Claimed by a worker; other workers skip it.
    Loading,
    Failed(Instant),
}

type Cache = Mutex<HashMap<String, Entry>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Finished PNG bytes for an icon location, if a prefetch already produced them.
pub fn cached(location: &str) -> Option<Arc<[u8]>> {
    match cache().lock().ok()?.get(location) {
        Some(Entry::Ready(bytes)) => Some(Arc::clone(bytes)),
        _ => None,
    }
}

/// Marks every location that still needs work as `Loading` and returns those.
fn claim<'a>(locations: impl Iterator<Item = &'a str>) -> Vec<String> {
    let Ok(mut cache) = cache().lock() else {
        return Vec::new();
    };
    let mut claimed = Vec::new();
    for location in locations.filter(|location| !location.is_empty()) {
        let needs_work = match cache.get(location) {
            None => true,
            Some(Entry::Failed(at)) => at.elapsed() >= FAILED_RETRY,
            Some(Entry::Ready(_) | Entry::Loading) => false,
        };
        if needs_work {
            cache.insert(location.to_string(), Entry::Loading);
            claimed.push(location.to_string());
        }
    }
    claimed
}

fn finish(location: &str, png: Option<Vec<u8>>) {
    if let Ok(mut cache) = cache().lock() {
        let entry = match png {
            Some(bytes) => Entry::Ready(Arc::from(bytes)),
            None => Entry::Failed(Instant::now()),
        };
        cache.insert(location.to_string(), entry);
    }
}

/// Extracts every not-yet-cached local `DisplayIcon` (never the network). Worker thread only.
pub fn prefetch_local(packages: &[PackageSummary]) {
    let pending = claim(
        packages
            .iter()
            .filter_map(|package| package.icon.as_deref())
            .filter(|location| !is_http_url(location)),
    );
    if pending.is_empty() {
        return;
    }
    let encoder = PngEncoder::new();
    for location in pending {
        let png = encoder
            .as_ref()
            .and_then(|encoder| resolve_local(encoder, &location));
        finish(&location, png);
    }
}

/// Downloads every not-yet-cached HTTP(S) icon with a few parallel workers and calls
/// `on_ready` after each logo becomes available. Worker thread only; returns when done.
pub fn prefetch_remote(locations: &[String], on_ready: &(dyn Fn() + Sync)) {
    let pending = claim(
        locations
            .iter()
            .map(String::as_str)
            .filter(|location| is_http_url(location)),
    );
    if pending.is_empty() {
        return;
    }
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..DOWNLOAD_WORKERS.min(pending.len()) {
            scope.spawn(|| {
                // Each worker owns its COM apartment and WIC factory.
                let encoder = PngEncoder::new();
                while let Some(location) = pending.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let png = encoder
                        .as_ref()
                        .and_then(|encoder| resolve_remote(encoder, location));
                    let ready = png.is_some();
                    finish(location, png);
                    if ready {
                        on_ready();
                    }
                }
            });
        }
    });
}

/// Downloads an HTTPS body for the icon database refresh. Worker thread only.
pub fn download_url(url: &str, max_bytes: usize) -> Option<Vec<u8>> {
    download_http(url, max_bytes, BULK_DOWNLOAD_TIMEOUT)
}

fn resolve_local(encoder: &PngEncoder, location: &str) -> Option<Vec<u8>> {
    parse_location(location)
        .and_then(|(path, index)| extract_icon(&path, index))
        .and_then(|icon| encoder.encode_hicon(&icon))
}

/// Disk cache first, then the network; fresh downloads are stored for the next start.
fn resolve_remote(encoder: &PngEncoder, url: &str) -> Option<Vec<u8>> {
    let path = disk_cache_path(url);
    if let Some(png) = path.as_ref().and_then(|path| read_disk_icon(path)) {
        return Some(png);
    }
    let bytes = download_http(url, MAX_ICON_DOWNLOAD_BYTES, ICON_DOWNLOAD_TIMEOUT)?;
    let png = encoder.encode_image_bytes(&bytes)?;
    if let Some(path) = path {
        write_disk_icon(&path, &png);
    }
    Some(png)
}

pub(super) fn is_http_url(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("https://") || trimmed.starts_with("http://")
}

/// `%LOCALAPPDATA%\PowerKit\icons\<hash of url>.png`; a changed URL is a new file.
fn disk_cache_path(url: &str) -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(base)
            .join("PowerKit")
            .join("icons")
            .join(format!("{:016x}.png", fnv1a(url.as_bytes()))),
    )
}

/// Stable 64-bit FNV-1a; the std hasher may change between Rust versions.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn read_disk_icon(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    (bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() as u64 <= MAX_PNG_BYTES)
        .then_some(bytes)
}

fn write_disk_icon(path: &Path, png: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Write beside the target first so a crash never leaves a truncated logo behind.
    let temp = path.with_extension("tmp");
    if std::fs::write(&temp, png).is_ok() && std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
}

fn download_http(url: &str, max_bytes: usize, timeout: Duration) -> Option<Vec<u8>> {
    if !is_http_url(url) {
        return None;
    }
    // WinRT HttpClient on a worker thread; poll instead of blocking the apartment.
    let client = HttpClient::new().ok()?;
    // Some image hosts reject requests without a product token.
    if let Ok(headers) = client.DefaultRequestHeaders()
        && let Ok(agents) = headers.UserAgent()
    {
        let _ = agents.TryParseAdd(&HSTRING::from(USER_AGENT));
    }
    let uri = Uri::CreateUri(&HSTRING::from(url)).ok()?;
    let op = client.GetBufferAsync(&uri).ok()?;
    let started = Instant::now();
    let buffer = loop {
        match op.Status() {
            Ok(windows_future::AsyncStatus::Started) => {
                if started.elapsed() > timeout {
                    if let Ok(info) = op.cast::<windows_future::IAsyncInfo>() {
                        let _ = info.Cancel();
                    }
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(windows_future::AsyncStatus::Completed) => break op.GetResults().ok()?,
            _ => return None,
        }
    };
    let length = buffer.Length().ok()? as usize;
    if length == 0 || length > max_bytes {
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

    /// Decodes any WIC-supported image and re-encodes it as a PNG whose longest side is at
    /// most [`REMOTE_ICON_PIXELS`] (UniGetUI also downsizes before caching).
    fn encode_image_bytes(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        // SAFETY: stream, decoder and converters stay on this thread; `bytes` outlives the decode.
        unsafe {
            let stream = self.factory.CreateStream().ok()?;
            stream.InitializeFromMemory(bytes).ok()?;
            let decoder = self
                .factory
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
                .ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            // Palette formats cannot be scaled directly; premultiplied alpha scales cleanly.
            let source = WICConvertBitmapSource(&GUID_WICPixelFormat32bppPBGRA, &frame).ok()?;
            let (mut width, mut height) = (0u32, 0u32);
            source.GetSize(&mut width, &mut height).ok()?;
            let longest = width.max(height);
            if longest <= REMOTE_ICON_PIXELS {
                return self.encode_source(&source);
            }
            let scale = f64::from(REMOTE_ICON_PIXELS) / f64::from(longest);
            let scaled_width = ((f64::from(width) * scale).round() as u32).max(1);
            let scaled_height = ((f64::from(height) * scale).round() as u32).max(1);
            let scaler = self.factory.CreateBitmapScaler().ok()?;
            scaler
                .Initialize(
                    &source,
                    scaled_width,
                    scaled_height,
                    WICBitmapInterpolationModeFant,
                )
                .ok()?;
            self.encode_source(&scaler)
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
        prefetch_local(std::slice::from_ref(&package));
        let png = cached(&location).expect("shell32 icon should encode");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(cached("C:\\missing\\nothing.exe,0").is_none());
    }

    #[test]
    fn fnv_hash_is_stable_across_builds() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn downloaded_images_are_scaled_to_the_logo_limit() {
        use windows::Win32::Graphics::Imaging::WICBitmapCacheOnDemand;
        let encoder = PngEncoder::new().expect("WIC factory");
        // SAFETY: the synthetic bitmap lives on this thread for the whole block.
        let (original, scaled) = unsafe {
            let bitmap = encoder
                .factory
                .CreateBitmap(
                    512,
                    256,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapCacheOnDemand,
                )
                .expect("synthetic bitmap");
            let original = encoder
                .encode_source(&bitmap)
                .expect("encode synthetic PNG");
            let scaled = encoder
                .encode_image_bytes(&original)
                .expect("scale synthetic PNG");
            (original, scaled)
        };
        assert_eq!(png_size(&encoder, &original), (512, 256));
        assert_eq!(
            png_size(&encoder, &scaled),
            (REMOTE_ICON_PIXELS, REMOTE_ICON_PIXELS / 2)
        );
    }

    fn png_size(encoder: &PngEncoder, bytes: &[u8]) -> (u32, u32) {
        // SAFETY: decoder objects stay on this thread; `bytes` outlives them.
        unsafe {
            let stream = encoder.factory.CreateStream().unwrap();
            stream.InitializeFromMemory(bytes).unwrap();
            let decoder = encoder
                .factory
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
                .unwrap();
            let frame = decoder.GetFrame(0).unwrap();
            let (mut width, mut height) = (0, 0);
            frame.GetSize(&mut width, &mut height).unwrap();
            (width, height)
        }
    }

    #[test]
    fn claim_skips_loading_ready_and_recently_failed_locations() {
        let ready = "https://example.test/claim-ready.png";
        let failed = "https://example.test/claim-failed.png";
        let fresh = "https://example.test/claim-fresh.png";
        finish(ready, Some(vec![1]));
        finish(failed, None);
        let claimed = claim([ready, failed, fresh, fresh, ""].into_iter());
        assert_eq!(claimed, [fresh]);
        assert!(cached(ready).is_some());
        assert!(cached(fresh).is_none());
    }
}
