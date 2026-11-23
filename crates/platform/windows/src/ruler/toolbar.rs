//! The toolbar window: placement on open, moving with the grip, DPI and display
//! changes, theme refresh and its window procedure. Drawing and hit testing live
//! in `bar`.
use super::*;

/// Work area and effective DPI of a monitor, or `None` when Windows cannot describe it.
fn monitor_work(monitor: HMONITOR) -> Option<(RECT, i32)> {
    if monitor.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    Some((info.rcWork, dpi_x.max(96) as i32))
}
/// Toolbar rectangle in screen pixels: the user's last drag position pulled back
/// into the nearest work area, or PowerToys' default (centered, 12 DIP below the
/// top of the primary work area).
fn toolbar_rect(shots: &[Shot], remembered: Option<POINT>) -> RECT {
    let primary = shots.iter().find(|shot| shot.primary).unwrap_or(&shots[0]);
    let anchor = remembered.unwrap_or(POINT {
        x: primary.origin_x + primary.width / 2,
        y: primary.origin_y + primary.height / 2,
    });
    let monitor = unsafe { MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST) };
    let (work, dpi) = monitor_work(monitor).unwrap_or((
        RECT {
            left: primary.origin_x,
            top: primary.origin_y,
            right: primary.origin_x + primary.width,
            bottom: primary.origin_y + primary.height,
        },
        primary.dpi.max(96.0) as i32,
    ));
    let width = bar::px(bar::WIDTH_DIP, dpi);
    let height = bar::px(bar::HEIGHT_DIP, dpi);
    let (x, y) = match remembered {
        Some(point) => bar::fit(
            RECT {
                left: point.x,
                top: point.y,
                right: point.x + width,
                bottom: point.y + height,
            },
            work,
        ),
        None => (
            work.left + (work.right - work.left - width) / 2,
            work.top + bar::px(bar::TOP_DIP, dpi),
        ),
    };
    RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}
/// Ends a toolbar drag: keeps the bar fully inside the nearest work area and
/// remembers where it was left.
fn toolbar_moved(hwnd: HWND) {
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return;
    }
    let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) };
    let (x, y) = monitor_work(monitor)
        .map(|(work, _)| bar::fit(rect, work))
        .unwrap_or((rect.left, rect.top));
    if (x, y) != (rect.left, rect.top) {
        unsafe {
            SetWindowPos(
                hwnd,
                null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    RULER.with(|ruler| ruler.borrow_mut().toolbar_at = Some(POINT { x, y }));
    redraw();
}
/// Per-monitor DPI v2 leaves resizing to the window. Keep the system's suggested
/// position, size the bar exactly for the new DPI and move the tooltip areas.
fn toolbar_dpi_changed(hwnd: HWND, w: WPARAM, l: LPARAM) {
    if l == 0 {
        return;
    }
    // SAFETY: for WM_DPICHANGED, lParam points to a RECT owned by the system that
    // stays valid for the duration of this synchronous message.
    let suggested = unsafe { *(l as *const RECT) };
    let dpi = ((w & 0xffff) as i32).max(96);
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            suggested.left,
            suggested.top,
            bar::px(bar::WIDTH_DIP, dpi),
            bar::px(bar::HEIGHT_DIP, dpi),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        InvalidateRect(hwnd, null(), 0);
    }
    bar::refresh_tips(hwnd);
}
/// Creates the hidden toolbar window; `open` shows it once the session exists.
pub(super) fn create_toolbar(shots: &[Shot], remembered: Option<POINT>) -> io::Result<HWND> {
    let RECT {
        left: x,
        top: y,
        right,
        bottom,
    } = toolbar_rect(shots, remembered);
    let (width, height) = (right - x, bottom - y);
    let instance = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()) };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            wide(TOOLBAR_CLASS).as_ptr(),
            wide("ScreenRuler").as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    if hwnd.is_null() {
        Err(io::Error::last_os_error())
    } else {
        unsafe {
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
        bar::polish(hwnd);
        Ok(hwnd)
    }
}
/// Window procedure of the toolbar.
pub(super) unsafe extern "system" fn toolbar_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mode = RULER.with(|ruler| {
                ruler
                    .borrow()
                    .session
                    .as_ref()
                    .map(|session| session.mode)
                    .unwrap_or(Mode::None)
            });
            bar::paint(hwnd, mode);
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = point_from(l);
            bar::pointer_move(hwnd, x, y);
            0
        }
        WM_MOUSELEAVE => {
            bar::pointer_leave(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = point_from(l);
            if bar::on_grip(hwnd, x, y) {
                bar::begin_move(hwnd);
            } else {
                bar::pointer_down(hwnd, x, y);
            }
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = point_from(l);
            if let Some(hit) = bar::pointer_up(hwnd, x, y) {
                match hit {
                    Hit::Close => end_session(),
                    Hit::Tool(mode) => select_tool(mode),
                    Hit::Grip => {}
                }
            }
            0
        }
        WM_SETCURSOR if (l & 0xffff) as u32 == HTCLIENT && bar::cursor_on_grip(hwnd) => {
            unsafe {
                SetCursor(LoadCursorW(null_mut(), IDC_SIZEALL));
            }
            1
        }
        WM_EXITSIZEMOVE => {
            toolbar_moved(hwnd);
            0
        }
        WM_DPICHANGED => {
            toolbar_dpi_changed(hwnd, w, l);
            0
        }
        // Rebuild after the layout settles; never inside the broadcast itself.
        WM_DISPLAYCHANGE => {
            notify_app(DISPLAY_CHANGED);
            0
        }
        // Light/dark, high contrast or accent changed while the bar is open: the
        // colors are read on every paint, only the DWM dark-mode frame is cached.
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED => {
            bar::polish(hwnd);
            unsafe {
                InvalidateRect(hwnd, null(), 0);
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
        WM_NCDESTROY => {
            bar::clear();
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
