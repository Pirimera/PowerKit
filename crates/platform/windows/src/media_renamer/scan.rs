//! Lists MediaRenamer inputs: allow-listed files, same-stem companions, folder walks.
//! Reads directory entries only; never opens file contents.
use super::super::clock;
use super::{commit, probe::Prober};
pub(super) use powerkit_app::controller::media_renamer::lower;
use powerkit_app::controller::media_renamer::{Listing, Recheck};
use powerkit_core::media_renamer::{
    Kind, MAX_BATCH,
    plan::{Member, Probe, split_name},
};
use std::{
    collections::{HashMap, HashSet},
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_OFFLINE,
    FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, GetLongPathNameW,
};

const CLOUD: u32 =
    FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_OFFLINE;

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    name: String,
    attributes: u32,
    size: u64,
    modified: powerkit_core::primitives::UtcTicks,
}
impl Entry {
    fn is_dir(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    }
    fn hidden(&self) -> bool {
        self.attributes & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
    }
}

/// One listed folder, read once and shared by every input that names one of its files.
struct Folder {
    /// Sorted by lower-case name, so folder walks keep a stable order.
    entries: Vec<Entry>,
    /// Lower-case path to the index in `entries`.
    by_path: HashMap<String, usize>,
    /// Lower-case stem to the visible, non-folder entries that carry it (companions).
    by_stem: HashMap<String, Vec<usize>>,
}
impl Folder {
    fn build(mut entries: Vec<Entry>) -> Self {
        entries.sort_by_key(|entry| entry.name.to_lowercase());
        let mut by_path = HashMap::with_capacity(entries.len());
        let mut by_stem: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            by_path.insert(lower(&entry.path), index);
            if entry.is_dir() || entry.hidden() {
                continue;
            }
            if let Some((stem, _, _)) = split_name(&entry.name) {
                by_stem.entry(stem.to_lowercase()).or_default().push(index);
            }
        }
        Self {
            entries,
            by_path,
            by_stem,
        }
    }
}

/// Absolute long-name form without the `\\?\` prefix, so duplicates compare equal.
fn normalize(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let stripped = text
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| text.strip_prefix(r"\\?\").map(str::to_string))
        .unwrap_or_else(|| text.into_owned());
    let absolute = std::path::absolute(&stripped).unwrap_or_else(|_| PathBuf::from(&stripped));
    let wide: Vec<u16> = absolute.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0u16; 32_768];
    // SAFETY: `wide` is NUL-terminated and `buffer` is writable for its full length.
    let length =
        unsafe { GetLongPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) }
            as usize;
    if length == 0 || length >= buffer.len() {
        return absolute;
    }
    PathBuf::from(String::from_utf16_lossy(&buffer[..length]))
}

struct Walker<'a> {
    folders: HashMap<String, Rc<Folder>>,
    seen: HashSet<String>,
    listing: Listing,
    media: usize,
    limit: usize,
    cancel: &'a AtomicBool,
}
impl Walker<'_> {
    fn folder(&mut self, dir: &Path) -> Rc<Folder> {
        let key = lower(dir);
        if let Some(folder) = self.folders.get(&key) {
            return folder.clone();
        }
        let mut entries = Vec::new();
        if let Ok(read) = std::fs::read_dir(dir) {
            for entry in read.flatten() {
                // DirEntry metadata on Windows comes from the directory listing itself.
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                let path = entry.path();
                self.listing.disk.insert(lower(&path));
                entries.push(Entry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path,
                    attributes: metadata.file_attributes(),
                    size: metadata.file_size(),
                    modified: clock::file_time(metadata.last_write_time()),
                });
            }
        }
        let folder = Rc::new(Folder::build(entries));
        self.folders.insert(key, folder.clone());
        folder
    }
    fn add(&mut self, entry: &Entry, companion: bool) -> bool {
        let Some((stem, suffix, kind)) = split_name(&entry.name) else {
            return false;
        };
        if !self.seen.insert(lower(&entry.path)) {
            return true;
        }
        if kind.is_media() {
            if self.media >= self.limit {
                self.listing.truncated = true;
                return true;
            }
            self.media += 1;
        }
        self.listing.members.push(Member {
            path: entry.path.clone(),
            kind,
            stem,
            suffix,
            size: entry.size,
            modified: entry.modified,
            modified_local: clock::local_wall_clock(entry.modified),
            companion: companion || kind == Kind::Sidecar,
            probe: if entry.attributes & CLOUD != 0 {
                Probe::OnlineOnly
            } else {
                Probe::Pending
            },
        });
        true
    }
    fn file(&mut self, path: &Path) {
        let Some(dir) = path.parent() else {
            return;
        };
        let folder = self.folder(dir);
        let Some(&index) = folder.by_path.get(&lower(path)) else {
            return;
        };
        let entry = &folder.entries[index];
        let Some((stem, _, kind)) = split_name(&entry.name) else {
            self.listing.skipped += 1;
            return;
        };
        if !kind.is_media() {
            self.listing.skipped += 1;
            return;
        }
        self.add(entry, false);
        if let Some(siblings) = folder.by_stem.get(&stem.to_lowercase()) {
            for &sibling in siblings {
                self.add(&folder.entries[sibling], true);
            }
        }
    }
    fn tree(&mut self, root: &Path, recurse: bool) {
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            if self.cancel.load(Ordering::Relaxed) || self.listing.truncated {
                return;
            }
            let folder = self.folder(&dir);
            let mut subfolders = Vec::new();
            for entry in &folder.entries {
                if entry.is_dir() {
                    // Junctions and symbolic links are reparse points; never follow them.
                    if recurse
                        && !entry.hidden()
                        && entry.attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                    {
                        subfolders.push(entry.path.clone());
                    }
                    continue;
                }
                if entry.hidden() || !self.add(entry, false) {
                    self.listing.skipped += 1;
                }
            }
            pending.extend(subfolders.into_iter().rev());
        }
    }
}

/// Checks one file against the size and time the preview recorded. A file that is
/// online-only is never read, because reading it would download it.
pub(super) fn recheck(
    path: &Path,
    kind: Kind,
    known: (u64, powerkit_core::primitives::UtcTicks),
    readable: bool,
    prober: &Prober,
) -> Recheck {
    match commit::fingerprint(path) {
        None => Recheck::Gone,
        Some(now) if now == known => Recheck::Unchanged,
        Some((size, modified)) => Recheck::Changed {
            size,
            modified,
            modified_local: clock::local_wall_clock(modified),
            probe: if readable {
                prober.probe(path, kind)
            } else {
                Probe::OnlineOnly
            },
        },
    }
}

/// Lower-case paths of every entry in `dirs`, so collision checks see files that
/// appeared since the preview was made.
pub(super) fn disk_of(dirs: &[PathBuf]) -> HashSet<String> {
    let mut disk = HashSet::new();
    for dir in dirs {
        if let Ok(read) = std::fs::read_dir(dir) {
            disk.extend(read.flatten().map(|entry| lower(&entry.path())));
        }
    }
    disk
}

/// Lists `inputs` (files or folders). `known` holds lower-case paths already in the
/// workspace; `room` is how many more media files the batch may take.
pub(super) fn list(
    inputs: &[PathBuf],
    recurse: bool,
    known: &HashSet<String>,
    room: usize,
    cancel: &AtomicBool,
) -> Listing {
    let mut walker = Walker {
        folders: HashMap::new(),
        seen: known.clone(),
        listing: Listing::default(),
        media: 0,
        limit: room.min(MAX_BATCH),
        cancel,
    };
    for input in inputs {
        if cancel.load(Ordering::Relaxed) || walker.listing.truncated {
            break;
        }
        let path = normalize(input);
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => walker.tree(&path, recurse),
            Ok(_) => walker.file(&path),
            Err(_) => walker.listing.skipped += 1,
        }
    }
    walker.listing
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "powerkit-rename-scan-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // The temp root may be an 8.3 short path; listings use long names.
        normalize(&dir)
    }
    fn names(listing: &Listing) -> Vec<(String, bool)> {
        let mut names: Vec<(String, bool)> = listing
            .members
            .iter()
            .map(|m| {
                (
                    m.path.file_name().unwrap().to_string_lossy().into_owned(),
                    m.companion,
                )
            })
            .collect();
        names.sort();
        names
    }
    #[test]
    fn explicit_files_bring_their_companions() {
        let dir = temp("companions");
        for name in [
            "IMG_1.HEIC",
            "img_1.mov",
            "IMG_1.aae",
            "IMG_1.HEIC.xmp",
            "IMG_2.jpg",
            "notes.txt",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let cancel = AtomicBool::new(false);
        let listing = list(
            &[
                dir.join("IMG_1.HEIC"),
                dir.join("img_1.HEIC"),
                dir.join("notes.txt"),
            ],
            false,
            &HashSet::new(),
            MAX_BATCH,
            &cancel,
        );
        assert_eq!(
            names(&listing),
            [
                ("IMG_1.HEIC".into(), false),
                ("IMG_1.HEIC.xmp".into(), true),
                ("IMG_1.aae".into(), true),
                ("img_1.mov".into(), true)
            ]
        );
        assert_eq!(listing.skipped, 1);
        assert!(listing.disk.contains(&lower(&dir.join("IMG_2.jpg"))));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn folders_skip_unsupported_and_hidden_files_and_honor_recursion_and_limits() {
        let dir = temp("tree");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for name in ["a.jpg", "b.png", "c.txt", "sub\\d.mp4"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let hidden = dir.join("h.jpg");
        std::fs::write(&hidden, b"x").unwrap();
        let wide: Vec<u16> = hidden.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `wide` is a live NUL-terminated path.
        unsafe {
            windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(
                wide.as_ptr(),
                FILE_ATTRIBUTE_HIDDEN,
            )
        };
        let cancel = AtomicBool::new(false);
        let flat = list(
            std::slice::from_ref(&dir),
            false,
            &HashSet::new(),
            MAX_BATCH,
            &cancel,
        );
        assert_eq!(names(&flat).len(), 2);
        assert_eq!(flat.skipped, 2);
        let deep = list(
            std::slice::from_ref(&dir),
            true,
            &HashSet::new(),
            MAX_BATCH,
            &cancel,
        );
        assert_eq!(names(&deep).len(), 3);
        let capped = list(
            std::slice::from_ref(&dir),
            true,
            &HashSet::new(),
            2,
            &cancel,
        );
        assert_eq!(capped.members.len(), 2);
        assert!(capped.truncated);
        let known: HashSet<String> = [lower(&dir.join("a.jpg"))].into();
        let again = list(
            std::slice::from_ref(&dir),
            false,
            &known,
            MAX_BATCH,
            &cancel,
        );
        assert_eq!(names(&again), [("b.png".into(), false)]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
