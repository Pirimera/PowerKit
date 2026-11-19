//! WinEvent hooks that keep pins topmost and borders in place across focus, minimize, move and destroy.
use super::*;
pub(super) fn restore_pin_if_dropped() {
    if PIN.with(|s| s.borrow().holding) {
        return;
    }
    let hwnds: Vec<HWND> = PIN.with(|s| s.borrow().pins.iter().map(|pin| pin.hwnd).collect());
    PIN.with(|s| s.borrow_mut().holding = true);
    for hwnd in hwnds {
        if live(hwnd) && !is_topmost(hwnd) {
            let _ = set_topmost(hwnd, true);
            glue_border_for(hwnd);
        }
    }
    PIN.with(|s| s.borrow_mut().holding = false);
}
pub(super) fn reapply_pin(hwnd: HWND) {
    if PIN.with(|s| s.borrow().holding) || !is_pinned_window(hwnd) || !live(hwnd) {
        return;
    }
    PIN.with(|s| s.borrow_mut().holding = true);
    let _ = set_topmost(hwnd, true);
    glue_border_for(hwnd);
    PIN.with(|s| s.borrow_mut().holding = false);
}
pub(super) fn install_hooks() {
    uninstall_hooks();
    // SAFETY: callback only reads thread-local pin state on this UI thread's message loop.
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    let focus = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_MINIMIZEEND,
            null_mut(),
            Some(on_win_event),
            0,
            0,
            flags,
        )
    };
    let target = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_LOCATIONCHANGE,
            null_mut(),
            Some(on_win_event),
            0,
            0,
            flags,
        )
    };
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.hook_focus = focus as isize;
        s.hook_target = target as isize;
    });
}
pub(super) fn uninstall_hooks() {
    let (focus, target) = PIN.with(|s| {
        let s = s.borrow();
        (s.hook_focus, s.hook_target)
    });
    // SAFETY: hooks were installed on this thread and are unhooked once.
    unsafe {
        if focus != 0 {
            UnhookWinEvent(focus as _);
        }
        if target != 0 {
            UnhookWinEvent(target as _);
        }
    }
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.hook_focus = 0;
        s.hook_target = 0;
    });
}
pub(super) unsafe extern "system" fn on_win_event(
    _: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if id_object != OBJID_WINDOW {
        return;
    }
    let app = PIN.with(|s| s.borrow().app);
    if app.is_null() {
        return;
    }
    match event {
        EVENT_SYSTEM_FOREGROUND => {
            let _ = offer(hwnd, app);
            restore_pin_if_dropped();
        }
        EVENT_OBJECT_FOCUS => restore_pin_if_dropped(),
        EVENT_SYSTEM_MINIMIZESTART => {
            let target = root_window(hwnd);
            if is_pinned_window(target) {
                hide_border_for(target);
            }
        }
        EVENT_SYSTEM_MINIMIZEEND => {
            let target = root_window(hwnd);
            if is_pinned_window(target) {
                reapply_pin(target);
                show_border(target);
            }
        }
        EVENT_OBJECT_DESTROY => {
            let target = root_window(hwnd);
            if is_pinned_window(target) {
                let _ = unpin_window(target);
            }
        }
        EVENT_OBJECT_LOCATIONCHANGE | EVENT_SYSTEM_MOVESIZEEND => {
            let target = root_window(hwnd);
            if is_pinned_window(target) {
                glue_border_for(target);
            }
        }
        _ => {}
    }
}
