//! Foreground elevation watch. While the foreground window belongs to an
//! elevated process, PowerMouse passes every trigger through.
#![expect(unsafe_code, reason = "WinEvent hooks and token elevation queries")]
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HANDLE, HWND},
    Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
    System::Threading::{
        GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetWindowThreadProcessId,
            WINEVENT_OUTOFCONTEXT,
        },
    },
};

static ELEVATED: AtomicBool = AtomicBool::new(false);

pub fn is_elevated_foreground() -> bool {
    ELEVATED.load(Ordering::Acquire)
}

pub struct Watch {
    hook: isize,
}

impl Watch {
    pub fn start() -> Self {
        refresh();
        // Own-process events are not skipped: switching from an elevated
        // window to a PowerKit window must clear the flag too.
        // SAFETY: callback only touches the process-wide atomic and Win32 queries.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        Self {
            hook: hook as isize,
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if self.hook != 0 {
            // SAFETY: hook was installed by SetWinEventHook on this thread and is unhooked once.
            unsafe {
                UnhookWinEvent(self.hook as _);
            }
            self.hook = 0;
        }
        ELEVATED.store(false, Ordering::Release);
    }
}

unsafe extern "system" fn on_foreground(
    _: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    refresh();
}

fn refresh() {
    let elevated = foreground_elevated();
    // Adapters undivert their device controls while an elevated window is
    // in front; tell them only when the state actually changed.
    if ELEVATED.swap(elevated, Ordering::AcqRel) != elevated {
        super::elevation_changed(elevated);
    }
}

fn foreground_elevated() -> bool {
    // SAFETY: returns the foreground window or null.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return false;
    }
    let mut pid = 0;
    // SAFETY: hwnd is non-null; pid is a valid out-pointer.
    unsafe {
        GetWindowThreadProcessId(hwnd, &raw mut pid);
    }
    if pid == 0 {
        return false;
    }
    // SAFETY: own process id; no pointers.
    if pid == unsafe { GetCurrentProcessId() } {
        return false;
    }
    process_elevated(pid)
}

fn process_elevated(pid: u32) -> bool {
    // SAFETY: OpenProcess takes the PID by value.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        // Access denied often means a higher-integrity process; treat as elevated.
        // SAFETY: reads the calling thread error slot.
        return unsafe { GetLastError() } == 5; // ERROR_ACCESS_DENIED
    }
    let elevated = token_elevated(process);
    // SAFETY: process was opened above and is closed once.
    unsafe {
        CloseHandle(process);
    }
    elevated
}

fn token_elevated(process: HANDLE) -> bool {
    let mut token = std::ptr::null_mut();
    // SAFETY: process is a live handle; token receives an owned handle.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) } == 0 {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0;
    // SAFETY: correctly sized TOKEN_ELEVATION buffer.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&raw mut elevation).cast(),
            size_of_elevation(),
            &raw mut returned,
        )
    } != 0;
    // SAFETY: token was opened above and is closed once.
    unsafe {
        CloseHandle(token);
    }
    ok && elevation.TokenIsElevated != 0
}

fn size_of_elevation() -> u32 {
    u32::try_from(std::mem::size_of::<TOKEN_ELEVATION>()).unwrap_or(0)
}
