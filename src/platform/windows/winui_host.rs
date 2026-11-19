//! Win32 side of WinUI windows owned by windows-reactor.
//!
//! Callers enqueue a [`WindowRole`] with [`expect`] before creating each
//! top-level WinUI window. A CBT hook claims HWNDs in that order: settings get
//! tray hide-on-close semantics; dialogs only get icon/foreground hooks. The
//! queue allows a dialog announced during settings `Component::create` (before
//! the settings HWND exists) to claim correctly. Everything here runs on the UI
//! thread that also owns the hidden app window and power request.
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
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
const SETTINGS_SUBCLASS_ID: usize = 1;
const DIALOG_SUBCLASS_ID: usize = 2;

/// Which top-level WinUI window the next create should claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowRole {
    Settings,
    Dialog,
}

thread_local! {
    static APP: Cell<HWND> = const { Cell::new(null_mut()) };
    static SETTINGS: Cell<HWND> = const { Cell::new(null_mut()) };
    static DIALOG: Cell<HWND> = const { Cell::new(null_mut()) };
    // FIFO: Component::create can announce a dialog before the settings HWND exists.
    static PENDING: RefCell<VecDeque<WindowRole>> = const { RefCell::new(VecDeque::new()) };
    static HIDDEN: Cell<bool> = const { Cell::new(false) };
    // The reactor resizes the client area but keeps the default top-left corner.
    static PLACED: Cell<bool> = const { Cell::new(false) };
    static EXITING: Cell<bool> = const { Cell::new(false) };
    static CBT: Cell<HHOOK> = const { Cell::new(null_mut()) };

    static KEYS: Cell<HHOOK> = const { Cell::new(null_mut()) };
}
/// Owns the thread hooks for the lifetime of the WinUI loop.
pub(super) struct Host;
impl Drop for Host {
    fn drop(&mut self) {
        end_key_capture();
        PENDING.with(|pending| pending.borrow_mut().clear());
        let handle = CBT.with(|hook| hook.replace(null_mut()));
        if !handle.is_null() {
            // SAFETY: the handle came from SetWindowsHookExW on this thread.
            unsafe {
                UnhookWindowsHookEx(handle);
            }
        }
    }
}
/// Starts watching this thread for WinUI windows. `start_hidden` keeps the
/// settings window off screen until `present` is called, which `--resize`
/// launches rely on.
pub(super) fn install(app: HWND, start_hidden: bool) -> Host {
    APP.set(app);
    HIDDEN.set(start_hidden);
    EXITING.set(false);
    PENDING.with(|pending| pending.borrow_mut().clear());
    // SAFETY: thread-local hooks with procedures in this module; Host removes them.
    unsafe {
        let thread = GetCurrentThreadId();
        CBT.set(SetWindowsHookExW(WH_CBT, Some(cbt), null_mut(), thread));
    }
    Host
}
/// Queue the role of the next top-level WinUI window before creating it.
///
/// Multiple calls enqueue in order so a dialog announced during settings
/// `Component::create` still claims after the settings HWND.
pub(super) fn expect(role: WindowRole) {
    PENDING.with(|pending| pending.borrow_mut().push_back(role));
}
fn take_expected() -> Option<WindowRole> {
    PENDING.with(|pending| pending.borrow_mut().pop_front())
}
/// The WinUI settings window, once created and until destroyed.
pub(super) fn settings() -> Option<HWND> {
    let hwnd = SETTINGS.get();
    (!hwnd.is_null()).then_some(hwnd)
}
/// The WinUI Image Resizer dialog, when open.
pub(super) fn dialog() -> Option<HWND> {
    let hwnd = DIALOG.get();
    (!hwnd.is_null()).then_some(hwnd)
}
/// Shows and activates the settings window. Returns false when there is none.
pub(super) fn present() -> bool {
    let Some(hwnd) = settings() else {
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
/// True while the settings window is on screen (shown and not minimized).
pub(super) fn settings_visible() -> bool {
    settings().is_some_and(|hwnd| {
        // SAFETY: read-only state queries on a live window owned by this thread.
        unsafe { IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 }
    })
}
pub(super) fn hide() {
    if let Some(hwnd) = settings() {
        // SAFETY: see `present`.
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}
/// Lets the next close reach WinUI, which ends the reactor loop. Returns false when
/// no WinUI window exists, so the caller must end its own loop.
pub(super) fn close_for_exit() -> bool {
    let Some(hwnd) = settings() else {
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
/// Clears layered window alpha so Desktop Acrylic remains the shell backdrop.
/// Tint strength comes from the WinUI overlay driven by `shell_opacity`, not HWND alpha.
pub(super) fn apply_shell_opacity(_percent: i32) {
    let Some(hwnd) = settings() else {
        return;
    };
    // SAFETY: settings HWND is live on this UI thread until NCDESTROY clears it.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if style & WS_EX_LAYERED as isize != 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style & !(WS_EX_LAYERED as isize));
        }
    }
}
fn is_winui_window(hwnd: HWND) -> bool {
    let mut class = [0u16; 64];
    // SAFETY: the buffer length is passed and GetClassNameW NUL-terminates.
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    len > 0 && String::from_utf16_lossy(&class[..len as usize]) == WINUI_CLASS
}
fn claim(hwnd: HWND, role: WindowRole) -> bool {
    match role {
        WindowRole::Settings if !SETTINGS.get().is_null() => false,
        WindowRole::Dialog if !DIALOG.get().is_null() => false,
        WindowRole::Settings => {
            // SAFETY: hwnd is the window under HCBT_CREATEWND on this thread.
            if unsafe { SetWindowSubclass(hwnd, Some(settings_subclass), SETTINGS_SUBCLASS_ID, 0) }
                == 0
            {
                return false;
            }
            SETTINGS.set(hwnd);
            PLACED.set(false);
            unsafe {
                PostMessageW(APP.get(), CREATED, hwnd as WPARAM, 0);
            }
            true
        }
        WindowRole::Dialog => {
            // SAFETY: see Settings arm.
            if unsafe { SetWindowSubclass(hwnd, Some(dialog_subclass), DIALOG_SUBCLASS_ID, 0) } == 0
            {
                return false;
            }
            DIALOG.set(hwnd);
            unsafe {
                PostMessageW(APP.get(), DIALOG_CREATED, hwnd as WPARAM, 0);
            }
            true
        }
    }
}
unsafe extern "system" fn cbt(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: Windows passes the window being created as wParam for HCBT_CREATEWND;
    // it is valid for this call and subclasses are removed on WM_NCDESTROY.
    unsafe {
        if code == HCBT_CREATEWND as i32 {
            let hwnd = w as HWND;
            let create = &*(l as *const CBT_CREATEWNDW);
            let top_level = (*create.lpcs).hwndParent.is_null();
            if top_level
                && is_winui_window(hwnd)
                && let Some(role) = take_expected()
            {
                let _ = claim(hwnd, role);
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
unsafe extern "system" fn settings_subclass(
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
            WM_WINDOWPOSCHANGING => {
                let pos = &mut *(l as *mut WINDOWPOS);
                if HIDDEN.get() {
                    pos.flags &= !SWP_SHOWWINDOW;
                } else if pos.flags & SWP_SHOWWINDOW != 0
                    && IsIconic(hwnd) == 0
                    && !PLACED.replace(true)
                {
                    center_on_work_area(hwnd, pos);
                }
            }
            WM_CLOSE if !EXITING.get() => {
                PostMessageW(APP.get(), CLOSE_REQUESTED, 0, 0);
                return 0;
            }
            WM_ACTIVATE if (w & 0xffff) as u32 == WA_INACTIVE => {
                PostMessageW(APP.get(), DEACTIVATED, 0, 0);
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(settings_subclass), SETTINGS_SUBCLASS_ID);
                SETTINGS.set(null_mut());
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}
/// Rewrites the first show of the settings window so it fits and is centred in the
/// work area of its monitor (taskbar excluded).
///
/// # Safety
/// `hwnd` must be the live settings window on this thread and `pos` the WINDOWPOS
/// of its current WM_WINDOWPOSCHANGING.
unsafe fn center_on_work_area(hwnd: HWND, pos: &mut WINDOWPOS) {
    // SAFETY: hwnd is live per the caller; the out-structs are sized for these APIs.
    unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() || GetMonitorInfoW(monitor, &mut info) == 0 {
            return;
        }
        let size = if pos.flags & SWP_NOSIZE != 0 {
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return;
            }
            (rect.right - rect.left, rect.bottom - rect.top)
        } else {
            (pos.cx, pos.cy)
        };
        let (x, y, cx, cy) = centered(info.rcWork, size);
        pos.x = x;
        pos.y = y;
        pos.cx = cx;
        pos.cy = cy;
        pos.flags &= !(SWP_NOMOVE | SWP_NOSIZE);
    }
}
/// Clamps `size` to `work` and centres it there; returns x, y, width and height.
fn centered(work: RECT, (width, height): (i32, i32)) -> (i32, i32, i32, i32) {
    let work_width = work.right - work.left;
    let work_height = work.bottom - work.top;
    let width = width.clamp(1, work_width.max(1));
    let height = height.clamp(1, work_height.max(1));
    (
        work.left + (work_width - width) / 2,
        work.top + (work_height - height) / 2,
        width,
        height,
    )
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

#[cfg(test)]
mod tests {
    use super::{PENDING, RECT, WindowRole, centered, expect, take_expected};

    #[test]
    fn first_show_is_centred_and_clamped_to_the_work_area() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 2880,
            bottom: 1704,
        };
        assert_eq!(centered(work, (2000, 1500)), (440, 102, 2000, 1500));
        assert_eq!(centered(work, (3000, 2000)), (0, 0, 2880, 1704));
        let second = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        assert_eq!(centered(second, (1000, 800)), (-1460, 120, 1000, 800));
    }

    #[test]
    fn expect_queues_roles_in_order() {
        PENDING.with(|pending| pending.borrow_mut().clear());
        expect(WindowRole::Settings);
        expect(WindowRole::Dialog);
        assert_eq!(take_expected(), Some(WindowRole::Settings));
        assert_eq!(take_expected(), Some(WindowRole::Dialog));
        assert_eq!(take_expected(), None);
    }
}
