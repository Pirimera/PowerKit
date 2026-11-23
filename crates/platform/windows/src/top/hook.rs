//! WinEvent hooks that keep pins topmost and borders in place across focus, minimize, move and destroy.
use super::*;
/// Marks a topmost re-apply in progress, so the events it causes are not handled again.
/// Dropping the guard clears the mark on every exit path, including early returns.
struct Holding;
impl Holding {
    fn acquire() -> Option<Self> {
        PIN.with(|s| {
            let mut s = s.borrow_mut();
            if s.holding {
                return None;
            }
            s.holding = true;
            Some(Self)
        })
    }
}
impl Drop for Holding {
    fn drop(&mut self) {
        PIN.with(|s| s.borrow_mut().holding = false);
    }
}
pub(super) fn restore_pin_if_dropped() {
    let Some(_hold) = Holding::acquire() else {
        return;
    };
    let hwnds: Vec<HWND> = PIN.with(|s| s.borrow().pins.iter().map(|pin| pin.hwnd).collect());
    for hwnd in hwnds {
        if live(hwnd) && !is_topmost(hwnd) {
            let _ = set_topmost(hwnd, true);
            glue_border_for(hwnd);
        }
    }
}
pub(super) fn reapply_pin(hwnd: HWND) {
    if !is_pinned_window(hwnd) || !live(hwnd) {
        return;
    }
    let Some(_hold) = Holding::acquire() else {
        return;
    };
    let _ = set_topmost(hwnd, true);
    glue_border_for(hwnd);
}
/// Event ranges: foreground through minimize end; destroy, show, hide, focus and
/// location change; cloak and uncloak. The cloak pair has its own hook so the
/// frequent name and value events between the ranges are never delivered.
const HOOK_RANGES: [(u32, u32); 3] = [
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND),
    (EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE),
    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
];
pub(super) fn install_hooks() {
    uninstall_hooks();
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    // SAFETY: out-of-context hooks call back on this UI thread's message loop, where
    // the callback only touches thread-local pin state.
    let hooks = HOOK_RANGES.map(|(min, max)| unsafe {
        SetWinEventHook(min, max, null_mut(), Some(on_win_event), 0, 0, flags) as isize
    });
    PIN.with(|s| s.borrow_mut().hooks = hooks);
}
pub(super) fn uninstall_hooks() {
    let hooks = PIN.with(|s| std::mem::take(&mut s.borrow_mut().hooks));
    // SAFETY: hooks were installed on this thread and are unhooked once.
    for hook in hooks.into_iter().filter(|&hook| hook != 0) {
        unsafe {
            UnhookWinEvent(hook as _);
        }
    }
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
    on_event(event, hwnd, app);
}
/// Window-event handling, separate from the hook callback so the smoke check can
/// drive it: the hooks skip PowerKit's own process and never see its fixtures.
pub(super) fn on_event(event: u32, hwnd: HWND, app: HWND) {
    match event {
        EVENT_SYSTEM_FOREGROUND => {
            let _ = offer(hwnd, app);
            restore_pin_if_dropped();
            // The backup timer is slow, so a focused pinned window restacks its ring here.
            let root = root_window(hwnd);
            if is_pinned_window(root) {
                glue_border_for(root);
            }
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
        // Destroy, show, hide and cloak name the window itself. Match it exactly: by
        // now a destroyed window has no root, and a child's root is not the target.
        EVENT_OBJECT_DESTROY if tracked(hwnd) => {
            let _ = unpin_window(hwnd);
        }
        EVENT_OBJECT_SHOW | EVENT_OBJECT_HIDE | EVENT_OBJECT_CLOAKED | EVENT_OBJECT_UNCLOAKED
            if tracked(hwnd) =>
        {
            sync_border(hwnd);
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
