//! Real device renders for recognized mice, fetched off the UI thread and
//! cached in `%LOCALAPPDATA%\PowerKit\mouse-assets`.
//!
//! Uses OpenLogi's public device asset CDN (`assets.openlogi.org`, with a
//! jsDelivr mirror fallback) so no third-party product artwork ships inside the
//! repository or binary. The UI thread reads only from the in-memory cache via
//! [`cached`] and falls back to the generic vector silhouette while loading or
//! offline.
use super::super::power_store::icons::download_url;
use powerkit_app::event::Event;
use powerkit_core::power_mouse::device::Device;
use powerkit_core::power_mouse::diagram::{device_art, png_dimensions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::HWND;

/// Largest device PNG accepted from disk or the network (high-DPI studio renders
/// are ~600–850 KB).
const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024;
/// Wait before retrying a failed fetch so offline sessions do not loop.
const FAILED_RETRY: Duration = Duration::from_secs(10 * 60);
const PRIMARY_BASE: &str = "https://assets.openlogi.org/v1/devices";
const FALLBACK_BASE: &str = "https://cdn.jsdelivr.net/npm/@logi-assets/pointing@0.1/v1/devices";

pub use powerkit_app::ui::{DeviceImage as CachedImage, MouseImageKind as ImageKind};

const ALL_KINDS: [ImageKind; 2] = [ImageKind::Side, ImageKind::Front];

fn remote_file(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::Side => "side_core.png",
        ImageKind::Front => "front_core.png",
    }
}

fn cache_suffix(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::Side => "side",
        ImageKind::Front => "front",
    }
}

enum Entry {
    Ready(CachedImage),
    Loading,
    Failed(Instant),
}

type Cache = Mutex<HashMap<(&'static str, ImageKind), Entry>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

static REVISION: AtomicU64 = AtomicU64::new(0);

/// Monotonic counter bumped whenever a device render becomes available so the
/// UI snapshot detects the change and re-renders.
pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

/// Non-blocking read of a cached device image for the WinUI shell.
pub fn cached(depot: &'static str, kind: ImageKind) -> Option<CachedImage> {
    match cache().lock().ok()?.get(&(depot, kind)) {
        Some(Entry::Ready(image)) => Some(image.clone()),
        _ => None,
    }
}

/// Starts a background fetch (or disk-cache read) for every recognized device
/// whose renders are not in memory yet. Safe to call on the UI thread.
pub fn prefetch(devices: &[Device], app: HWND) {
    let mut pending: Vec<(&'static str, ImageKind)> = Vec::new();
    {
        let Ok(mut guard) = cache().lock() else {
            return;
        };
        for device in devices {
            let Some(art) = device_art(device.product_id, &device.name) else {
                continue;
            };
            for kind in ALL_KINDS {
                let key = (art.depot, kind);
                let needs_work = match guard.get(&key) {
                    None => true,
                    Some(Entry::Failed(at)) => at.elapsed() >= FAILED_RETRY,
                    Some(Entry::Ready(_) | Entry::Loading) => false,
                };
                if needs_work {
                    guard.insert(key, Entry::Loading);
                    pending.push(key);
                }
            }
        }
    }
    if pending.is_empty() {
        return;
    }
    let hwnd = app as isize;
    let _ = std::thread::Builder::new()
        .name("power-mouse-assets".into())
        .spawn(move || {
            let mut any_ready = false;
            for (depot, kind) in pending {
                let loaded = load_or_fetch(depot, kind);
                let ready = loaded.is_some();
                if let Ok(mut guard) = cache().lock() {
                    guard.insert(
                        (depot, kind),
                        match loaded {
                            Some(image) => Entry::Ready(image),
                            None => Entry::Failed(Instant::now()),
                        },
                    );
                }
                if ready {
                    any_ready = true;
                }
            }
            if any_ready {
                REVISION.fetch_add(1, Ordering::Relaxed);
                if hwnd != 0 {
                    crate::events::post(hwnd as HWND, Event::MouseDevicesChanged);
                }
            }
        });
}

fn load_or_fetch(depot: &str, kind: ImageKind) -> Option<CachedImage> {
    let disk_path = cache_path(depot, kind);
    if let Some(bytes) = disk_path.as_deref().and_then(read_cached_png)
        && let Some((width, height)) = png_dimensions(&bytes)
    {
        return Some(CachedImage {
            bytes: Arc::from(bytes),
            width,
            height,
        });
    }
    let file = remote_file(kind);
    let primary = format!("{PRIMARY_BASE}/{depot}/{file}");
    let fallback = format!("{FALLBACK_BASE}/{depot}/{file}");
    let bytes = download_valid_png(&primary).or_else(|| download_valid_png(&fallback))?;
    let (width, height) = png_dimensions(&bytes)?;
    if let Some(path) = disk_path {
        write_cached_png(&path, &bytes);
    }
    Some(CachedImage {
        bytes: Arc::from(bytes),
        width,
        height,
    })
}

fn download_valid_png(url: &str) -> Option<Vec<u8>> {
    let bytes = download_url(url, MAX_ASSET_BYTES)?;
    png_dimensions(&bytes).map(|_| bytes)
}

fn cache_path(depot: &str, kind: ImageKind) -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(base)
            .join("PowerKit")
            .join("mouse-assets")
            .join(format!("{depot}_{}.png", cache_suffix(kind))),
    )
}

fn read_cached_png(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    (bytes.len() <= MAX_ASSET_BYTES && png_dimensions(&bytes).is_some()).then_some(bytes)
}

fn write_cached_png(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temp = path.with_extension("tmp");
    if std::fs::write(&temp, bytes).is_ok() && std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_uses_mouse_assets_directory() {
        if let Some(path) = cache_path("mx_master_4", ImageKind::Side) {
            let text = path.to_string_lossy();
            assert!(text.contains("PowerKit"));
            assert!(text.contains("mouse-assets"));
            assert!(text.ends_with("mx_master_4_side.png"));
        }
    }
}
