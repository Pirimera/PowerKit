//! Hands a launch's request to the PowerKit that is already running.
//!
//! The request is `powerkit_app::instance::Request`; this module is its
//! Windows transport. The second process sends one `WM_COPYDATA` to the
//! running app window and exits. The receiver only decodes inside that call
//! and posts itself a message, so nothing slow or modal runs while the sender
//! is blocked in `SendMessageW`.
use super::{fault::Fault, wide};
pub use powerkit_app::instance::Request;
use powerkit_core::always_on;
use std::{
    ffi::OsString,
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    System::DataExchange::COPYDATASTRUCT,
    UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_APP, WM_COPYDATA},
};

/// An AlwaysOn request for the app window: wParam and lParam are the pair
/// from [`always_on_message`].
pub const ALWAYS_ON: u32 = WM_APP + 15;
/// Marks a `WM_COPYDATA` as a PowerKit request ("PKR1").
const COPY_ID: usize = 0x504B_5231;
const OPEN: u8 = 0;
const AWAKE: u8 = 1;
const RESIZE: u8 = 2;
const RENAME: u8 = 3;

/// The window-message form of an AlwaysOn request: (kind, value).
pub fn always_on_message(request: always_on::Request) -> (usize, isize) {
    match request {
        always_on::Request::Off => (0, 0),
        always_on::Request::Indefinite => (1, 0),
        always_on::Request::Minutes(minutes) => (2, minutes as isize),
        always_on::Request::Process(pid) => (3, pid as isize),
    }
}
/// `None` for a pair that is not a request; minutes are clamped to the
/// valid interval.
pub fn always_on_from_message(kind: usize, value: isize) -> Option<always_on::Request> {
    let value = u32::try_from(value).ok();
    match kind {
        0 => Some(always_on::Request::Off),
        1 => Some(always_on::Request::Indefinite),
        2 => value.filter(|minutes| *minutes > 0).map(|minutes| {
            always_on::Request::Minutes(always_on::clamp_interval(i64::from(minutes)))
        }),
        3 => value
            .filter(|pid| *pid != 0)
            .map(always_on::Request::Process),
        _ => None,
    }
}
/// One tag byte, then the body: nothing for Open, a kind byte and a
/// little-endian `u32` for AlwaysOn, NUL-terminated UTF-16LE paths otherwise.
pub fn encode(request: &Request) -> Vec<u8> {
    let paths = |tag: u8, paths: &[PathBuf]| {
        let mut bytes = vec![tag];
        for path in paths {
            for unit in path.as_os_str().encode_wide().chain(Some(0)) {
                bytes.extend(unit.to_le_bytes());
            }
        }
        bytes
    };
    match request {
        Request::Open => vec![OPEN],
        Request::AlwaysOn(request) => {
            let (kind, value) = always_on_message(*request);
            let mut bytes = vec![AWAKE, kind as u8];
            bytes.extend((value as u32).to_le_bytes());
            bytes
        }
        Request::Resize(files) => paths(RESIZE, files),
        Request::Rename(files) => paths(RENAME, files),
    }
}
/// `None` for anything that is not a well-formed request. Path lists are cut
/// at the batch limit of their tool.
pub fn decode(bytes: &[u8]) -> Option<Request> {
    let (&tag, body) = bytes.split_first()?;
    let paths = |limit: usize| -> Option<Vec<PathBuf>> {
        let (units, rest) = body.as_chunks::<2>();
        if !rest.is_empty() {
            return None;
        }
        let units: Vec<u16> = units.iter().map(|unit| u16::from_le_bytes(*unit)).collect();
        Some(
            units
                .split(|unit| *unit == 0)
                .filter(|path| !path.is_empty())
                .take(limit)
                .map(|path| PathBuf::from(OsString::from_wide(path)))
                .collect(),
        )
    };
    match tag {
        OPEN if body.is_empty() => Some(Request::Open),
        AWAKE => {
            let (&kind, value) = body.split_first()?;
            let value = u32::from_le_bytes(value.try_into().ok()?);
            always_on_from_message(usize::from(kind), isize::try_from(value).ok()?)
                .map(Request::AlwaysOn)
        }
        RESIZE => paths(powerkit_core::image_resizer::MAX_BATCH).map(Request::Resize),
        RENAME => paths(powerkit_core::media_renamer::MAX_BATCH).map(Request::Rename),
        _ => None,
    }
}
/// Sends `request` to the running instance and waits until it has taken it.
pub fn forward(request: &Request) -> io::Result<()> {
    let hwnd = find_main();
    if hwnd.is_null() {
        return Err(Fault::NoRunningInstance.into());
    }
    let mut bytes = encode(request);
    let mut copy = COPYDATASTRUCT {
        dwData: COPY_ID,
        cbData: bytes.len() as u32,
        lpData: bytes.as_mut_ptr().cast(),
    };
    // SAFETY: `bytes` and `copy` stay alive until SendMessageW returns, and
    // the receiver copies the buffer before it answers.
    let accepted = unsafe {
        SendMessageW(
            hwnd,
            WM_COPYDATA,
            0,
            (&mut copy as *mut COPYDATASTRUCT) as LPARAM,
        )
    };
    if accepted == 0 {
        Err(Fault::ForwardFailed.into())
    } else {
        Ok(())
    }
}
/// The request in a `WM_COPYDATA` lParam, or `None` when another program sent
/// the message or the payload is malformed.
///
/// # Safety
/// `l` must be the lParam of a `WM_COPYDATA` being handled: Windows keeps the
/// sender's `COPYDATASTRUCT` and its buffer valid until the handler returns.
pub unsafe fn accept(l: LPARAM) -> Option<Request> {
    if l == 0 {
        return None;
    }
    // SAFETY: see the function contract.
    let data = unsafe { &*(l as *const COPYDATASTRUCT) };
    if data.dwData != COPY_ID || data.lpData.is_null() || data.cbData == 0 {
        return None;
    }
    // SAFETY: lpData points at cbData readable bytes for the same duration.
    let bytes =
        unsafe { std::slice::from_raw_parts(data.lpData.cast::<u8>(), data.cbData as usize) };
    decode(bytes)
}
/// The running instance's app window. A PowerKit that is still starting has
/// the singleton but no window yet, so this waits briefly for it.
fn find_main() -> HWND {
    let class = wide("PowerKit.Window");
    for _ in 0..30 {
        // SAFETY: the class name is a live NUL-terminated buffer.
        let hwnd = unsafe { FindWindowW(class.as_ptr(), null()) };
        if !hwnd.is_null() {
            return hwnd;
        }
        thread::sleep(Duration::from_millis(50));
    }
    null_mut()
}
/// Reads and deletes a path list the shell DLL wrote for a long selection. An
/// unreadable list yields no paths.
pub fn take_path_list(path: &Path, limit: usize) -> Vec<PathBuf> {
    let paths = read_path_list(path, limit).unwrap_or_default();
    let _ = std::fs::remove_file(path);
    paths
}
/// A UTF-16LE (BOM) newline-separated path list.
pub fn read_path_list(path: &Path, limit: usize) -> io::Result<Vec<PathBuf>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > limit.saturating_mul(1_024).max(1_048_576) {
        return Err(io::Error::other("path list is too large"));
    }
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xFE {
        return Err(io::Error::other("path list is not valid UTF-16"));
    }
    let (chunks, _) = bytes[2..].as_chunks::<2>();
    let units: Vec<u16> = chunks
        .iter()
        .map(|chunk| u16::from_le_bytes(*chunk))
        .collect();
    let mut files = Vec::new();
    for line in units.split(|unit| *unit == u16::from(b'\n') || *unit == u16::from(b'\r')) {
        if line.is_empty() || files.len() == limit {
            continue;
        }
        files.push(PathBuf::from(OsString::from_wide(line)));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_survives_the_wire() {
        for request in [
            Request::Open,
            Request::AlwaysOn(always_on::Request::Off),
            Request::AlwaysOn(always_on::Request::Indefinite),
            Request::AlwaysOn(always_on::Request::Minutes(45)),
            Request::AlwaysOn(always_on::Request::Process(4242)),
            Request::Resize(Vec::new()),
            Request::Resize(vec![PathBuf::from(r"C:\photos\a b.jpg")]),
            Request::Rename(vec![
                PathBuf::from(r"C:\medya\şömine.mov"),
                PathBuf::from(r"D:\x"),
            ]),
        ] {
            assert_eq!(decode(&encode(&request)), Some(request));
        }
    }

    #[test]
    fn always_on_requests_survive_the_window_message() {
        for request in [
            always_on::Request::Off,
            always_on::Request::Indefinite,
            always_on::Request::Minutes(45),
            always_on::Request::Process(7),
        ] {
            let (kind, value) = always_on_message(request);
            assert_eq!(always_on_from_message(kind, value), Some(request));
        }
        assert_eq!(always_on_from_message(2, 0), None);
        assert_eq!(always_on_from_message(3, -1), None);
        assert_eq!(always_on_from_message(9, 1), None);
    }

    #[test]
    fn malformed_payloads_are_not_requests() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[9]), None);
        // Open carries nothing.
        assert_eq!(decode(&[OPEN, 1]), None);
        // AlwaysOn needs its kind and four value bytes, and a valid pair.
        assert_eq!(decode(&[AWAKE]), None);
        assert_eq!(decode(&[AWAKE, 2, 5, 0]), None);
        assert_eq!(decode(&[AWAKE, 2, 0, 0, 0, 0]), None);
        assert_eq!(decode(&[AWAKE, 7, 1, 0, 0, 0]), None);
        // Half a UTF-16 unit.
        assert_eq!(decode(&[RESIZE, b'a']), None);
    }

    #[test]
    fn a_path_list_is_read_as_utf16_and_then_removed() {
        let path =
            std::env::temp_dir().join(format!("powerkit-list-test-{}.txt", std::process::id()));
        let mut bytes = vec![0xFF, 0xFE];
        for line in [
            "C:\\photos\\a.jpg",
            "",
            "C:\\photos\\b.png",
            "C:\\c d\\e.mov",
        ] {
            for unit in line.encode_utf16().chain(Some(u16::from(b'\n'))) {
                bytes.extend(unit.to_le_bytes());
            }
        }
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            read_path_list(&path, 2).unwrap(),
            [
                PathBuf::from(r"C:\photos\a.jpg"),
                PathBuf::from(r"C:\photos\b.png")
            ]
        );
        assert_eq!(take_path_list(&path, 10).len(), 3);
        assert!(!path.exists());
        assert!(read_path_list(Path::new("missing-powerkit-list.txt"), 10).is_err());
        assert_eq!(
            take_path_list(Path::new("missing-powerkit-list.txt"), 10),
            Vec::<PathBuf>::new()
        );
    }
}
