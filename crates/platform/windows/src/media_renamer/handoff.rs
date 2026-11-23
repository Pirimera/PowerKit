//! Explorer handoff for MediaRenamer: the message that confirms a received
//! selection outside `SendMessage` and the classic per-user context-menu verbs.
//! The request itself arrives through `instance.rs`.
use super::super::resize;
use powerkit_core::media_extensions;
use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

/// Posted to the app window after WM_COPYDATA so confirmation runs outside SendMessage.
pub const HANDOFF: u32 = WM_APP + 11;
const VERB_KEY: &str = "PowerKit.MediaRenamer";

/// Classic verbs on media files and folders; Windows 11 shows the sparse-package verb.
pub fn register_context_menu() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    let dll = dir.join("powerkit_shell.dll");
    let clsid = format!(
        "{{{}}}",
        include_str!("../../../../../packaging/msix/shell_clsid_rename.txt").trim()
    );
    let exe = exe.display().to_string().replace('"', "");
    let title = super::super::ui_strings().rename.verb;
    let bases = media_extensions::media()
        .map(|extension| format!("Software\\Classes\\SystemFileAssociations\\.{extension}"))
        .chain(["Software\\Classes\\Directory".to_string()]);
    for base in bases {
        let key = format!("{base}\\shell\\{VERB_KEY}");
        let _ = resize::set_sz(&key, None, title);
        let _ = resize::set_sz(&key, Some("MultiSelectModel"), "Player");
        let _ = resize::set_sz(&key, Some("Icon"), &format!("\"{exe}\",0"));
        let _ = resize::set_sz(
            &format!("{key}\\command"),
            None,
            &format!("\"{exe}\" --rename \"%1\""),
        );
        if dll.is_file() {
            let _ = resize::set_sz(&key, Some("DelegateExecute"), &clsid);
        } else {
            resize::delete_value(&key, "DelegateExecute");
        }
    }
    if dll.is_file() {
        let key = format!("Software\\Classes\\CLSID\\{clsid}");
        let _ = resize::set_sz(&key, None, "PowerKit MediaRenamer");
        let server = format!("{key}\\InprocServer32");
        let _ = resize::set_sz(&server, None, &dll.display().to_string().replace('"', ""));
        let _ = resize::set_sz(&server, Some("ThreadingModel"), "Apartment");
    }
}
