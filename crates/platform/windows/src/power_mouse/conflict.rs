//! Detects other applications that divert the same device controls. Two
//! clients diverting one control fight over it, so the adapter stays off
//! while one of them runs.
#![expect(unsafe_code, reason = "ToolHelp process snapshot")]
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    },
};

/// Executable names (lowercase) and the product to name on the page.
/// `mx4-lite.exe` was read from a running installation; the other two names
/// are the products' agent executables and have not been checked against an
/// installation.
const RIVALS: [(&str, &str); 3] = [
    ("logioptionsplus_agent.exe", "Logi Options+"),
    ("openlogi-agent.exe", "OpenLogi"),
    ("mx4-lite.exe", "MX4Lite"),
];

/// The product name of a running rival, if any.
pub fn running() -> Option<&'static str> {
    rival_among(&process_names())
}

fn rival_among(names: &[String]) -> Option<&'static str> {
    RIVALS
        .iter()
        .find(|(exe, _)| names.iter().any(|name| name.eq_ignore_ascii_case(exe)))
        .map(|(_, product)| *product)
}

fn process_names() -> Vec<String> {
    // SAFETY: a process snapshot takes no pointers; the handle is closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return Vec::new();
    }
    let mut names = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(0),
        ..PROCESSENTRY32W::default()
    };
    // SAFETY: entry is writable with dwSize set; the snapshot is live.
    let mut more = unsafe { Process32FirstW(snapshot, &raw mut entry) } != 0;
    while more {
        let end = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        names.push(String::from_utf16_lossy(&entry.szExeFile[..end]));
        // SAFETY: same snapshot and entry as above.
        more = unsafe { Process32NextW(snapshot, &raw mut entry) } != 0;
    }
    // SAFETY: the snapshot handle is closed once.
    unsafe {
        CloseHandle(snapshot);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rival_is_recognized_by_executable_name_whatever_its_case() {
        let names = |list: &[&str]| {
            list.iter()
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(rival_among(&names(&["explorer.exe", "powerkit.exe"])), None);
        assert_eq!(
            rival_among(&names(&["explorer.exe", "MX4-Lite.exe"])),
            Some("MX4Lite")
        );
        assert_eq!(
            rival_among(&names(&["LogiOptionsPlus_Agent.exe"])),
            Some("Logi Options+")
        );
        // A name that merely contains a rival's name is not one.
        assert_eq!(rival_among(&names(&["not-mx4-lite.exe.bak"])), None);
    }

    #[test]
    fn the_process_list_includes_this_test() {
        assert!(!process_names().is_empty());
    }
}
