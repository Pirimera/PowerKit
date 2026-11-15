//! Win32 side of WinUI windows owned by windows-reactor.
//!
//! The settings shell is the first top-level WinUI window: a CBT hook records its
//! HWND, and a subclass keeps tray semantics (start hidden, close hides, exit
//! closes). Extra windows such as Image Resizer are not subclassed; they only get
//! a `DIALOG_CREATED` so the app can copy icons and bring them forward. Everything
//! here runs on the UI thread that also owns the hidden app window and power request.
use super::resize;
use std::{cell::Cell, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
/// wParam is the settings WinUI HWND, posted once so the app can apply its icons.
pub(super) const CREATED: u32 = WM_APP + 2;
/// The user closed the settings window; the app decides between hide and exit.
pub(super) const CLOSE_REQUESTED: u32 = WM_APP + 3;
/// The settings window lost activation, which cancels a hotkey capture.
pub(super) const DEACTIVATED: u32 = WM_APP + 4;
/// wParam is the captured virtual key, lParam the `MOD_*` flags held with it.
pub(super) const CAPTURED_KEY: u32 = WM_APP + 5;
/// wParam is a secondary WinUI HWND (Image Resizer); icons + foreground only.
pub(super) const DIALOG_CREATED: u32 = WM_APP + 6;
/// Posted after `DIALOG_CREATED` so activation runs once the window is mapped.
pub(super) const DIALOG_PRESENT: u32 = WM_APP + 7;
pub(super) const MOD_WIN: isize = 1;
pub(super) const MOD_CTRL: isize = 2;
pub(super) const MOD_ALT: isize = 4;
pub(super) const MOD_SHIFT: isize = 8;
const WINUI_CLASS: &str = "WinUIDesktopWin32WindowClass";
const SUBCLASS_ID: usize = 1;
const DIALOG_SUBCLASS_ID: usize = 2;
thread_local! {
    static APP: Cell<HWND> = const { Cell::new(null_mut()) };
    static WINDOW: Cell<HWND> = const { Cell::new(null_mut()) };
    static DIALOG: Cell<HWND> = const { Cell::new(null_mut()) };
    static HIDDEN: Cell<bool> = const { Cell::new(false) };
    static EXITING: Cell<bool> = const { Cell::new(false) };
    static CBT: Cell<HHOOK> = const { Cell::new(null_mut()) };
    static MESSAGES: Cell<HHOOK> = const { Cell::new(null_mut()) };
    static KEYS: Cell<HHOOK> = const { Cell::new(null_mut()) };
}
/// Owns the thread hooks for the lifetime of the WinUI loop.
pub(super) struct Host;
impl Drop for Host {
    fn drop(&mut self) {
        end_key_capture();
        for hook in [&CBT, &MESSAGES] {
            let handle = hook.with(|hook| hook.replace(null_mut()));
            if !handle.is_null() {
                // SAFETY: the handle came from SetWindowsHookExW on this thread.
                unsafe {
                    UnhookWindowsHookEx(handle);
                }
            }
        }
    }
}
/// Starts watching this thread for the WinUI window. `start_hidden` keeps it off
/// screen until `present` is called, which `--resize` launches rely on.
pub(super) fn install(app: HWND, start_hidden: bool) -> Host {
    APP.set(app);
    HIDDEN.set(start_hidden);
    EXITING.set(false);
    // SAFETY: thread-local hooks with procedures in this module; Host removes them.
    unsafe {
        let thread = GetCurrentThreadId();
        CBT.set(SetWindowsHookExW(WH_CBT, Some(cbt), null_mut(), thread));
        MESSAGES.set(SetWindowsHookExW(
            WH_GETMESSAGE,
            Some(dialog_messages),
            null_mut(),
            thread,
        ));
    }
    Host
}
/// The WinUI settings window, once created and until destroyed.
pub(super) fn window() -> Option<HWND> {
    let hwnd = WINDOW.get();
    (!hwnd.is_null()).then_some(hwnd)
}
/// The WinUI Image Resizer dialog, when open.
pub(super) fn dialog() -> Option<HWND> {
    let hwnd = DIALOG.get();
    (!hwnd.is_null()).then_some(hwnd)
}
/// Shows and activates the settings window. Returns false when there is none.
pub(super) fn present() -> bool {
    let Some(hwnd) = window() else {
        return false;
    };
    HIDDEN.set(false);
    activate(hwnd);
    true
}
/// Shows and activates a secondary WinUI dialog (Image Resizer).
pub(super) fn present_dialog() -> bool {
    let Some(hwnd) = dialog() else {
        return false;
    };
    activate(hwnd);
    true
}
fn activate(hwnd: HWND) {
    // SAFETY: hwnd is a live WinUI window on this thread. Explorer verbs and
    // second-instance launches often lack foreground permission; a brief
    // topmost pulse plus AttachThreadInput matches the usual Win32 bring-up.
    unsafe {
        ShowWindow(
            hwnd,
            if IsIconic(hwnd) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW;
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
        SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
        let foreground = GetForegroundWindow();
        let mut foreground_pid = 0;
        let foreground_thread = GetWindowThreadProcessId(foreground, &mut foreground_pid);
        let this_thread = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != this_thread
            && AttachThreadInput(this_thread, foreground_thread, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(this_thread, foreground_thread, 0);
        }
    }
}
pub(super) fn hide() {
    if let Some(hwnd) = window() {
        // SAFETY: see `present`.
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}
/// Lets the next close reach WinUI, which ends the reactor loop. Returns false when
/// no WinUI window exists, so the caller must end its own loop.
pub(super) fn close_for_exit() -> bool {
    let Some(hwnd) = window() else {
        return false;
    };
    EXITING.set(true);
    // Posted, not sent: callers are inside the app window's WM_DESTROY, and
    // PostQuitMessage must not run while the WinUI window is still alive.
    // SAFETY: see `present`.
    unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 }
}
/// Routes the next non-modifier key press to the app as CAPTURED_KEY.
pub(super) fn begin_key_capture() {
    if !KEYS.get().is_null() {
        return;
    }
    // SAFETY: thread-local hook removed by end_key_capture or Host::drop.
    unsafe {
        KEYS.set(SetWindowsHookExW(
            WH_KEYBOARD,
            Some(keys),
            null_mut(),
            GetCurrentThreadId(),
        ));
    }
}
pub(super) fn end_key_capture() {
    let handle = KEYS.replace(null_mut());
    if !handle.is_null() {
        // SAFETY: the handle came from SetWindowsHookExW on this thread.
        unsafe {
            UnhookWindowsHookEx(handle);
        }
    }
}
fn is_winui_window(hwnd: HWND) -> bool {
    let mut class = [0u16; 64];
    // SAFETY: the buffer length is passed and GetClassNameW NUL-terminates.
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    len > 0 && String::from_utf16_lossy(&class[..len as usize]) == WINUI_CLASS
}
unsafe extern "system" fn cbt(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: Windows passes the window being created as wParam for HCBT_CREATEWND;
    // it is valid for this call and subclasses are removed on WM_NCDESTROY.
    unsafe {
        if code == HCBT_CREATEWND as i32 {
            let hwnd = w as HWND;
            let create = &*(l as *const CBT_CREATEWNDW);
            let top_level = (*create.lpcs).hwndParent.is_null();
            if top_level && is_winui_window(hwnd) {
                if WINDOW.get().is_null() {
                    if SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, 0) != 0 {
                        WINDOW.set(hwnd);
                        PostMessageW(APP.get(), CREATED, hwnd as WPARAM, 0);
                    }
                } else if DIALOG.get().is_null()
                    && SetWindowSubclass(hwnd, Some(dialog_subclass), DIALOG_SUBCLASS_ID, 0) != 0
                {
                    DIALOG.set(hwnd);
                    PostMessageW(APP.get(), DIALOG_CREATED, hwnd as WPARAM, 0);
                }
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}
unsafe extern "system" fn dialog_subclass(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // SAFETY: only clears the dialog HWND; close still reaches WinUI so the
    // secondary pump can end without tray hide-on-close semantics.
    unsafe {
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(dialog_subclass), DIALOG_SUBCLASS_ID);
            if DIALOG.get() == hwnd {
                DIALOG.set(null_mut());
            }
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}
unsafe extern "system" fn subclass(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // SAFETY: Windows calls the subclass on the window's own thread with valid
    // message parameters; WINDOWPOS is writable during WM_WINDOWPOSCHANGING.
    unsafe {
        match msg {
            WM_WINDOWPOSCHANGING if HIDDEN.get() => {
                let pos = &mut *(l as *mut WINDOWPOS);
                pos.flags &= !SWP_SHOWWINDOW;
            }
            WM_CLOSE if !EXITING.get() => {
                PostMessageW(APP.get(), CLOSE_REQUESTED, 0, 0);
                return 0;
            }
            WM_ACTIVATE if (w & 0xffff) as u32 == WA_INACTIVE => {
                PostMessageW(APP.get(), DEACTIVATED, 0, 0);
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
                WINDOW.set(null_mut());
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}
unsafe extern "system" fn dialog_messages(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: for HC_ACTION the hook receives the MSG about to be returned by the
    // reactor's GetMessage; it may be rewritten before that loop dispatches it.
    unsafe {
        if code == HC_ACTION as i32
            && w == PM_REMOVE as usize
            && let Some(dialog) = resize::live()
        {
            let message = &mut *(l as *mut MSG);
            if message.message != WM_NULL && IsDialogMessageW(dialog, message) != 0 {
                message.message = WM_NULL;
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}
fn is_modifier(vk: u16) -> bool {
    matches!(vk, VK_SHIFT | VK_CONTROL | VK_MENU | VK_LWIN | VK_RWIN)
}
unsafe extern "system" fn keys(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: WH_KEYBOARD passes the virtual key in wParam and keystroke flags in
    // lParam; modifier presses pass through so GetKeyState stays accurate.
    unsafe {
        let vk = w as u16;
        if code == HC_ACTION as i32 && !is_modifier(vk) {
            if (l >> 31) & 1 == 0 {
                let down = |key: u16| GetKeyState(key as i32) < 0;
                let mut mods = 0;
                if down(VK_LWIN) || down(VK_RWIN) {
                    mods |= MOD_WIN;
                }
                if down(VK_CONTROL) {
                    mods |= MOD_CTRL;
                }
                if down(VK_MENU) {
                    mods |= MOD_ALT;
                }
                if down(VK_SHIFT) {
                    mods |= MOD_SHIFT;
                }
                PostMessageW(APP.get(), CAPTURED_KEY, vk as WPARAM, mods);
            }
            return 1;
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}
