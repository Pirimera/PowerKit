//! UI-thread topmost ownership. HWND values are used only while IsWindow is true.
use crate::always_top::{
    DEFAULT_FRAME_OPACITY, DEFAULT_FRAME_THICKNESS, Hotkey, MAX_OPACITY, MIN_OPACITY, OPACITY_STEP,
    Pin, alpha_from_percent, percent_from_alpha, step_opacity,
};
use crate::settings::Preferences;
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
#[derive(Clone, Copy)]
struct FramePaint {
    rgb: u32,
    alpha: f32,
    thickness: i32,
}
fn frame_paint(hwnd: HWND) -> FramePaint {
    let (color, opacity, thickness) = PIN.with(|s| {
        let s = s.borrow();
        (s.frame_color, s.frame_opacity, s.frame_thickness)
    });
    FramePaint {
        rgb: color.unwrap_or_else(super::frame::accent_color),
        alpha: opacity as f32 / 100.0,
        thickness: scaled_frame_pad(thickness, super::frame::scale_factor(hwnd)),
    }
}
fn scaled_frame_pad(thickness: i32, scale: f32) -> i32 {
    (thickness as f32 * scale).round().max(1.0) as i32
}
fn refresh_frames() {
    let targets: Vec<HWND> = PIN.with(|s| s.borrow().pins.iter().map(|pin| pin.hwnd).collect());
    for target in targets {
        glue_border_for(target);
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
fn resolve_target(app: HWND) -> HWND {
    let last = PIN.with(|s| s.borrow().last);
    if eligible(last, app) {
        return last;
    }
    if !PIN.with(|s| s.borrow().discover) {
        return null_mut();
    }
    if offer(unsafe { GetForegroundWindow() }, app) {
        return PIN.with(|s| s.borrow().last);
    }
    null_mut()
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
fn live(hwnd: HWND) -> bool {
    !hwnd.is_null() && unsafe { IsWindow(hwnd) } != 0
}
fn root_window(hwnd: HWND) -> HWND {
    if hwnd.is_null() {
        return hwnd;
    }
    let ancestor = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if ancestor.is_null() { hwnd } else { ancestor }
}
fn owned_by(hwnd: HWND, owner: HWND) -> bool {
    let mut current = hwnd;
    while !current.is_null() {
        if current == owner {
            return true;
        }
        current = unsafe { GetWindow(current, GW_OWNER) };
    }
    false
}
fn cloaked(hwnd: HWND) -> bool {
    let mut value = 0u32;
    // SAFETY: value is a live DWORD; hwnd is a window IsWindow accepted.
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, (&raw mut value).cast(), 4) == 0
            && value != 0
    }
}
fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len as usize])
    }
}
fn shell_class(name: &str) -> bool {
    matches!(
        name,
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "Progman"
            | "WorkerW"
            | "ForegroundStaging"
            | "#32768"
            | "PowerKit.AlwaysTop.Border"
            | "PowerKit.ScreenRuler.Overlay"
            | "PowerKit.ScreenRuler.Toolbar"
    )
}
fn shell_surface(hwnd: HWND) -> bool {
    shell_class(&class_name(hwnd))
}
fn eligible(hwnd: HWND, app: HWND) -> bool {
    if !live(hwnd) || hwnd == app || owned_by(hwnd, app) {
        return false;
    }
    unsafe {
        if IsWindowVisible(hwnd) == 0 || hwnd == GetDesktopWindow() {
            return false;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_CHILD != 0 || ex & WS_EX_TOOLWINDOW != 0 {
            return false;
        }
    }
    !cloaked(hwnd) && !shell_surface(hwnd)
}
fn title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len as usize])
    }
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
pub(super) fn step_foreground_opacity(delta: i32) -> bool {
    step_window_opacity(unsafe { GetForegroundWindow() }, delta)
}
pub(super) fn step_pinned_opacity(delta: i32) -> bool {
    let hwnd = PIN.with(|s| {
        s.borrow()
            .pins
            .last()
            .map(|pin| pin.hwnd)
            .unwrap_or(null_mut())
    });
    step_window_opacity(hwnd, delta)
}
fn step_window_opacity(hwnd: HWND, delta: i32) -> bool {
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
fn current_opacity(hwnd: HWND) -> i32 {
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
fn apply_window_alpha(hwnd: HWND, percent: i32) -> io::Result<()> {
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
                return Err(super::fault::Fault::OpacityUnreadable.into());
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
fn restore_window_alpha(hwnd: HWND) {
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
fn is_layered(hwnd: HWND) -> bool {
    live(hwnd) && unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0 }
}
fn restore_pin_if_dropped() {
    if PIN.with(|s| s.borrow().holding) {
        return;
    }
    let hwnds: Vec<HWND> = PIN.with(|s| s.borrow().pins.iter().map(|pin| pin.hwnd).collect());
    PIN.with(|s| s.borrow_mut().holding = true);
    for hwnd in hwnds {
        if live(hwnd) && !is_topmost(hwnd) {
            let _ = set_topmost(hwnd, true);
            glue_border_for(hwnd);
        }
    }
    PIN.with(|s| s.borrow_mut().holding = false);
}
fn reapply_pin(hwnd: HWND) {
    if PIN.with(|s| s.borrow().holding) || !is_pinned_window(hwnd) || !live(hwnd) {
        return;
    }
    PIN.with(|s| s.borrow_mut().holding = true);
    let _ = set_topmost(hwnd, true);
    glue_border_for(hwnd);
    PIN.with(|s| s.borrow_mut().holding = false);
}
fn install_hooks() {
    uninstall_hooks();
    // SAFETY: callback only reads thread-local pin state on this UI thread's message loop.
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    let focus = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_MINIMIZEEND,
            null_mut(),
            Some(on_win_event),
            0,
            0,
            flags,
        )
    };
    let target = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_LOCATIONCHANGE,
            null_mut(),
            Some(on_win_event),
            0,
            0,
            flags,
        )
    };
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.hook_focus = focus as isize;
        s.hook_target = target as isize;
    });
}
fn uninstall_hooks() {
    let (focus, target) = PIN.with(|s| {
        let s = s.borrow();
        (s.hook_focus, s.hook_target)
    });
    // SAFETY: hooks were installed on this thread and are unhooked once.
    unsafe {
        if focus != 0 {
            UnhookWinEvent(focus as _);
        }
        if target != 0 {
            UnhookWinEvent(target as _);
        }
    }
    PIN.with(|s| {
        let mut s = s.borrow_mut();
        s.hook_focus = 0;
        s.hook_target = 0;
    });
}
fn try_register(app: HWND, id: i32, key: Hotkey) -> bool {
    // SAFETY: app is our live top-level window; id is unique to this process.
    unsafe { RegisterHotKey(app, id, key.win32_mods(), u32::from(key.vk)) != 0 }
}
fn fallback_alt(key: Hotkey) -> Option<Hotkey> {
    if key.ctrl && !key.alt {
        Hotkey::new(key.win, false, true, key.shift, key.vk)
    } else {
        None
    }
}
fn install_slot(app: HWND, id: i32, preferred: Hotkey) -> Option<String> {
    if try_register(app, id, preferred) {
        return Some(preferred.label());
    }
    let is_default = preferred == Hotkey::pin_default()
        || preferred == Hotkey::opacity_up_default()
        || preferred == Hotkey::opacity_down_default();
    if is_default
        && let Some(alt) = fallback_alt(preferred)
        && try_register(app, id, alt)
    {
        return Some(alt.label());
    }
    Some(preferred.label())
}
fn install_hotkey(app: HWND) {
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
fn uninstall_hotkey(app: HWND) {
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
fn uninstall() {
    let app = PIN.with(|s| s.borrow().app);
    uninstall_hotkey(app);
    uninstall_hooks();
    PIN.with(|s| s.borrow_mut().app = null_mut());
}
unsafe extern "system" fn on_win_event(
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
    match event {
        EVENT_SYSTEM_FOREGROUND => {
            let _ = offer(hwnd, app);
            restore_pin_if_dropped();
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
        EVENT_OBJECT_DESTROY => {
            let target = root_window(hwnd);
            if is_pinned_window(target) {
                let _ = unpin_window(target);
            }
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
fn frame_rect(hwnd: HWND) -> Option<RECT> {
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
fn show_border(target: HWND) {
    if unsafe { IsWindowVisible(target) } == 0 {
        return;
    }
    hide_border_for(target);
    let Some(rect) = frame_rect(target) else {
        return;
    };
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = super::wide(BORDER_CLASS);
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
    let Some(drawer) = super::frame::FrameDrawer::create(hwnd) else {
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
fn make_border_transparent(hwnd: HWND) -> bool {
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
fn glue_border_for(target: HWND) {
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
    let radius = super::frame::corner_radius(target);
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
fn sync_border(target: HWND) {
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
fn hide_border_for(target: HWND) {
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
unsafe extern "system" fn border_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
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
pub(super) fn smoke(app: HWND) -> io::Result<()> {
    struct Fixture {
        hwnd: HWND,
        class: Vec<u16>,
        instance: windows_sys::Win32::Foundation::HINSTANCE,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                if !self.hwnd.is_null() {
                    DestroyWindow(self.hwnd);
                }
                UnregisterClassW(self.class.as_ptr(), self.instance);
            }
        }
    }
    forget();
    if toggle(app).is_ok() {
        let _ = release();
        return Err(io::Error::other("Aday yokken AlwaysTop açılmamalı"));
    }
    if session().active() {
        return Err(io::Error::other("Başarısız sabitleme oturumu açık bıraktı"));
    }
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = super::wide(&format!("PowerKit.Smoke.AlwaysTop.{}", std::process::id()));
    let wnd = WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&wnd) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut fixture = Fixture {
        hwnd: unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                super::wide("AlwaysTop fixture").as_ptr(),
                WS_OVERLAPPED | WS_CAPTION,
                -32_000,
                -32_000,
                240,
                80,
                null_mut(),
                null_mut(),
                instance,
                std::ptr::null(),
            )
        },
        class,
        instance,
    };
    if fixture.hwnd.is_null() {
        return Err(io::Error::last_os_error());
    }
    if pin_window(fixture.hwnd, app)? != Pin::on("AlwaysTop fixture") || !is_topmost(fixture.hwnd) {
        return Err(io::Error::other("AlwaysTop sabitleme uygulanmadı"));
    }
    if !step_window_opacity(fixture.hwnd, -OPACITY_STEP) || !is_layered(fixture.hwnd) {
        return Err(io::Error::other("AlwaysTop saydamlık uygulanmadı"));
    }
    let mut alpha = 0u8;
    if unsafe { GetLayeredWindowAttributes(fixture.hwnd, null_mut(), &mut alpha, null_mut()) } == 0
        || alpha != alpha_from_percent(90)
    {
        return Err(io::Error::other("AlwaysTop saydamlık uygulanmadı"));
    }
    unpin_window(fixture.hwnd)?;
    if session().active() || is_topmost(fixture.hwnd) || is_layered(fixture.hwnd) {
        return Err(io::Error::other("AlwaysTop sabitleme geri alınmadı"));
    }
    let second = unsafe {
        CreateWindowExW(
            0,
            fixture.class.as_ptr(),
            super::wide("AlwaysTop second").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION,
            -32_000,
            -32_000,
            240,
            80,
            null_mut(),
            null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if second.is_null() {
        return Err(io::Error::last_os_error());
    }
    pin_window(fixture.hwnd, app)?;
    pin_window(second, app)?;
    if !is_topmost(fixture.hwnd) || !is_topmost(second) || session() != Pin::on("2 pencere") {
        unsafe {
            DestroyWindow(second);
        }
        return Err(io::Error::other("AlwaysTop birden fazla pencereyi tutmadı"));
    }
    unpin_window(fixture.hwnd)?;
    if session().title() != Some("AlwaysTop second")
        || is_topmost(fixture.hwnd)
        || !is_topmost(second)
    {
        unsafe {
            DestroyWindow(second);
        }
        return Err(io::Error::other("AlwaysTop diğer pinleri düşürdü"));
    }
    unsafe {
        DestroyWindow(second);
    }
    if !reconcile() || session().active() {
        return Err(io::Error::other(
            "Kapanan pencere AlwaysTop oturumunu kapatmadı",
        ));
    }
    pin_window(fixture.hwnd, app)?;
    unsafe {
        DestroyWindow(fixture.hwnd);
    }
    fixture.hwnd = null_mut();
    if !reconcile() || session().active() {
        return Err(io::Error::other(
            "Kapanan pencere AlwaysTop oturumunu kapatmadı",
        ));
    }
    Ok(())
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
