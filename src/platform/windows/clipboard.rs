//! Plain-text clipboard writes for ScreenRuler measurements and PowerStore ids.
//! Runs on the UI thread; the clipboard is held only inside [`set_text`].
use std::{io, thread, time::Duration};
use windows_sys::Win32::{
    Foundation::{GlobalFree, HWND},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
    },
};

/// `CF_UNICODETEXT` (13). `windows-sys` keeps it behind the `Win32_System_Ole`
/// feature, which nothing else here needs.
const CF_UNICODETEXT: u32 = 13;
/// Another app can hold the clipboard briefly (clipboard managers, RDP); retry for
/// up to about 100 ms before giving up.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY: Duration = Duration::from_millis(10);

/// Closes the clipboard when the write ends, on every path.
struct Opened;
impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: only created after OpenClipboard succeeded on this thread.
        unsafe {
            CloseClipboard();
        }
    }
}

/// Replaces the clipboard with `text`. `owner` (may be null) becomes the clipboard
/// owner. An empty `text` leaves the clipboard untouched.
pub(super) fn set_text(owner: HWND, text: &str) -> io::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let _opened = open(owner)?;
    // SAFETY: the clipboard is open on this thread until `_opened` drops. The
    // global block is sized for `wide` including its NUL, written only while
    // locked, and freed here unless SetClipboardData took ownership of it.
    unsafe {
        if EmptyClipboard() == 0 {
            return Err(io::Error::last_os_error());
        }
        let handle = GlobalAlloc(GMEM_MOVEABLE, wide.len() * size_of::<u16>());
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let locked = GlobalLock(handle).cast::<u16>();
        if locked.is_null() {
            let error = io::Error::last_os_error();
            GlobalFree(handle);
            return Err(error);
        }
        locked.copy_from_nonoverlapping(wide.as_ptr(), wide.len());
        GlobalUnlock(handle);
        if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
            let error = io::Error::last_os_error();
            GlobalFree(handle);
            return Err(error);
        }
    }
    Ok(())
}

fn open(owner: HWND) -> io::Result<Opened> {
    for attempt in 1..=OPEN_ATTEMPTS {
        // SAFETY: plain Win32 call; a null owner is allowed.
        if unsafe { OpenClipboard(owner) } != 0 {
            return Ok(Opened);
        }
        if attempt < OPEN_ATTEMPTS {
            thread::sleep(OPEN_RETRY);
        }
    }
    Err(io::Error::last_os_error())
}
