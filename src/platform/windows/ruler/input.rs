//! Overlay input: drag, click, right click, wheel and keyboard shortcuts on the
//! measurement overlays. The toolbar forwards its wheel and keys here too.
use super::*;

/// Window procedure of the per-monitor measurement overlays.
pub(super) unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_SETCURSOR => {
            unsafe {
                SetCursor(null_mut());
            }
            1
        }
        WM_LBUTTONDOWN => {
            on_down(hwnd, l);
            0
        }
        WM_MOUSEMOVE => {
            on_move(hwnd, l);
            0
        }
        WM_LBUTTONUP => {
            on_up(hwnd);
            0
        }
        WM_RBUTTONUP => {
            on_right(hwnd);
            0
        }
        WM_MOUSEWHEEL => {
            on_wheel(w);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            on_key(w);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}
fn monitor_index(hwnd: HWND) -> Option<usize> {
    RULER.with(|ruler| {
        ruler.borrow().session.as_ref().and_then(|session| {
            session
                .monitors
                .iter()
                .position(|monitor| monitor.hwnd == hwnd)
        })
    })
}
/// Signed client coordinates packed in a mouse message's `LPARAM`.
pub(super) fn point_from(l: LPARAM) -> (i32, i32) {
    let x = (l & 0xffff) as i16 as i32;
    let y = ((l >> 16) & 0xffff) as i16 as i32;
    (x, y)
}
fn shift_down() -> bool {
    unsafe { GetKeyState(VK_SHIFT as i32) < 0 }
}
fn ctrl_down() -> bool {
    unsafe { GetKeyState(VK_CONTROL as i32) < 0 }
}
/// Ctrl+1 through Ctrl+4 choose a tool; Escape closes (and copies in Bounds).
pub(super) fn on_key(vk: WPARAM) {
    if ctrl_down() {
        let mode = match vk as u16 {
            0x31 => Some(Mode::Bounds),
            0x32 => Some(Mode::Spacing),
            0x33 => Some(Mode::Horizontal),
            0x34 => Some(Mode::Vertical),
            _ => None,
        };
        if let Some(mode) = mode {
            select_tool(mode);
            return;
        }
    }
    if vk as u16 == VK_ESCAPE {
        escape();
    }
}
fn on_down(hwnd: HWND, l: LPARAM) {
    let Some(index) = monitor_index(hwnd) else {
        return;
    };
    if toolbar_contains_cursor() {
        return;
    }
    let (x, y) = point_from(l);
    let started = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let Some(session) = ruler.session.as_mut() else {
            return false;
        };
        let mode = session.mode;
        session
            .monitors
            .get_mut(index)
            .is_some_and(|monitor| monitor.marks.press(mode, x, y))
    });
    if !started {
        return;
    }
    // Keep the drag on this monitor until the button is released.
    unsafe {
        SetCapture(hwnd);
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect) != 0 {
            ClipCursor(&rect);
        }
    }
    refresh(Some(index));
}
fn on_move(hwnd: HWND, l: LPARAM) {
    let Some(index) = monitor_index(hwnd) else {
        refresh(None);
        return;
    };
    let (x, y) = point_from(l);
    with_monitor_mut(index, |monitor| monitor.marks.drag_to(x, y));
    refresh(Some(index));
}
fn on_up(hwnd: HWND) {
    let Some(index) = monitor_index(hwnd) else {
        return;
    };
    unsafe {
        ReleaseCapture();
        ClipCursor(null());
    }
    let release = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let session = ruler.session.as_mut()?;
        let mode = session.mode;
        let per_channel = session.config.per_channel;
        let tolerance = session.tolerance;
        let fresh = wants_capture(session);
        let monitor = session.monitors.get_mut(index)?;
        let measured = if mode.edges() {
            measure_at_cursor(monitor, fresh, per_channel, tolerance)
        } else {
            None
        };
        monitor.marks.release(mode, shift_down(), measured)
    });
    let Some(release) = release else {
        refresh(Some(index));
        return;
    };
    copy_measurements(Some(index), release.current);
    if release.close {
        end_session();
    } else {
        refresh(Some(index));
    }
}
/// Cursor position and the edge span under it on `monitor`, for the spacing tools.
fn measure_at_cursor(
    monitor: &mut Monitor,
    fresh: bool,
    per_channel: bool,
    tolerance: u8,
) -> Option<(i32, i32, Span)> {
    let (x, y) = cursor_on(monitor)?;
    if fresh {
        // The timer only refreshes the cursor's monitor every 33 ms; measure the
        // click against the screen as it is now.
        regrab(monitor);
    }
    let span = detect_edges(
        &monitor.pixels,
        monitor.width,
        monitor.height,
        x,
        y,
        per_channel,
        tolerance,
    )?;
    Some((x, y, span))
}
fn on_right(hwnd: HWND) {
    let Some(index) = monitor_index(hwnd) else {
        return;
    };
    // `right_click` already applied the action to the marks; carry out the rest.
    let action = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let session = ruler.session.as_mut()?;
        let mode = session.mode;
        Some(session.monitors.get_mut(index)?.marks.right_click(mode))
    });
    match action {
        Some(BoundsRight::CancelDrag) => {
            unsafe {
                ReleaseCapture();
                ClipCursor(null());
            }
            refresh(Some(index));
        }
        Some(BoundsRight::ClearKept) => refresh(Some(index)),
        _ => end_session(),
    }
}
/// The wheel changes edge tolerance for this session only.
pub(super) fn on_wheel(w: WPARAM) {
    let delta = ((w >> 16) & 0xffff) as u16 as i16;
    RULER.with(|ruler| {
        if let Some(session) = ruler.borrow_mut().session.as_mut() {
            session.tolerance = wheel_tolerance(session.tolerance, delta);
        }
    });
    refresh(None);
}
