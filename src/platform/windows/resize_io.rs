//! Commit a finished resize: copies land on a new name, replacements keep a restorable backup.
use crate::image_resizer::PlannedWrite;
use std::{fs, io, path::Path, time::SystemTime};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
        SHFileOperationW,
    },
};
pub(super) fn commit(
    temp: &Path,
    plan: &PlannedWrite,
    modified: Option<SystemTime>,
    recycle: bool,
) -> io::Result<()> {
    if plan.replace {
        let backup = plan
            .backup
            .as_ref()
            .ok_or_else(|| io::Error::other("Yedek yolu yok"))?;
        // The original moves aside before the temp takes its name, so a failed swap can restore it.
        fs::rename(&plan.source, backup)?;
        if let Err(error) = fs::rename(temp, &plan.destination) {
            let _ = fs::rename(backup, &plan.source);
            return Err(error);
        }
        apply_time(&plan.destination, modified)?;
        if recycle {
            let _ = recycle_file(backup);
        }
        return Ok(());
    }
    if plan.destination.exists() {
        return Err(super::fault::Fault::TargetAppeared.into());
    }
    fs::rename(temp, &plan.destination)?;
    apply_time(&plan.destination, modified)
}
fn apply_time(path: &Path, modified: Option<SystemTime>) -> io::Result<()> {
    let Some(modified) = modified else {
        return Ok(());
    };
    let file = fs::File::options().write(true).open(path)?;
    file.set_modified(modified)
}
fn recycle_file(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let mut from: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut operation = SHFILEOPSTRUCTW {
        hwnd: HWND::default(),
        wFunc: FO_DELETE,
        pFrom: from.as_mut_ptr(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as u16,
        ..Default::default()
    };
    // SAFETY: pFrom is a double-NUL path that stays alive for this call. Recycle does not take ownership of the buffer.
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code == 0 && operation.fAnyOperationsAborted == 0 && !path.exists() {
        Ok(())
    } else {
        Err(super::fault::Fault::BackupNotRecycled.into())
    }
}
pub(super) struct Staging(pub(super) std::path::PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
impl Staging {
    pub(super) fn create(directory: &Path) -> io::Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let name = format!(
            ".powerkit-resize-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let path = directory.join(name);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self(path))
    }
    #[cfg(test)]
    pub(super) fn keep(self) {
        std::mem::forget(self);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_resizer::{Container, Encoder, FramePlan, PlannedWrite};
    fn plan(dir: &Path, replace: bool) -> PlannedWrite {
        let source = dir.join("a.txt");
        PlannedWrite {
            destination: if replace {
                source.clone()
            } else {
                dir.join("b.txt")
            },
            backup: replace.then(|| dir.join("a.bak.txt")),
            source,
            replace,
            encoder: Encoder::Png,
            frame: FramePlan {
                scaled_width: 1,
                scaled_height: 1,
                crop: None,
                output_width: 1,
                output_height: 1,
                unchanged: true,
            },
            container: Container::Png,
            source_width: 1,
            source_height: 1,
        }
    }
    #[test]
    fn replace_keeps_the_original_until_the_new_file_is_in_place() {
        let dir = std::env::temp_dir().join(format!("powerkit-resize-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let planned = plan(&dir, true);
        fs::write(&planned.source, b"old").unwrap();
        let staging = Staging::create(&dir).unwrap();
        fs::write(&staging.0, b"new").unwrap();
        commit(&staging.0, &planned, None, false).unwrap();
        staging.keep();
        assert_eq!(fs::read(&planned.destination).unwrap(), b"new");
        assert_eq!(fs::read(planned.backup.as_ref().unwrap()).unwrap(), b"old");
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn failed_replace_restores_the_original() {
        let dir = std::env::temp_dir().join(format!("powerkit-resize-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let planned = plan(&dir, true);
        fs::write(&planned.source, b"old").unwrap();
        fs::create_dir(planned.backup.as_ref().unwrap()).unwrap();
        let staging = Staging::create(&dir).unwrap();
        fs::write(&staging.0, b"new").unwrap();
        assert!(commit(&staging.0, &planned, None, false).is_err());
        assert_eq!(fs::read(&planned.source).unwrap(), b"old");
        assert!(staging.0.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
