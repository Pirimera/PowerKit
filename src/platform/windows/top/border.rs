//! Border HWND lifecycle for pinned windows: D2D frame paint, z-order glue behind the target, show/hide and the backup timer.
use super::*;
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
        rgb: color.unwrap_or_else(super::super::frame::accent_color),
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
pub(super) fn show_border(target: HWND) {
    if unsafe { IsWindowVisible(target) } == 0 {
        return;
    }
    hide_border_for(target);
    let Some(rect) = frame_rect(target) else {
        return;
    };
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = super::super::wide(BORDER_CLASS);
    let wnd = WNDCLASSW {
        lpfnWndProc: Some(border_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&wnd) } == 0 {
        let err = std::io::Error::last_os_error().raw_os_error();
        if err != Some(ERROR_CLASS_ALREADY_EXISTS as i32) {
            return;
        }
    }
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
    PIN.with(|s| {
        if let Some(pin) = s
            .borrow_mut()
            .pins
            .iter_mut()
            .find(|pin| pin.hwnd == target)
        {
            pin.border = hwnd;
            pin.drawer = Some(drawer);
        }
    });
    PIN.with(|s| {
        if let Some(pin) = s.borrow().pins.iter().find(|pin| pin.hwnd == target)
            && let Some(drawer) = pin.drawer.as_ref()
        {
            drawer.show();
        }
    });
    glue_border_for(target);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, target as isize);
        SetTimer(hwnd, BORDER_TIMER, 100, None);
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
    let (border, target) = PIN.with(|s| {
        s.borrow()
            .pins
            .iter()
            .find(|pin| pin.hwnd == target)
            .map(|pin| (pin.border, pin.hwnd))
            .unwrap_or((null_mut(), null_mut()))
    });
    if !live(border) || !live(target) {
        return;
    }
    let Some(rect) = frame_rect(target) else {
        PIN.with(|s| {
            if let Some(pin) = s.borrow().pins.iter().find(|pin| pin.hwnd == target)
                && let Some(drawer) = pin.drawer.as_ref()
            {
                drawer.hide();
            }
        });
        return;
    };
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    unsafe {
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
    PIN.with(|s| {
        if let Some(pin) = s
            .borrow_mut()
            .pins
            .iter_mut()
            .find(|pin| pin.hwnd == target)
            && let Some(drawer) = pin.drawer.as_mut()
        {
            drawer.paint(local, paint.rgb, paint.alpha, paint.thickness, radius);
        }
    });
}
pub(super) fn sync_border(target: HWND) {
    let target = root_window(target);
    if !is_pinned_window(target) {
        return;
    }
    if !live(target) {
        let _ = unpin_window(target);
        return;
    }
    let border = PIN.with(|s| {
        s.borrow()
            .pins
            .iter()
            .find(|pin| pin.hwnd == target)
            .map(|pin| pin.border)
            .unwrap_or(null_mut())
    });
    if border.is_null() {
        if unsafe { IsWindowVisible(target) } != 0 {
            show_border(target);
        }
        return;
    }
    if live(target) && !is_topmost(target) {
        restore_pin_if_dropped();
    }
    glue_border_for(target);
}
pub(super) fn hide_border_for(target: HWND) {
    let (border, drawer) = PIN.with(|s| {
        if let Some(pin) = s
            .borrow_mut()
            .pins
            .iter_mut()
            .find(|pin| pin.hwnd == target)
        {
            (
                std::mem::replace(&mut pin.border, null_mut()),
                pin.drawer.take(),
            )
        } else {
            (null_mut(), None)
        }
    });
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
            unsafe {
                KillTimer(hwnd, BORDER_TIMER);
                SetTimer(hwnd, BORDER_TIMER, 100, None);
            }
            let target = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as HWND;
            sync_border(target);
            0
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
