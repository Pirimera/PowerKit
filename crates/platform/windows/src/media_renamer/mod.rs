//! MediaRenamer on Windows. The workspace itself is `powerkit_app::controller::media_renamer`;
//! this module gives it its files: directory walks, WIC and Property System
//! metadata, `MoveFileExW` renames that never replace, the pickers and the Explorer
//! handoff. Workers raise `Event::RenamerProgress` so the UI thread polls the
//! session.
mod commit;
pub mod handoff;
pub mod pick;
mod probe;
mod scan;
#[cfg(test)]
mod tests;

pub use powerkit_app::controller::media_renamer::{Action, Session};
#[cfg(test)]
pub use powerkit_app::controller::media_renamer::{DrawerView, MetadataView, Notice, StateFilter};

use super::clock;
use powerkit_app::controller::media_renamer::{Listing, Recheck};
use powerkit_app::ports::{MediaFiles, MediaProber};
use powerkit_core::media_renamer::{
    Kind, Preferences, exif::ExifTag, history::Fingerprint, plan::Probe,
};
use powerkit_core::primitives::UtcTicks;
use std::{
    collections::HashSet,
    io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
use windows_sys::Win32::Foundation::HWND;

/// Win32 paths from the `\\?\` form start here: the classic limit is 260 units, and the
/// verbatim form is used a little earlier so 8.3 names of long folders still fit.
const LONG_PATH_UNITS: usize = 248;

/// NUL-terminated UTF-16 for a Win32 call. Long absolute paths get the `\\?\` form, so
/// renames and WIC reads work beyond MAX_PATH without relying on the app manifest.
pub fn wide_path(path: &Path) -> Vec<u16> {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    let mut out = if units.len() < LONG_PATH_UNITS
        || !path.is_absolute()
        || units.starts_with(&utf16(r"\\?\"))
    {
        units
    } else if let Some(rest) = units.strip_prefix(utf16(r"\\").as_slice()) {
        let mut out = utf16(r"\\?\UNC\");
        out.extend_from_slice(rest);
        out
    } else {
        let mut out = utf16(r"\\?\");
        out.extend(units);
        out
    };
    out.push(0);
    out
}
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// The Windows file system as the session's port.
struct Files;
impl MediaFiles for Files {
    fn list(
        &self,
        inputs: &[PathBuf],
        recurse: bool,
        known: &HashSet<String>,
        room: usize,
        cancel: &AtomicBool,
    ) -> Listing {
        scan::list(inputs, recurse, known, room, cancel)
    }
    fn folder_names(&self, dirs: &[PathBuf]) -> HashSet<String> {
        scan::disk_of(dirs)
    }
    fn prober(&self) -> Box<dyn MediaProber> {
        Box::new(probe::Prober::new())
    }
    fn fingerprint(&self, path: &Path) -> Option<Fingerprint> {
        commit::fingerprint(path)
    }
    fn exists(&self, path: &Path) -> bool {
        commit::exists(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        commit::rename(from, to)
    }
    fn read_text(&self, path: &Path, limit: u64) -> io::Result<Option<String>> {
        commit::read_text(path, limit)
    }
    fn replace_text(&self, path: &Path, text: &str) -> io::Result<()> {
        commit::replace_text(path, text)
    }
    fn now_utc(&self) -> UtcTicks {
        clock::now_utc()
    }
    fn local_stamp(&self, at: UtcTicks) -> String {
        clock::local_stamp(at)
    }
}
impl MediaProber for probe::Prober {
    fn probe(&self, path: &Path, kind: Kind) -> Probe {
        probe::Prober::probe(self, path, kind)
    }
    fn dump(&self, path: &Path, kind: Kind) -> Vec<ExifTag> {
        probe::Prober::dump(self, path, kind)
    }
    fn recheck(&self, path: &Path, kind: Kind, known: Fingerprint, readable: bool) -> Recheck {
        scan::recheck(path, kind, known, readable, self)
    }
}

/// A workspace whose workers wake the app window `hwnd`. The self-test and
/// unit tests pass null, which makes the wake a no-op.
pub fn session(hwnd: HWND, prefs: &Preferences) -> Session {
    let hwnd = hwnd as isize;
    Session::new(
        Arc::new(Files),
        Arc::new(move || {
            crate::events::post(hwnd as HWND, powerkit_app::event::Event::RenamerProgress);
        }),
        prefs,
    )
}
