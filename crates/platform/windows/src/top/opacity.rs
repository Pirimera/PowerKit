//! Pinned-window transparency: layered-style apply, step and restore.
use super::*;
/// The opacity shortcuts: steps the foreground window when it is pinned.
pub fn step_foreground_opacity(delta: i32) -> bool {
    TOP.with(|top| top.step_foreground_opacity(&mut Desktop::current(), delta))
}
/// The tray opacity items; the controller picks the window they act on.
pub fn step_tray_opacity(delta: i32) -> bool {
    TOP.with(|top| top.step_tray_opacity(&mut Desktop::current(), delta))
}
pub(super) fn step_window_opacity(hwnd: HWND, delta: i32) -> bool {
    match id_of(root_window(hwnd)) {
        Some(window) => TOP.with(|top| top.step_opacity(&mut Desktop::current(), window, delta)),
        None => false,
    }
}
pub(super) fn current_opacity(hwnd: HWND) -> i32 {
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if ex & WS_EX_LAYERED == 0 {
        return MAX_OPACITY;
    }
    let mut alpha = 255u8;
    if unsafe { GetLayeredWindowAttributes(hwnd, null_mut(), &mut alpha, null_mut()) } != 0 {
        percent_from_alpha(alpha)
    } else {
        MAX_OPACITY
    }
}
pub(super) fn apply_window_alpha(hwnd: HWND, percent: i32) -> io::Result<()> {
    let percent = percent.clamp(MIN_OPACITY, MAX_OPACITY);
    let mut ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let layered = ex as u32 & WS_EX_LAYERED != 0;
    if with_pin(hwnd, |pin| pin.layered.is_none()).unwrap_or(false) {
        let mut restore = LayeredRestore {
            had_layered: layered,
            alpha: 255,
            color_key: 0,
            used_color_key: false,
        };
        if layered {
            let mut alpha = 255u8;
            let mut color_key = 0u32;
            let mut flags = 0u32;
            if unsafe { GetLayeredWindowAttributes(hwnd, &mut color_key, &mut alpha, &mut flags) }
                == 0
            {
                return Err(super::super::fault::Fault::OpacityUnreadable.into());
            }
            restore.alpha = alpha;
            restore.color_key = color_key;
            restore.used_color_key = flags & LWA_COLORKEY != 0;
        }
        with_pin_mut(hwnd, |pin| pin.layered = Some(restore));
    }
    if layered {
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED as isize));
            SetWindowPos(
                hwnd,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
        ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    }
    // SAFETY: hwnd is the live pinned window; alpha is applied only after caching restore state.
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
        SetLayeredWindowAttributes(hwnd, 0, alpha_from_percent(percent), LWA_ALPHA);
    }
    Ok(())
}
/// Puts back the layered style and attributes captured before the first opacity step;
/// the caller has already removed the pin.
pub(super) fn restore_layered(hwnd: HWND, original: LayeredRestore) {
    let mut ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    if original.had_layered {
        if ex as u32 & WS_EX_LAYERED != 0 {
            unsafe {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED as isize));
            }
            ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        }
        let flags = if original.used_color_key {
            LWA_ALPHA | LWA_COLORKEY
        } else {
            LWA_ALPHA
        };
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
            SetLayeredWindowAttributes(hwnd, original.color_key, original.alpha, flags);
            SetWindowPos(
                hwnd,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
    } else if ex as u32 & WS_EX_LAYERED != 0 {
        unsafe {
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED as isize));
            SetWindowPos(
                hwnd,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
    }
}
pub(super) fn is_layered(hwnd: HWND) -> bool {
    live(hwnd) && unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0 }
}
