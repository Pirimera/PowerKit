//! Which windows AlwaysTop may pin: root resolution, shell/cloaked/owned skips, titles and the remembered target.
use super::*;
/// The window a pin command acts on: the remembered eligible window, else `candidate`
/// (the foreground window in production) when that is eligible.
pub(super) fn resolve_target(app: HWND, candidate: HWND) -> HWND {
    let candidate = id_of(root_window(candidate));
    TOP.with(|top| top.target(&Desktop { app }, candidate))
        .map_or(null_mut(), hwnd_of)
}
pub(super) fn live(hwnd: HWND) -> bool {
    !hwnd.is_null() && unsafe { IsWindow(hwnd) } != 0
}
pub(super) fn root_window(hwnd: HWND) -> HWND {
    if hwnd.is_null() {
        return hwnd;
    }
    let ancestor = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if ancestor.is_null() { hwnd } else { ancestor }
}
pub(super) fn owned_by(hwnd: HWND, owner: HWND) -> bool {
    let mut current = hwnd;
    while !current.is_null() {
        if current == owner {
            return true;
        }
        current = unsafe { GetWindow(current, GW_OWNER) };
    }
    false
}
pub(super) fn cloaked(hwnd: HWND) -> bool {
    let mut value = 0u32;
    // SAFETY: value is a live DWORD; hwnd is a window IsWindow accepted.
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, (&raw mut value).cast(), 4) == 0
            && value != 0
    }
}
pub(super) fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len as usize])
    }
}
pub(super) fn shell_class(name: &str) -> bool {
    matches!(
        name,
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "Progman"
            | "WorkerW"
            | "ForegroundStaging"
            | "#32768"
            | "PowerKit.AlwaysTop.Border"
            | "PowerKit.ScreenRuler.Overlay"
            | "PowerKit.ScreenRuler.Toolbar"
    )
}
pub(super) fn shell_surface(hwnd: HWND) -> bool {
    shell_class(&class_name(hwnd))
}
/// True for any window of this process: the unowned WinUI settings window and
/// dialogs as well as the hidden app window and Win32 overlays.
pub(super) fn own_process(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    // SAFETY: pid is a live DWORD; a stale hwnd only yields thread id 0.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) != 0 && pid == GetCurrentProcessId() }
}
pub(super) fn eligible(hwnd: HWND, app: HWND) -> bool {
    if !live(hwnd) || hwnd == app || owned_by(hwnd, app) || own_process(hwnd) {
        return false;
    }
    unsafe {
        if IsWindowVisible(hwnd) == 0 || hwnd == GetDesktopWindow() {
            return false;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_CHILD != 0 || ex & WS_EX_TOOLWINDOW != 0 {
            return false;
        }
    }
    !cloaked(hwnd) && !shell_surface(hwnd)
}
pub(super) fn title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len as usize])
    }
}
