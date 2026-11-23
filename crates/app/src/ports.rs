//! Operating system services a controller may use.
//!
//! Each trait names a capability, not a system call, so another platform can
//! implement it its own way. Implementations are called on the application's
//! main thread and need not be `Send`: the keep-awake request in particular
//! belongs to the thread that made it.
use crate::controller::always_top::{Cue, WindowId};
use crate::controller::hotkeys::Slot;
use crate::controller::media_renamer::{Listing, Recheck};
use crate::controller::power_mouse::RequestError;
use crate::controller::power_store::{
    CatalogError, InstalledList, ManagerProbe, ManagerStatus, Prepared,
};
use powerkit_core::always_on::{Mode, PowerSource};
use powerkit_core::hotkey::Hotkey;
use powerkit_core::image_resizer::{PlannedWrite, ResizeSettings, SourceImage};
use powerkit_core::media_renamer::{Kind, exif::ExifTag, history::Fingerprint, plan::Probe};
use powerkit_core::power_mouse::hidpp::{PAYLOAD_LEN, Request};
use powerkit_core::power_store::{
    ManagerKind, PackageDetails, PackageKey, PackageSummary, QueuedOperation,
};
use powerkit_core::primitives::UtcTicks;
use powerkit_core::settings::Preferences;
use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

/// Monotonic uptime and the wall clock.
pub trait Clock {
    /// Milliseconds on a clock that never goes back and keeps counting while
    /// the machine sleeps.
    fn uptime_ms(&self) -> u64;
    fn now_utc(&self) -> UtcTicks;
}

/// Holds the request that keeps the system, and optionally the display, awake.
pub trait KeepAwake {
    /// Replaces the current request. `Mode::Off` releases it. `display` is
    /// ignored while the mode is off.
    fn keep_awake(&mut self, mode: Mode, display: bool) -> io::Result<()>;
}

/// The AC line and battery level.
pub trait PowerStatus {
    /// An unknown AC state must be reported as on AC, so it never ends a session.
    fn power_source(&self) -> PowerSource;
}

/// A process whose exit can be observed without a pid being reused.
pub trait WatchedProcess {
    fn exited(&self) -> bool;
    /// A short name to show, such as the executable's stem.
    fn name(&self) -> &str;
}

/// Opens other processes for watching.
pub trait Processes {
    fn current_pid(&self) -> u32;
    /// `None` when the process does not exist, cannot be opened or already exited.
    fn watch(&mut self, pid: u32) -> Option<Box<dyn WatchedProcess>>;
}

/// The one wake-up a controller keeps scheduled on the main thread.
pub trait WakeTimer {
    /// Wakes the controller after `delay_ms`, replacing any earlier wake-up;
    /// `None` cancels it.
    fn wake_after(&mut self, delay_ms: Option<u64>) -> io::Result<()>;
}

/// Durable storage for the user's preferences.
pub trait SettingsStore {
    /// Replaces the stored preferences as a whole. On failure the previous
    /// file must be left intact.
    fn save(&mut self, prefs: &Preferences) -> io::Result<()>;
}

/// Registration that starts PowerKit when the user signs in.
pub trait Autostart {
    fn set_autostart(&mut self, enabled: bool) -> io::Result<()>;
}

/// System-wide shortcut registration.
pub trait Hotkeys {
    /// Registers `chord` for `slot`. `false` when the system refuses it, which
    /// includes a chord another slot or application already holds.
    fn register(&mut self, slot: Slot, chord: Hotkey) -> bool;
    /// Releases the slot; releasing an unregistered slot is harmless.
    fn unregister(&mut self, slot: Slot);
}

/// Top-level windows of other applications that can be kept on top.
///
/// The platform owns each pin: the topmost state, the frame around the
/// window and its transparency, all restored when the pin ends. A platform
/// that cannot keep another application's window on top does not offer the
/// tool at all.
pub trait PinnableWindows {
    /// The top-level window in front, if any.
    fn foreground(&self) -> Option<WindowId>;
    /// Whether the window exists and belongs to PowerKit itself.
    fn is_own(&self, window: WindowId) -> bool;
    /// Whether the window exists and may be pinned. PowerKit's own windows,
    /// the desktop, shell surfaces and hidden windows may not.
    fn eligible(&self, window: WindowId) -> bool;
    fn is_live(&self, window: WindowId) -> bool;
    /// The pinned windows in the order they were pinned. A window that closed
    /// stays listed until it is unpinned.
    fn pinned(&self) -> Vec<WindowId>;
    fn pin(&mut self, window: WindowId) -> io::Result<()>;
    /// Ends the pin and restores what pinning changed. Unpinning a window
    /// that is not pinned, or that closed, succeeds.
    fn unpin(&mut self, window: WindowId) -> io::Result<()>;
    /// Opacity of a window in percent.
    fn opacity(&self, window: WindowId) -> i32;
    fn set_opacity(&mut self, window: WindowId, percent: i32) -> io::Result<()>;
    /// The window title; empty when it has none or closed.
    fn title(&self, window: WindowId) -> String;
    fn play(&mut self, cue: Cue);
}

/// Reads image files and writes resized ones. Created and used on one worker
/// thread.
pub trait ImageCodec {
    /// Size, resolution and container of an image, without decoding its pixels.
    fn probe(&self, path: &Path) -> io::Result<SourceImage>;
    /// Carries out one planned write. A replacement must keep the original
    /// restorable until the new file is in place, and a copy must not
    /// overwrite a file that appeared since the preview.
    fn write(&self, plan: &PlannedWrite, settings: &ResizeSettings) -> io::Result<()>;
}

/// The files MediaRenamer lists, reads and renames, and its history file.
///
/// One value is shared with the session's worker threads. The order of a
/// batch (revalidate, journal first, group rollback) is decided by
/// `media_renamer::commit`; the implementation gives it three guarantees:
/// [`Self::rename`] never replaces an existing file, [`Self::fingerprint`]
/// and [`Self::exists`] never follow a link, and [`Self::replace_text`]
/// leaves either the old file or the complete new one.
pub trait MediaFiles: Send + Sync {
    /// Lists `inputs` (files or folders) without opening file contents.
    /// `known` holds the keys of files already in the workspace; `room` is how
    /// many more media files the batch may take.
    fn list(
        &self,
        inputs: &[PathBuf],
        recurse: bool,
        known: &HashSet<String>,
        room: usize,
        cancel: &AtomicBool,
    ) -> Listing;
    /// Keys of every entry in `dirs`, so collision checks see what is on disk now.
    fn folder_names(&self, dirs: &[PathBuf]) -> HashSet<String>;
    /// A metadata reader for the calling worker thread; it must stay on it.
    fn prober(&self) -> Box<dyn MediaProber>;
    /// Size and last-write time of the regular file at `path`; `None` when
    /// nothing is there or it is not a regular file.
    fn fingerprint(&self, path: &Path) -> Option<Fingerprint>;
    /// Whether anything, of any kind, has this name.
    fn exists(&self, path: &Path) -> bool;
    /// Renames one file. It fails, and changes nothing, when `to` exists.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// The text of a file, `Ok(None)` when there is no such file. A file
    /// larger than `limit` bytes is an error and is not read.
    fn read_text(&self, path: &Path, limit: u64) -> io::Result<Option<String>>;
    /// Replaces the file with `text`, creating its folder. The text is on
    /// disk when this returns; on failure the previous file is unchanged.
    fn replace_text(&self, path: &Path, text: &str) -> io::Result<()>;
    fn now_utc(&self) -> UtcTicks;
    /// A UTC instant as local date and time text for a history row.
    fn local_stamp(&self, at: UtcTicks) -> String;
}

/// Reads capture metadata on one worker thread.
pub trait MediaProber {
    /// The capture time and camera of a file, or why it could not be read.
    fn probe(&self, path: &Path, kind: Kind) -> Probe;
    /// Every metadata value of a file, for the details drawer.
    fn dump(&self, path: &Path, kind: Kind) -> Vec<ExifTag>;
    /// Checks a listed file against the size and time the preview recorded.
    /// A file that is not `readable` (online-only) is never opened, because
    /// reading it would download it.
    fn recheck(&self, path: &Path, kind: Kind, known: Fingerprint, readable: bool) -> Recheck;
}

/// The package managers of this machine, their logos, and the files PowerStore
/// writes.
///
/// One value is shared with the session's worker threads, which make every
/// call here; none of them may be made on the main thread. A manager the user
/// switched off is located but never started.
pub trait PackageCatalog: Send + Sync {
    /// Which managers are installed, without starting any of them.
    fn manager_statuses(&self) -> Vec<ManagerStatus>;
    /// Installed packages of every enabled manager, with available updates.
    fn list_installed(&self) -> Result<InstalledList, CatalogError>;
    /// Packages matching `query`; `source` narrows the search to one source.
    fn search(
        &self,
        query: &str,
        source: Option<&str>,
    ) -> Result<Vec<PackageSummary>, CatalogError>;
    fn details(&self, key: &PackageKey) -> Result<PackageDetails, CatalogError>;
    /// Where a manager's program is and which version it reports.
    fn probe_manager(&self, kind: ManagerKind) -> ManagerProbe;
    /// Prepares one install, update or uninstall. Nothing starts until the
    /// returned `run` is called on a worker thread.
    fn prepare(&self, operation: QueuedOperation) -> Prepared;
    /// Decoded PNG bytes of a logo that is already in memory.
    fn cached_icon(&self, location: &str) -> Option<Arc<[u8]>>;
    /// Logo URLs from the icon database for rows that have none.
    fn resolve_icons(&self, packages: &[PackageSummary]) -> Vec<(PackageKey, String)>;
    /// Downloads and decodes logos, calling `on_ready` as each one finishes.
    fn fetch_icons(&self, locations: &[String], on_ready: &(dyn Fn() + Sync));
    /// Replaces a file as a whole; a failure leaves the previous file intact.
    fn write_text(&self, path: &Path, text: &str) -> io::Result<()>;
    /// Now, as local date and time text for history rows and check times.
    fn now_local_stamp(&self) -> String;
    /// Now, as ISO 8601 UTC text for an export document.
    fn now_iso_utc(&self) -> String;
}

/// HID++ report traffic with one device. A channel belongs to the thread that
/// opened it; other clients may talk to the same device at the same time, and
/// their replies are not ours.
pub trait HidppChannel {
    /// Sends a request and waits for its own reply. A device that refuses
    /// answers with an error code; one that is asleep or gone times out.
    fn request(&mut self, request: &Request) -> Result<[u8; PAYLOAD_LEN], RequestError>;
    /// Sends a request the device does not answer, such as moving itself to
    /// another computer.
    fn notify(&mut self, request: &Request) -> io::Result<()>;
}
