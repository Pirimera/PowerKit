//! Pinned-window transparency: layered-style apply, step and restore.
use super::*;
pub(in crate::platform::windows) fn step_foreground_opacity(delta: i32) -> bool {
    step_window_opacity(unsafe { GetForegroundWindow() }, delta)
}
/// Tray opacity items: the same window the tray pin item targets (the last eligible
/// foreground window before the menu opened) when it is pinned, otherwise the most
/// recently pinned window. Keeps the tray consistent with the opacity hotkeys.
pub(in crate::platform::windows) fn step_tray_opacity(delta: i32) -> bool {
    let hwnd = PIN.with(|s| {
        let s = s.borrow();
        let pins: Vec<HWND> = s.pins.iter().map(|pin| pin.hwnd).collect();
        tray_opacity_target(root_window(s.last), &pins)
    });
    step_window_opacity(hwnd, delta)
}
/// `remembered` when it is one of `pins`, else the last pin; null without pins.
pub(super) fn tray_opacity_target(remembered: HWND, pins: &[HWND]) -> HWND {
    if !remembered.is_null() && pins.contains(&remembered) {
        return remembered;
    }
    pins.last().copied().unwrap_or(null_mut())
}
pub(super) fn step_window_opacity(hwnd: HWND, delta: i32) -> bool {
    let hwnd = root_window(hwnd);
    if !is_pinned_window(hwnd) {
        return false;
    }
    let current = current_opacity(hwnd);
    let next = step_opacity(current, delta);
    if next == current || apply_window_alpha(hwnd, next).is_err() {
        return false;
    }
    play(if delta > 0 {
        Cue::Increase
    } else {
        Cue::Decrease
    });
    true
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
    if PIN.with(|s| {
        s.borrow()
            .pins
            .iter()
            .find(|pin| pin.hwnd == hwnd)
            .is_some_and(|pin| pin.layered.is_none())
    }) {
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
        PIN.with(|s| {
            if let Some(pin) = s.borrow_mut().pins.iter_mut().find(|pin| pin.hwnd == hwnd) {
                pin.layered = Some(restore);
            }
        });
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
pub(super) fn restore_window_alpha(hwnd: HWND) {
    let Some(original) = PIN.with(|s| {
        s.borrow()
            .pins
            .iter()
            .find(|pin| pin.hwnd == hwnd)
            .and_then(|pin| pin.layered)
    }) else {
        return;
    };
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
    PIN.with(|s| {
        if let Some(pin) = s.borrow_mut().pins.iter_mut().find(|pin| pin.hwnd == hwnd) {
            pin.layered = None;
        }
    });
}
pub(super) fn is_layered(hwnd: HWND) -> bool {
    live(hwnd) && unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0 }
}
