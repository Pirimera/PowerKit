//! Explorer and second-instance handoff for MediaRenamer: `--rename` arguments, the
//! `--rename-list` file the shell DLL writes for long selections, WM_COPYDATA and the
//! classic per-user context-menu verbs.
use super::super::resize;
use crate::{media_extensions, media_renamer::MAX_BATCH};
use std::{
    io,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{System::DataExchange::COPYDATASTRUCT, UI::WindowsAndMessaging::WM_APP};

/// Posted to the app window after WM_COPYDATA so confirmation runs outside SendMessage.
pub(in crate::platform::windows) const HANDOFF: u32 = WM_APP + 11;
const COPY_ID: usize = 0x524E_4D31;
const VERB_KEY: &str = "PowerKit.MediaRenamer";

fn parse_args(args: &[String]) -> Option<Vec<PathBuf>> {
    if let Some(index) = args.iter().position(|arg| arg == "--rename-list") {
        let list = args.get(index + 1)?;
        let paths = resize::read_path_list(Path::new(list), MAX_BATCH).unwrap_or_default();
        let _ = std::fs::remove_file(list);
        return Some(paths);
    }
    let index = args.iter().position(|arg| arg == "--rename")?;
    Some(
        args.iter()
            .skip(index + 1)
            .take_while(|arg| !arg.starts_with("--"))
            .filter(|arg| !arg.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}
/// Paths from this process's `--rename` or `--rename-list` arguments.
pub(in crate::platform::windows) fn command_paths() -> Option<Vec<PathBuf>> {
    parse_args(&std::env::args().skip(1).collect::<Vec<_>>())
}
pub(in crate::platform::windows) fn forward(paths: &[PathBuf]) -> io::Result<()> {
    resize::forward_with(COPY_ID, paths)
}
/// Paths carried by a MediaRenamer WM_COPYDATA, or `None` for another sender.
pub(in crate::platform::windows) fn accept(data: &COPYDATASTRUCT) -> Option<Vec<PathBuf>> {
    (data.dwData == COPY_ID).then(|| resize::paths_from_utf16(data))
}
/// Classic verbs on media files and folders; Windows 11 shows the sparse-package verb.
pub(in crate::platform::windows) fn register_context_menu() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    let dll = dir.join("powerkit_shell.dll");
    let clsid = format!(
        "{{{}}}",
        include_str!("../../../shell_clsid_rename.txt").trim()
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_arguments_and_lists_are_read() {
        let args = |items: &[&str]| {
            items
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            parse_args(&args(&["--rename", r"C:\a\b.jpg", r"C:\a\photos"])),
            Some(vec![
                PathBuf::from(r"C:\a\b.jpg"),
                PathBuf::from(r"C:\a\photos")
            ])
        );
        assert_eq!(parse_args(&args(&["--resize", "x.jpg"])), None);
        assert_eq!(parse_args(&args(&["--rename"])), Some(Vec::new()));
        let list = std::env::temp_dir().join(format!(
            "powerkit-rename-list-test-{}.txt",
            std::process::id()
        ));
        let mut bytes = vec![0xFF, 0xFE];
        for line in [r"C:\a\b.jpg", r"C:\c d\e.mov"] {
            for unit in line.encode_utf16().chain(Some(u16::from(b'\n'))) {
                bytes.extend(unit.to_le_bytes());
            }
        }
        std::fs::write(&list, bytes).unwrap();
        assert_eq!(
            parse_args(&args(&["--rename-list", &list.display().to_string()])),
            Some(vec![
                PathBuf::from(r"C:\a\b.jpg"),
                PathBuf::from(r"C:\c d\e.mov")
            ])
        );
        assert!(!list.exists());
    }
}
