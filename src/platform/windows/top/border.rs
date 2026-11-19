//! Border HWND lifecycle for pinned windows: D2D frame paint, z-order glue behind the target, show/hide and the backup timer.
use super::*;
use std::cell::Cell;
/// Backup resync for rings. Moves, shows, hides, cloaks and minimizes arrive from the
/// WinEvent hooks; this timer only catches what no event reports.
const BACKUP_SYNC_MS: u32 = 500;
thread_local! {
    static RING_ACCENT: Cell<Option<u32>> = const { Cell::new(None) };
    static CLASS_REGISTERED: Cell<bool> = const { Cell::new(false) };
}
/// Accent colour for rings without a custom colour. Read once instead of on every
/// paint; colour and theme broadcasts clear it through `forget_ring_accent`.
fn ring_accent() -> u32 {
    RING_ACCENT.with(|slot| match slot.get() {
        Some(color) => color,
        None => {
            let color = super::super::frame::accent_color();
            slot.set(Some(color));
            color
        }
    })
}
fn forget_ring_accent() {
    RING_ACCENT.with(|slot| slot.set(None));
}
#[derive(Clone, Copy)]
pub(super) struct FramePaint {
    rgb: u32,
    alpha: f32,
    thickness: i32,
}
pub(super) fn frame_paint(hwnd: HWND) -> FramePaint {
    let (color, opacity, thickness) = PIN.with(|s| {
        let s = s.borrow();
        (s.frame_color, s.frame_opacity, s.frame_thickness)
    });
    FramePaint {
        rgb: color.unwrap_or_else(ring_accent),
        alpha: opacity as f32 / 100.0,
        thickness: scaled_frame_pad(thickness, super::super::frame::scale_factor(hwnd)),
    }
}
pub(super) fn scaled_frame_pad(thickness: i32, scale: f32) -> i32 {
    (thickness as f32 * scale).round().max(1.0) as i32
}
pub(super) fn refresh_frames() {
    let targets: Vec<HWND> = PIN.with(|s| s.borrow().pins.iter().map(|pin| pin.hwnd).collect());
    for target in targets {
        glue_border_for(target);
    }
}
pub(super) fn frame_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&raw mut rect).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } != 0
    {
        return None;
    }
    let thick = frame_paint(hwnd).thickness;
    rect.left -= thick;
    rect.top -= thick;
    rect.right += thick;
    rect.bottom += thick;
    Some(rect)
}
/// Whether the ring should be on screen: the target is visible, not minimized and
/// not cloaked (another virtual desktop, a suspended app). The ring is a tool
/// window, which Windows shows on every virtual desktop, so it must follow cloaking.
pub(super) fn shown(target: HWND) -> bool {
    // SAFETY: plain state queries; a stale hwnd only reads as not visible.
    live(target)
        && unsafe { IsWindowVisible(target) != 0 && IsIconic(target) == 0 }
        && !cloaked(target)
}
/// The ring window of the exact pinned `target`, or null.
pub(super) fn border_of(target: HWND) -> HWND {
    with_pin(target, |pin| pin.border).unwrap_or(null_mut())
}
pub(super) fn show_border(target: HWND) {
    if !shown(target) {
        return;
    }
    if border_of(target).is_null() {
        create_border(target);
    } else {
        glue_border_for(target);
    }
}
/// Registers the ring class once per thread; later rings reuse it.
fn register_border_class() -> bool {
    CLASS_REGISTERED.with(|done| {
        if done.get() {
            return true;
        }
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class = super::super::wide(BORDER_CLASS);
        let wnd = WNDCLASSW {
            lpfnWndProc: Some(border_proc),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        let registered = unsafe { RegisterClassW(&wnd) } != 0
            || std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_CLASS_ALREADY_EXISTS as i32);
        done.set(registered);
        registered
    })
}
fn create_border(target: HWND) {
    let Some(rect) = frame_rect(target) else {
        return;
    };
    forget_ring_accent();
    if !register_border_class() {
        return;
    }
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = super::super::wide(BORDER_CLASS);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class.as_ptr(),
            std::ptr::null(),
            WS_POPUP | WS_DISABLED,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() || !make_border_transparent(hwnd) {
        if !hwnd.is_null() {
            unsafe {
                DestroyWindow(hwnd);
            }
        }
        return;
    }
    unsafe {
        let exclude = 1i32;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_EXCLUDED_FROM_PEEK as u32,
            (&raw const exclude).cast(),
            4,
        );
        SetWindowPos(
            target,
            hwnd,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOMOVE | SWP_NOSIZE,
        );
    }
    let Some(drawer) = super::super::frame::FrameDrawer::create(hwnd) else {
        unsafe {
            DestroyWindow(hwnd);
        }
        return;
    };
    with_pin_mut(target, |pin| {
        pin.border = hwnd;
        pin.drawer = Some(drawer);
    });
    // glue_border_for shows the ring, then paints it.
    glue_border_for(target);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, target as isize);
        SetTimer(hwnd, BORDER_TIMER, BACKUP_SYNC_MS, None);
    }
}
pub(super) fn make_border_transparent(hwnd: HWND) -> bool {
    let pos = -unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) } - 8;
    let region = unsafe { CreateRectRgn(pos, 0, pos + 1, 1) };
    if region.is_null() {
        return false;
    }
    let blur = DWM_BLURBEHIND {
        dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
        fEnable: 1,
        hRgnBlur: region,
        fTransitionOnMaximized: 0,
    };
    let ok = unsafe { DwmEnableBlurBehindWindow(hwnd, &blur) } == 0
        && unsafe { SetLayeredWindowAttributes(hwnd, 0, 0, LWA_COLORKEY) } != 0
        && unsafe { SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) } != 0;
    unsafe {
        DeleteObject(region);
    }
    ok
}
pub(super) fn glue_border_for(target: HWND) {
    let (border, target) =
        with_pin(target, |pin| (pin.border, pin.hwnd)).unwrap_or((null_mut(), null_mut()));
    if !live(border) || !live(target) {
        return;
    }
    let border_visible = unsafe { IsWindowVisible(border) } != 0;
    let Some(rect) = shown(target).then(|| frame_rect(target)).flatten() else {
        if border_visible {
            // SAFETY: border is our live ring window on this thread; no RefCell borrow held.
            unsafe {
                ShowWindow(border, SW_HIDE);
            }
        }
        return;
    };
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    unsafe {
        if !border_visible {
            ShowWindow(border, SW_SHOWNOACTIVATE);
        }
        SetWindowPos(
            border,
            target,
            rect.left,
            rect.top,
            width,
            height,
            SWP_NOREDRAW | SWP_NOACTIVATE,
        );
    }
    let local = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let paint = frame_paint(target);
    let radius = super::super::frame::corner_radius(target);
    // D2D runs with the drawer moved out of the pin, never while the pin state is borrowed.
    let Some(mut drawer) = take_drawer(target) else {
        return;
    };
    if !border_visible {
        // A ring shown again may have lost what it showed; paint it afresh.
        drawer.invalidate();
    }
    drawer.paint(local, paint.rgb, paint.alpha, paint.thickness, radius);
    put_drawer(target, drawer);
}
fn take_drawer(target: HWND) -> Option<super::super::frame::FrameDrawer> {
    with_pin_mut(target, |pin| pin.drawer.take()).flatten()
}
/// Returns the drawer to its pin; a drawer whose pin has gone is dropped here, on the UI thread.
fn put_drawer(target: HWND, drawer: super::super::frame::FrameDrawer) {
    with_pin_mut(target, |pin| pin.drawer = Some(drawer));
}
/// Brings the ring of the exact pinned `target` in line with the window: unpins a
/// destroyed window, creates a missing ring, reapplies a dropped topmost flag, and
/// moves, shows or hides the ring.
pub(super) fn sync_border(target: HWND) {
    if !tracked(target) {
        return;
    }
    if !live(target) {
        let _ = unpin_window(target);
        return;
    }
    if border_of(target).is_null() {
        show_border(target);
        return;
    }
    if !is_topmost(target) {
        restore_pin_if_dropped();
    }
    glue_border_for(target);
}
/// Hides the ring of the exact pinned `target`. The ring window and its D2D target
/// stay, so restoring the window shows the same ring again.
pub(super) fn hide_border_for(target: HWND) {
    let border = border_of(target);
    if live(border) {
        unsafe {
            ShowWindow(border, SW_HIDE);
        }
    }
}
/// Releases the D2D target, then destroys the ring window and its backup timer.
pub(super) fn destroy_border(border: HWND, drawer: Option<super::super::frame::FrameDrawer>) {
    drop(drawer);
    if !border.is_null() {
        unsafe {
            KillTimer(border, BORDER_TIMER);
            SetWindowLongPtrW(border, GWLP_USERDATA, 0);
            DestroyWindow(border);
        }
    }
}
pub(super) unsafe extern "system" fn border_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER if w == BORDER_TIMER => {
            let target = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as HWND;
            sync_border(target);
            0
        }
        // Colour and theme broadcasts reach every top-level window, rings included.
        WM_SETTINGCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED => {
            forget_ring_accent();
            unsafe { DefWindowProcW(hwnd, msg, w, l) }
        }
        WM_ERASEBKGND => 1,
        WM_SETCURSOR => {
            let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
            if !cursor.is_null() {
                unsafe {
                    SetCursor(cursor);
                }
            }
            1
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}
