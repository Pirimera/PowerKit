//! Common file dialogs for PowerStore export/import. Modal on the UI thread.
use super::super::wide;
use std::{mem::size_of, path::PathBuf};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::Controls::Dialogs::{
        GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
        OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    },
};

const FILTER: &str = "JSON\0*.json\0\0";
const MAX_PATH_UNITS: usize = 32_768;

pub fn save_json(owner: HWND, title: &str, default_name: &str) -> Option<PathBuf> {
    let mut buffer = vec![0u16; MAX_PATH_UNITS];
    for (dst, src) in buffer.iter_mut().zip(default_name.encode_utf16()) {
        *dst = src;
    }
    run(
        owner,
        title,
        &mut buffer,
        OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
        true,
    )
}

pub fn open_json(owner: HWND, title: &str) -> Option<PathBuf> {
    let mut buffer = vec![0u16; MAX_PATH_UNITS];
    run(
        owner,
        title,
        &mut buffer,
        OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        false,
    )
}

fn run(owner: HWND, title: &str, buffer: &mut [u16], flags: u32, save: bool) -> Option<PathBuf> {
    let filter = wide(FILTER);
    let title = wide(title);
    let extension = wide("json");
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filter.as_ptr(),
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        lpstrTitle: title.as_ptr(),
        lpstrDefExt: extension.as_ptr(),
        Flags: OFN_EXPLORER | OFN_HIDEREADONLY | flags,
        // SAFETY: OPENFILENAMEW is a plain C struct; zero is valid for every other field.
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: every pointer refers to a live NUL-terminated buffer owned by this frame,
    // and owner is either null or a window on this UI thread.
    let picked = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if picked == 0 {
        return None;
    }
    let length = buffer.iter().position(|unit| *unit == 0)?;
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..length])))
}
