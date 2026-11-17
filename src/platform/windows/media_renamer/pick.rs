//! File and folder pickers for MediaRenamer. Modal on the UI thread that owns `owner`.
use super::super::wide;
use std::{mem::size_of, path::PathBuf};
use windows::{
    Win32::{
        Foundation::HWND as WinHwnd,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
        UI::Shell::{
            FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
            IFileOpenDialog, SIGDN_FILESYSPATH,
        },
    },
    core::PCWSTR,
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    },
};

pub(in crate::platform::windows) fn files(
    owner: HWND,
    title: &str,
    filter_label: &str,
) -> Vec<PathBuf> {
    let mut buffer = vec![0u16; 65_536];
    let filter = wide(&format!(
        "{filter_label}\0{}\0\0",
        crate::media_renamer::picker_pattern()
    ));
    let title = wide(title);
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filter.as_ptr(),
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        lpstrTitle: title.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_ALLOWMULTISELECT
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_HIDEREADONLY,
        // SAFETY: OPENFILENAMEW is a plain C struct; zero is valid for every other field.
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: every pointer refers to a live buffer owned by this frame; owner is null or
    // a window on this thread.
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        return Vec::new();
    }
    split(&buffer)
}
/// Multi-select output: one full path, or the folder followed by file names.
fn split(buffer: &[u16]) -> Vec<PathBuf> {
    let parts: Vec<String> = buffer
        .split(|unit| *unit == 0)
        .take_while(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect();
    match parts.as_slice() {
        [] => Vec::new(),
        [one] => vec![PathBuf::from(one)],
        [folder, names @ ..] => names
            .iter()
            .map(|name| PathBuf::from(folder).join(name))
            .collect(),
    }
}
/// The Windows folder picker. COM is already initialized on the WinUI thread.
pub(in crate::platform::windows) fn folder(owner: HWND, title: &str) -> Option<PathBuf> {
    let title = wide(title);
    // SAFETY: the dialog and the returned item stay on this thread; the display-name
    // string is copied and freed with CoTaskMemFree before return.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .ok()?;
        let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
        dialog.Show(Some(WinHwnd(owner))).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0.cast()));
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picker_output_splits_into_paths() {
        let encode = |text: &str| -> Vec<u16> { text.encode_utf16().collect() };
        let mut single = encode(r"C:\a\b.jpg");
        single.extend([0, 0]);
        assert_eq!(split(&single), [PathBuf::from(r"C:\a\b.jpg")]);
        let mut many = encode(r"C:\a");
        many.push(0);
        many.extend(encode("x.jpg"));
        many.push(0);
        many.extend(encode("y.mov"));
        many.extend([0, 0]);
        assert_eq!(
            split(&many),
            [PathBuf::from(r"C:\a\x.jpg"), PathBuf::from(r"C:\a\y.mov")]
        );
        assert!(split(&[0, 0]).is_empty());
    }
}
