//! RegisterHotKey slots for pin and opacity, with Win+Alt fallbacks for taken defaults.
use super::*;
pub(super) fn try_register(app: HWND, id: i32, key: Hotkey) -> bool {
    // SAFETY: app is our live top-level window; id is unique to this process.
    unsafe { RegisterHotKey(app, id, key.win32_mods(), u32::from(key.vk)) != 0 }
}
/// Label of the chord that is actually registered; `None` when every attempt failed.
pub(super) fn install_slot(app: HWND, id: i32, preferred: Hotkey) -> Option<String> {
    let is_default = preferred == Hotkey::pin_default()
        || preferred == Hotkey::opacity_up_default()
        || preferred == Hotkey::opacity_down_default();
    preferred
        .register_with_fallback(is_default, |key| try_register(app, id, key))
        .map(Hotkey::label)
}
pub(super) fn install_hotkey(app: HWND) {
    uninstall_hotkey(app);
    let (pin, up, down) = PIN.with(|s| {
        let s = s.borrow();
        (s.pin_key, s.up_key, s.down_key)
    });
    let pin = install_slot(app, HOTKEY_PIN, pin);
    let up = install_slot(app, HOTKEY_OPACITY_UP, up);
    let down = install_slot(app, HOTKEY_OPACITY_DOWN, down);
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.hotkey = pin;
        s.opacity_up = up;
        s.opacity_down = down;
    });
}
pub(super) fn uninstall_hotkey(app: HWND) {
    if !app.is_null() {
        unsafe {
            UnregisterHotKey(app, HOTKEY_PIN);
            UnregisterHotKey(app, HOTKEY_OPACITY_UP);
            UnregisterHotKey(app, HOTKEY_OPACITY_DOWN);
        }
    }
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.hotkey = None;
        s.opacity_up = None;
        s.opacity_down = None;
    });
}
