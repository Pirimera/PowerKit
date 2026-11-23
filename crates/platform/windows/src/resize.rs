//! Image Resizer on Windows: the Explorer verb, the files another launch handed
//! over (`instance.rs` receives them), the file picker and the worker thread that runs a `powerkit_app` resize job over
//! the WIC codec. The dialog itself is WinUI (`resize_dialog.rs`); without the
//! Windows App Runtime there is no resize UI.
use super::{resize_wic::Images, wide};
use powerkit_app::controller::image_resizer::Job;
use std::{cell::RefCell, mem::size_of, path::PathBuf, ptr::null, thread};
use windows_sys::Win32::{
    Foundation::HWND,
    System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW},
};
pub const SHOW: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 8;
thread_local! {
    static PENDING: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}
/// Remembers files received from another launch until `SHOW` opens the dialog.
pub fn queue(files: Vec<PathBuf>) {
    PENDING.with(|pending| *pending.borrow_mut() = Some(files));
}
/// The files a launch handed over, once.
pub fn take_pending() -> Option<Vec<PathBuf>> {
    PENDING.with(|pending| pending.borrow_mut().take())
}
pub fn register_context_menu() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    let exe = exe.display().to_string().replace('"', "");
    let base = "Software\\Classes\\SystemFileAssociations\\image\\shell\\PowerKit.ImageResizer";
    let _ = set_sz(base, None, super::ui_strings().resize_verb);
    let _ = set_sz(base, Some("MultiSelectModel"), "Document");
    let _ = set_sz(base, Some("Icon"), &format!("\"{exe}\",0"));
    let _ = set_sz(
        &format!("{base}\\command"),
        None,
        &format!("\"{exe}\" --resize \"%1\""),
    );
    let dll = dir.join("powerkit_shell.dll");
    let clsid = format!(
        "{{{}}}",
        include_str!("../../../../packaging/msix/shell_clsid.txt").trim()
    );
    if dll.is_file() {
        let _ = set_sz(base, Some("DelegateExecute"), &clsid);
        let key = format!("Software\\Classes\\CLSID\\{clsid}");
        let _ = set_sz(&key, None, "PowerKit Image Resizer");
        let server = format!("{key}\\InprocServer32");
        let _ = set_sz(&server, None, &dll.display().to_string().replace('"', ""));
        let _ = set_sz(&server, Some("ThreadingModel"), "Apartment");
        super::shell_package::start(dir.to_path_buf());
    } else {
        delete_value(base, "DelegateExecute");
    }
}
pub fn delete_value(subkey: &str, name: &str) {
    let subkey = wide(subkey);
    let name = wide(name);
    // SAFETY: both strings are NUL-terminated and live for this call. No key handle is returned.
    unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), name.as_ptr());
    }
}
pub fn set_sz(subkey: &str, name: Option<&str>, value: &str) -> bool {
    let subkey = wide(subkey);
    let name = name.map(wide);
    let mut value = wide(value);
    // SAFETY: registry strings are NUL-terminated and live for this call. No key handle is returned.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ref().map_or(null(), |name| name.as_ptr()),
            REG_SZ,
            value.as_mut_ptr().cast(),
            (value.len() * 2) as u32,
        )
    };
    status == 0
}
/// Runs `job` on a worker thread. Every progress report and the end of the job
/// raise `Event::ResizeProgress` for the app window `notify`, which refreshes
/// the dialog.
pub fn spawn(notify: HWND, job: Job) {
    let notify = notify as isize;
    thread::spawn(move || {
        powerkit_app::controller::image_resizer::run(Images::new, &job, &|| {
            crate::events::post(notify as HWND, powerkit_app::event::Event::ResizeProgress);
        });
    });
}
/// Asks for the images to resize; empty when the user cancels.
pub fn pick(owner: HWND) -> Vec<PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut buffer = vec![0u16; 32_768];
    let t = super::ui_strings();
    let filter = wide(&format!(
        "{}\0*.bmp;*.dib;*.gif;*.heic;*.jpg;*.jpeg;*.jpe;*.png;*.tif;*.tiff;*.wdp;*.webp\0\0",
        t.resize_picker_filter
    ));
    let title = wide(t.resize_picker_title);
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
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        return Vec::new();
    }
    split_picker(&buffer)
}
fn split_picker(buffer: &[u16]) -> Vec<PathBuf> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, unit) in buffer.iter().enumerate() {
        if *unit == 0 {
            if index == start {
                break;
            }
            parts.push(String::from_utf16_lossy(&buffer[start..index]));
            start = index + 1;
        }
    }
    match parts.as_slice() {
        [] => Vec::new(),
        [one] => vec![PathBuf::from(one)],
        [directory, names @ ..] => names
            .iter()
            .map(|name| PathBuf::from(directory).join(name))
            .collect(),
    }
}
