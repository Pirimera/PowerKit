//! Media extensions MediaRenamer accepts, shared by the app and the Explorer shell DLL.
pub const JPEG: &[&str] = &["jpg", "jpeg", "jpe"];
/// `hif` is the extension Canon, Sony and Fujifilm cameras write for HEIF stills.
pub const HEIF: &[&str] = &["heic", "heif", "hif"];
pub const RAW: &[&str] = &[
    "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef",
    "srw", "x3f", "3fr", "iiq", "mrw", "erf", "kdc", "dcr", "mos", "rwl",
];
pub const VIDEO: &[&str] = &["mp4", "mov", "m4v", "3gp"];
/// Lower-case media extensions (no sidecars), in picker order.
pub fn media() -> impl Iterator<Item = &'static str> {
    JPEG.iter()
        .chain(HEIF)
        .chain(&["png", "webp"])
        .chain(VIDEO)
        .chain(&["avi"])
        .chain(RAW)
        .copied()
}
/// Only the shell DLL filters by extension; the app maps extensions through `Kind`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_media(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    media().any(|known| known == lower)
}
