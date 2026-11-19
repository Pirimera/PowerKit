//! UI-thread topmost ownership. HWND values are used only while IsWindow is true.
//! Pin lifecycle, state and public commands; the module map is in docs/always-top.md.
mod border;
mod eligibility;
mod hook;
mod hotkeys;
mod opacity;
mod smoke;
use crate::always_top::{
    DEFAULT_FRAME_OPACITY, DEFAULT_FRAME_THICKNESS, Hotkey, MAX_OPACITY, MIN_OPACITY, OPACITY_STEP,
    Pin, alpha_from_percent, percent_from_alpha, step_opacity,
};
use crate::settings::Preferences;
use border::*;
use eligibility::*;
use hook::*;
use hotkeys::*;
pub(super) use opacity::step_foreground_opacity;
pub(super) use opacity::step_pinned_opacity;
use opacity::*;
pub(super) use smoke::smoke;
use std::{cell::RefCell, io, ptr::null_mut, sync::OnceLock};
use windows_sys::Win32::{
    Foundation::{ERROR_CLASS_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND, DWMWA_CLOAKED,
            DWMWA_EXCLUDED_FROM_PEEK, DWMWA_EXTENDED_FRAME_BOUNDS, DwmEnableBlurBehindWindow,
            DwmGetWindowAttribute, DwmSetWindowAttribute,
        },
        Gdi::{CreateRectRgn, DeleteObject},
    },
    Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT},
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetWindowsDirectoryW},
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent},
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey},
        WindowsAndMessaging::*,
    },
};
pub(super) const HOTKEY_PIN: i32 = 1;
pub(super) const HOTKEY_OPACITY_UP: i32 = 2;
pub(super) const HOTKEY_OPACITY_DOWN: i32 = 3;
const BORDER_TIMER: usize = 123;
const BORDER_CLASS: &str = "PowerKit.AlwaysTop.Border";
thread_local! {
    static PIN: RefCell<PinState> = RefCell::new(PinState::default());
}
struct Tracked {
    hwnd: HWND,
    restore: bool,
    border: HWND,
    drawer: Option<super::frame::FrameDrawer>,
    layered: Option<LayeredRestore>,
}
struct PinState {
    pins: Vec<Tracked>,
    last: HWND,
    discover: bool,
    app: HWND,
    hook_focus: isize,
    hook_target: isize,
    hotkey: Option<String>,
    opacity_up: Option<String>,
    opacity_down: Option<String>,
    pin_key: Hotkey,
    up_key: Hotkey,
    down_key: Hotkey,
    frame_color: Option<u32>,
    frame_opacity: i32,
    frame_thickness: i32,
    holding: bool,
    sound: bool,
}
#[derive(Clone, Copy)]
struct LayeredRestore {
    had_layered: bool,
    alpha: u8,
    color_key: u32,
    used_color_key: bool,
}
impl Default for PinState {
    fn default() -> Self {
        Self {
            pins: Vec::new(),
            last: null_mut(),
            discover: true,
            app: null_mut(),
            hook_focus: 0,
            hook_target: 0,
            hotkey: None,
            opacity_up: None,
            opacity_down: None,
            pin_key: Hotkey::pin_default(),
            up_key: Hotkey::opacity_up_default(),
            down_key: Hotkey::opacity_down_default(),
            frame_color: None,
            frame_opacity: DEFAULT_FRAME_OPACITY,
            frame_thickness: DEFAULT_FRAME_THICKNESS,
            holding: false,
            sound: false,
        }
    }
}
// Rc's marker makes the guard !Send and !Sync: restore must stay on its creator's thread.
#[derive(Default)]
pub(super) struct PinGuard(std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for PinGuard {
    fn drop(&mut self) {
        shutdown();
    }
}
pub(super) fn session() -> Pin {
    PIN.with(|s| {
        let s = s.borrow();
        let last = s.pins.last().map(|pin| title(pin.hwnd)).unwrap_or_default();
        Pin::summary(s.pins.len(), last)
    })
}
pub(super) fn hotkey_label() -> Option<String> {
    PIN.with(|s| s.borrow().hotkey.clone())
}
pub(super) fn opacity_hotkeys() -> (Option<String>, Option<String>) {
    PIN.with(|s| {
        (
            s.borrow().opacity_up.clone(),
            s.borrow().opacity_down.clone(),
        )
    })
}
pub(super) fn apply_prefs(prefs: &Preferences) {
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.pin_key = prefs.pin;
        s.up_key = prefs.opacity_up;
        s.down_key = prefs.opacity_down;
        s.frame_color = prefs.frame_color;
        s.frame_opacity = prefs.frame_opacity;
        s.frame_thickness = prefs.frame_thickness;
        s.sound = prefs.sound;
    });
    let app = PIN.with(|s| s.borrow().app);
    if !app.is_null() {
        install_hotkey(app);
        refresh_frames();
    }
}
pub(super) fn sound_enabled() -> bool {
    PIN.with(|s| s.borrow().sound)
}
pub(super) fn listen(app: HWND) {
    PIN.with(|s| s.borrow_mut().app = app);
    install_hooks();
    install_hotkey(app);
}
pub(super) fn pause_hotkeys() {
    let app = PIN.with(|s| s.borrow().app);
    uninstall_hotkey(app);
}
pub(super) fn shutdown() {
    uninstall();
    let _ = release();
}
/// Tracks the foreground window and returns the last eligible one (or null).
pub(super) fn remember_foreground(app: HWND) -> HWND {
    resolve_target(app)
}
pub(super) fn offer(hwnd: HWND, app: HWND) -> bool {
    let root = root_window(hwnd);
    if !eligible(root, app) {
        return false;
    }
    PIN.with(|s| s.borrow_mut().last = root);
    true
}
pub(super) fn forget() {
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.last = null_mut();
        s.discover = false;
    });
}
pub(super) fn reconcile() -> bool {
    let dead: Vec<HWND> = PIN.with(|s| {
        s.borrow()
            .pins
            .iter()
            .filter(|pin| !live(pin.hwnd))
            .map(|pin| pin.hwnd)
            .collect()
    });
    if dead.is_empty() {
        return false;
    }
    for hwnd in dead {
        let _ = unpin_window(hwnd);
    }
    true
}
pub(super) fn toggle_hotkey(app: HWND) -> io::Result<Pin> {
    // PowerToys ProcessCommand: toggle only the foreground window.
    process_command(root_window(unsafe { GetForegroundWindow() }), app)
}
pub(super) fn toggle(app: HWND) -> io::Result<Pin> {
    reconcile();
    process_command(resolve_target(app), app)
}
fn process_command(target: HWND, app: HWND) -> io::Result<Pin> {
    if !eligible(target, app) {
        return Err(io::Error::other(pin_none_message()));
    }
    if is_pinned_window(target) {
        unpin_window(target)?;
        play(Cue::Off);
    } else {
        pin_window(target, app)?;
        play(Cue::On);
    }
    Ok(session())
}
fn pin_none_message() -> String {
    let t = super::ui_strings();
    t.pin_none.replace(
        "{hotkey}",
        hotkey_label().as_deref().unwrap_or(t.hotkey_pin),
    )
}
fn pin_window(target: HWND, app: HWND) -> io::Result<Pin> {
    let target = root_window(target);
    if !live(target) || target == app || owned_by(target, app) {
        return Err(io::Error::other(pin_none_message()));
    }
    if is_pinned_window(target) {
        return Ok(session());
    }
    let already = is_topmost(target);
    set_topmost(target, true)?;
    PIN.with(|s| {
        s.borrow_mut().pins.push(Tracked {
            hwnd: target,
            restore: !already,
            border: null_mut(),
            drawer: None,
            layered: None,
        });
    });
    show_border(target);
    Ok(session())
}
fn unpin_window(target: HWND) -> io::Result<()> {
    let target = root_window(target);
    hide_border_for(target);
    if live(target) {
        restore_window_alpha(target);
    }
    let restore = PIN.with(|s| {
        s.borrow()
            .pins
            .iter()
            .find(|pin| pin.hwnd == target)
            .map(|pin| pin.restore)
            .unwrap_or(false)
    });
    if restore && live(target) {
        set_topmost(target, false)?;
    }
    PIN.with(|s| s.borrow_mut().pins.retain(|pin| pin.hwnd != target));
    Ok(())
}
pub(super) fn release() -> io::Result<()> {
    let hwnds: Vec<HWND> = PIN.with(|s| s.borrow().pins.iter().map(|pin| pin.hwnd).collect());
    for hwnd in hwnds {
        unpin_window(hwnd)?;
    }
    Ok(())
}
fn is_topmost(hwnd: HWND) -> bool {
    live(hwnd) && unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 }
}
fn pinned_prop() -> *const u16 {
    static NAME: OnceLock<Vec<u16>> = OnceLock::new();
    NAME.get_or_init(|| super::wide("PowerKit.AlwaysTop.Pinned"))
        .as_ptr()
}
fn mark_pinned(hwnd: HWND, pinned: bool) {
    // SAFETY: property name is process-static; hwnd is a live top-level window.
    unsafe {
        if pinned {
            SetPropW(hwnd, pinned_prop(), 1 as HANDLE);
        } else {
            RemovePropW(hwnd, pinned_prop());
        }
    }
}
fn set_topmost(hwnd: HWND, topmost: bool) -> io::Result<()> {
    // PowerToys PinTopmostWindow / UnpinTopmostWindow: SetProp plus SetWindowPos
    // without SWP_NOACTIVATE so Explorer keeps the topmost band.
    mark_pinned(hwnd, topmost);
    let insert = if topmost {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    // SAFETY: hwnd is a live top-level window; call stays on the UI thread that owns the pin.
    if unsafe { SetWindowPos(hwnd, insert, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE) } == 0 {
        Err(io::Error::other(if topmost {
            super::ui_strings().pin_failed
        } else {
            super::ui_strings().unpin_failed
        }))
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Cue {
    On,
    Off,
    Increase,
    Decrease,
}
fn play(cue: Cue) {
    if !sound_enabled() {
        return;
    }
    let name = match cue {
        Cue::On => "Speech On.wav",
        Cue::Off => "Speech Sleep.wav",
        Cue::Increase => "Windows Hardware Insert.wav",
        Cue::Decrease => "Windows Hardware Remove.wav",
    };
    let Some(path) = media_wav(name) else {
        return;
    };
    // SAFETY: path is a live NUL-terminated Windows Media file; PlaySound copies it when async.
    unsafe {
        PlaySoundW(
            path.as_ptr(),
            null_mut(),
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        );
    }
}
fn media_wav(name: &str) -> Option<Vec<u16>> {
    let mut buf = [0u16; 260];
    let len = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 || (len as usize) >= buf.len() {
        return None;
    }
    let dir = String::from_utf16_lossy(&buf[..len as usize]);
    Some(super::wide(&format!("{dir}\\Media\\{name}")))
}
fn is_pinned_window(hwnd: HWND) -> bool {
    let root = root_window(hwnd);
    live(root) && PIN.with(|s| s.borrow().pins.iter().any(|pin| pin.hwnd == root))
}
fn uninstall() {
    let app = PIN.with(|s| s.borrow().app);
    uninstall_hotkey(app);
    uninstall_hooks();
    PIN.with(|s| s.borrow_mut().app = null_mut());
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_desktop_and_own_window_are_not_eligible() {
        assert!(!eligible(null_mut(), null_mut()));
        unsafe {
            let desktop = GetDesktopWindow();
            assert!(!eligible(desktop, desktop));
            assert!(!eligible(desktop, null_mut()));
        }
        let _guard = PinGuard::default();
        assert!(!session().active());
        forget();
        assert!(resolve_target(null_mut()).is_null());
    }
    #[test]
    fn resolve_does_not_invent_a_z_order_window_without_a_remembered_target() {
        forget();
        assert!(resolve_target(null_mut()).is_null());
    }
    #[test]
    fn explorer_is_not_a_shell_surface() {
        assert!(!shell_class("CabinetWClass"));
        assert!(!shell_class("ExploreWClass"));
        assert!(!shell_class("Notepad"));
        assert!(shell_class("Shell_TrayWnd"));
        assert!(shell_class("PowerKit.AlwaysTop.Border"));
    }

    #[test]
    fn frame_pad_scales_without_collapsing_to_zero() {
        assert_eq!(scaled_frame_pad(4, 1.0), 4);
        assert_eq!(scaled_frame_pad(4, 1.5), 6);
        assert_eq!(scaled_frame_pad(1, 0.1), 1);
    }
}
