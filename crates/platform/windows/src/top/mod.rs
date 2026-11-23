//! UI-thread topmost ownership. HWND values are used only while IsWindow is true.
//! Pin lifecycle, state and public commands; the module map is in docs/always-top.md.
//!
//! The `powerkit_app` AlwaysTop controller decides which window a command acts
//! on and whether it pins or unpins. This module is its Windows port
//! ([`Desktop`]): it owns the pins, their frames and their transparency.
mod border;
mod eligibility;
mod hook;
mod opacity;
mod smoke;
use border::*;
use eligibility::*;
use hook::*;
pub use opacity::step_foreground_opacity;
pub use opacity::step_tray_opacity;
use opacity::*;
use powerkit_app::controller::always_top::{Controller, Cue, Error, WindowId};
use powerkit_app::ports::PinnableWindows;
use powerkit_core::always_top::{
    DEFAULT_FRAME_OPACITY, DEFAULT_FRAME_THICKNESS, MAX_OPACITY, MIN_OPACITY, OPACITY_STEP, Pin,
    alpha_from_percent, percent_from_alpha,
};
use powerkit_core::settings::Preferences;
pub use smoke::smoke;
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
    System::{
        LibraryLoader::GetModuleHandleW, SystemInformation::GetWindowsDirectoryW,
        Threading::GetCurrentProcessId,
    },
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::*,
    },
};
const BORDER_TIMER: usize = 123;
const BORDER_CLASS: &str = "PowerKit.AlwaysTop.Border";
thread_local! {
    static PIN: RefCell<PinState> = RefCell::new(PinState::default());
    /// Decides targets and toggles. Its state is in cells, so the window-event
    /// hook may reach it while a command is still being carried out.
    static TOP: Controller = const { Controller::new() };
}
fn id(hwnd: HWND) -> WindowId {
    WindowId(hwnd as usize as u64)
}
fn hwnd_of(window: WindowId) -> HWND {
    window.0 as usize as HWND
}
fn id_of(hwnd: HWND) -> Option<WindowId> {
    (!hwnd.is_null()).then(|| id(hwnd))
}
/// The controller's view of the desktop. `app` is the PowerKit app window,
/// which is never pinned.
struct Desktop {
    app: HWND,
}
impl Desktop {
    fn current() -> Self {
        Self {
            app: PIN.with(|s| s.borrow().app),
        }
    }
}
impl PinnableWindows for Desktop {
    fn foreground(&self) -> Option<WindowId> {
        id_of(root_window(unsafe { GetForegroundWindow() }))
    }
    fn is_own(&self, window: WindowId) -> bool {
        let hwnd = hwnd_of(window);
        live(hwnd) && own_process(hwnd)
    }
    fn eligible(&self, window: WindowId) -> bool {
        eligible(hwnd_of(window), self.app)
    }
    fn is_live(&self, window: WindowId) -> bool {
        live(hwnd_of(window))
    }
    fn pinned(&self) -> Vec<WindowId> {
        PIN.with(|s| s.borrow().pins.iter().map(|pin| id(pin.hwnd)).collect())
    }
    fn pin(&mut self, window: WindowId) -> io::Result<()> {
        pin_window(hwnd_of(window), self.app).map(|_| ())
    }
    fn unpin(&mut self, window: WindowId) -> io::Result<()> {
        unpin_window(hwnd_of(window))
    }
    fn opacity(&self, window: WindowId) -> i32 {
        current_opacity(hwnd_of(window))
    }
    fn set_opacity(&mut self, window: WindowId, percent: i32) -> io::Result<()> {
        apply_window_alpha(hwnd_of(window), percent)
    }
    fn title(&self, window: WindowId) -> String {
        title(hwnd_of(window))
    }
    fn play(&mut self, cue: Cue) {
        play(cue);
    }
}
/// The controller's refusal in the user's language; OS errors keep their text.
fn worded(error: Error) -> io::Error {
    match error {
        Error::NoWindow => io::Error::other(pin_none_message()),
        Error::Platform(error) => error,
    }
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
    app: HWND,
    /// Foreground/minimize, destroy..location change, and cloak hooks.
    hooks: [isize; 3],
    frame_color: Option<u32>,
    frame_opacity: i32,
    frame_thickness: i32,
    holding: bool,
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
            app: null_mut(),
            hooks: [0; 3],
            frame_color: None,
            frame_opacity: DEFAULT_FRAME_OPACITY,
            frame_thickness: DEFAULT_FRAME_THICKNESS,
            holding: false,
        }
    }
}
// Rc's marker makes the guard !Send and !Sync: restore must stay on its creator's thread.
#[derive(Default)]
pub struct PinGuard(std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for PinGuard {
    fn drop(&mut self) {
        shutdown();
    }
}
pub fn session() -> Pin {
    TOP.with(|top| top.status(&Desktop::current()))
}
pub fn apply_prefs(prefs: &Preferences) {
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.frame_color = prefs.frame_color.map(super::color::colorref);
        s.frame_opacity = prefs.frame_opacity;
        s.frame_thickness = prefs.frame_thickness;
    });
    TOP.with(|top| top.set_sound(prefs.sound));
    let app = PIN.with(|s| s.borrow().app);
    if !app.is_null() {
        refresh_frames();
    }
}
pub fn listen(app: HWND) {
    PIN.with(|s| s.borrow_mut().app = app);
    install_hooks();
}
pub fn shutdown() {
    uninstall();
    let _ = release();
    super::frame::release_factory();
}
/// Tracks the foreground window and returns the last eligible one (or null).
pub fn remember_foreground(app: HWND) -> HWND {
    resolve_target(app, unsafe { GetForegroundWindow() })
}
pub fn offer(hwnd: HWND, app: HWND) -> bool {
    match id_of(root_window(hwnd)) {
        Some(root) => TOP.with(|top| top.offer(&Desktop { app }, root)),
        None => false,
    }
}
pub fn forget() {
    TOP.with(Controller::forget);
}
pub fn reconcile() -> bool {
    TOP.with(|top| top.reconcile(&mut Desktop::current()))
}
/// The pin shortcut: PowerToys ProcessCommand toggles only the foreground window.
pub fn toggle_hotkey(app: HWND) -> io::Result<Pin> {
    TOP.with(|top| top.toggle_shortcut(&mut Desktop { app }))
        .map_err(worded)
}
pub fn toggle(app: HWND) -> io::Result<Pin> {
    toggle_from(app, unsafe { GetForegroundWindow() })
}
/// Pins or unpins the remembered window, or `candidate` when none is remembered.
/// The smoke check passes null so that an unrelated foreground window is never pinned.
fn toggle_from(app: HWND, candidate: HWND) -> io::Result<Pin> {
    let candidate = id_of(root_window(candidate));
    TOP.with(|top| top.toggle_from(&mut Desktop { app }, candidate))
        .map_err(worded)
}
fn pin_none_message() -> String {
    let t = super::ui_strings();
    t.pin_none.replace(
        "{hotkey}",
        super::hotkeys::active_label(powerkit_app::controller::hotkeys::Slot::Pin)
            .as_deref()
            .unwrap_or(t.hotkey_pin),
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
    if let Err(error) = set_topmost(target, true) {
        // set_topmost marks first; a failed pin must not leave the property behind.
        mark_pinned(target, false);
        return Err(error);
    }
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
/// Unpins the tracked window `target` (exact HWND, live or not). The pin leaves the
/// list before anything is restored, so a failed restore never leaves a pin
/// without its ring or timer behind.
fn unpin_window(target: HWND) -> io::Result<()> {
    let Some(pin) = take_pin(target) else {
        return Ok(());
    };
    destroy_border(pin.border, pin.drawer);
    if !live(target) {
        return Ok(());
    }
    if let Some(original) = pin.layered {
        restore_layered(target, original);
    }
    // A window that was already topmost keeps the flag but not our property.
    mark_pinned(target, false);
    if pin.restore {
        set_topmost(target, false)?;
    }
    Ok(())
}
fn take_pin(target: HWND) -> Option<Tracked> {
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        let index = s.pins.iter().position(|pin| pin.hwnd == target)?;
        Some(s.pins.remove(index))
    })
}
/// Unpins every window, even after one fails, and reports the first failure.
pub fn release() -> io::Result<()> {
    TOP.with(|top| top.release(&mut Desktop::current()))
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
/// Plays the Windows Media cue PowerToys uses; the controller decides whether
/// sound is on.
fn play(cue: Cue) {
    let name = match cue {
        Cue::PinOn => "Speech On.wav",
        Cue::PinOff => "Speech Sleep.wav",
        Cue::OpacityUp => "Windows Hardware Insert.wav",
        Cue::OpacityDown => "Windows Hardware Remove.wav",
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
/// Whether the live root of `hwnd` is pinned (for hotkeys and moves inside a window).
fn is_pinned_window(hwnd: HWND) -> bool {
    let root = root_window(hwnd);
    live(root) && tracked(root)
}
/// Whether exactly `hwnd` is pinned, even after it was destroyed.
fn tracked(hwnd: HWND) -> bool {
    !hwnd.is_null() && with_pin(hwnd, |_| ()).is_some()
}
/// Runs `f` on the pin of exactly `hwnd`. `f` must not call Win32 or D2D: the pin
/// state is borrowed while it runs.
fn with_pin<R>(hwnd: HWND, f: impl FnOnce(&Tracked) -> R) -> Option<R> {
    PIN.with(|s| s.borrow().pins.iter().find(|pin| pin.hwnd == hwnd).map(f))
}
/// Mutable counterpart of `with_pin`, with the same rule about Win32 and D2D calls.
fn with_pin_mut<R>(hwnd: HWND, f: impl FnOnce(&mut Tracked) -> R) -> Option<R> {
    PIN.with(|s| {
        s.borrow_mut()
            .pins
            .iter_mut()
            .find(|pin| pin.hwnd == hwnd)
            .map(f)
    })
}
fn uninstall() {
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
        assert!(resolve_target(null_mut(), null_mut()).is_null());
    }
    #[test]
    fn windows_of_this_process_are_never_eligible() {
        let class = crate::wide("STATIC");
        // SAFETY: a hidden system-class window created and destroyed on this test thread.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_OVERLAPPED,
                0,
                0,
                10,
                10,
                null_mut(),
                null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        assert!(own_process(hwnd));
        assert!(!eligible(hwnd, null_mut()));
        // The controller ignores the pin shortcut over a window the port reports
        // as PowerKit's own, and reports the desktop as not pinnable.
        let desktop = Desktop { app: null_mut() };
        assert!(desktop.is_own(id(hwnd)) && desktop.is_live(id(hwnd)));
        assert!(!desktop.eligible(id(hwnd)));
        assert_eq!(hwnd_of(id(hwnd)), hwnd);
        unsafe {
            DestroyWindow(hwnd);
            assert!(!own_process(GetDesktopWindow()));
            let shell = id(GetDesktopWindow());
            assert!(!desktop.is_own(shell) && !desktop.eligible(shell));
        }
        // A destroyed window is neither own nor live any more.
        assert!(!desktop.is_own(id(hwnd)) && !desktop.is_live(id(hwnd)));
        assert_eq!(id_of(null_mut()), None);
    }
    #[test]
    fn resolve_does_not_invent_a_z_order_window_without_a_remembered_target() {
        forget();
        assert!(resolve_target(null_mut(), null_mut()).is_null());
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
