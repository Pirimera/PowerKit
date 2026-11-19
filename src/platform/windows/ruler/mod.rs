//! ScreenRuler on the UI thread. Measurement math lives in `crate::screen_ruler`.
//!
//! This module owns the session state, its lifecycle (open, rebuild, end, failure
//! reporting), the activation hotkey, tool selection and the clipboard. The
//! submodules split the rest by Win32 surface:
//! - `capture`: GDI surfaces and screen capture,
//! - `paint`: composing and pushing the layered overlays,
//! - `input`: overlay mouse/keyboard handling,
//! - `toolbar`: the toolbar window, its placement and window procedure,
//! - `bar`: toolbar drawing, hit testing and tooltips.
//!
//! All state lives in the thread-local `RULER` and every window here belongs to
//! the app's UI thread, so no handle or surface crosses threads.
mod bar;
mod capture;
mod input;
mod paint;
mod toolbar;

use super::fault::Fault;
use super::wide;
use crate::hotkey::Hotkey;
use crate::screen_ruler::{
    BoundsRight, FEET_HALF, Marks, Mode, Span, Unit, clipboard_text_by_display, detect_edges,
    escape_copies, overlay_label, wheel_tolerance,
};
use crate::settings::Preferences;
use bar::Hit;
use capture::{Dib, Shot, capture, regrab};
use input::{on_key, on_wheel, overlay_proc, point_from};
use paint::{cursor_monitor, cursor_on, redraw, refresh, toolbar_contains_cursor};
use std::{
    cell::{Cell, RefCell},
    io,
    mem::size_of,
    ptr::{null, null_mut},
};
use toolbar::{create_toolbar, toolbar_proc};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::WM_MOUSELEAVE,
        HiDpi::*,
        Input::KeyboardAndMouse::{
            GetKeyState, RegisterHotKey, ReleaseCapture, SetCapture, UnregisterHotKey, VK_CONTROL,
            VK_ESCAPE, VK_SHIFT,
        },
        WindowsAndMessaging::*,
    },
};

pub(super) const TIMER: usize = 4;
pub(super) const HOTKEY: i32 = 4;
/// Posted to the app window after a session ended, including on its own (toolbar
/// close, a measuring click, a failure). The app refreshes its status and shows
/// [`take_failure`] there, outside the toolbar's window procedure and after the
/// topmost overlays are gone, so the message box is never hidden behind them.
pub(super) const ENDED: u32 = WM_APP + 12;
/// Posted by the toolbar on `WM_DISPLAYCHANGE`; the app answers with
/// [`on_display_change`] once the new layout has settled.
pub(super) const DISPLAY_CHANGED: u32 = WM_APP + 13;
const OVERLAY_CLASS: &str = "PowerKit.ScreenRuler.Overlay";
const TOOLBAR_CLASS: &str = "PowerKit.ScreenRuler.Toolbar";
/// Overlay pixels of this color are transparent (`ULW_COLORKEY`).
const COLOR_KEY: u32 = 0x00FF_00FF;
/// What a captured or drawn pixel of exactly `COLOR_KEY` becomes, so it stays visible.
const KEY_SAFE: u32 = 0x00FF_00FE;
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;

thread_local! {
    static RULER: RefCell<Ruler> = RefCell::new(Ruler::default());
}

#[derive(Clone)]
struct Config {
    key: Hotkey,
    mode: Mode,
    tolerance: u8,
    per_channel: bool,
    feet: bool,
    continuous: bool,
    unit: Unit,
    color: u32,
    language: crate::locale::UiLanguage,
}
impl Default for Config {
    fn default() -> Self {
        Self::from_prefs(&Preferences::default())
    }
}
impl Config {
    fn from_prefs(prefs: &Preferences) -> Self {
        Self {
            key: prefs.ruler_hotkey,
            mode: prefs.ruler_mode,
            tolerance: prefs.ruler_tolerance,
            per_channel: prefs.ruler_per_channel,
            feet: prefs.ruler_feet,
            continuous: prefs.ruler_continuous,
            unit: prefs.ruler_unit,
            color: prefs.ruler_color,
            language: crate::locale::UiLanguage::resolve(prefs.language),
        }
    }
}
struct Ruler {
    app: HWND,
    config: Config,
    hotkey: Option<String>,
    session: Option<Session>,
    /// Screen top-left the user dragged the toolbar to. Kept for later sessions
    /// while PowerKit runs; never persisted, so a restart uses the default spot.
    toolbar_at: Option<POINT>,
    /// Failure that ended a session from inside a window procedure, waiting for
    /// the app to report it on `ENDED`.
    failure: Option<io::Error>,
}
impl Default for Ruler {
    fn default() -> Self {
        Self {
            app: null_mut(),
            config: Config::default(),
            hotkey: None,
            session: None,
            toolbar_at: None,
            failure: None,
        }
    }
}
/// One open ruler. `config` follows the preferences while it is open; `mode` and
/// `tolerance` start from them but change within the session (toolbar, Ctrl+1–4,
/// wheel).
struct Session {
    config: Config,
    mode: Mode,
    tolerance: u8,
    monitors: Vec<Monitor>,
    toolbar: HWND,
    /// Monitor that last showed the live crosshair; it needs one more paint to
    /// clear it after the cursor leaves.
    live: Option<usize>,
}
struct Monitor {
    hwnd: HWND,
    origin_x: i32,
    origin_y: i32,
    width: i32,
    height: i32,
    dpi: f32,
    px_to_mm: f32,
    pixels: Vec<u32>,
    /// Drag in progress and kept measurements; the decisions live in `Marks`.
    marks: Marks,
    /// Paint surface handed to `UpdateLayeredWindow`.
    dib: Option<Dib>,
    /// Capture surface reused by continuous capture instead of a new DIB per frame.
    capture: Option<Dib>,
}

/// The open session's toolbar, or null.
fn session_toolbar() -> HWND {
    RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .map(|session| session.toolbar)
            .unwrap_or(null_mut())
    })
}
/// Runs `change` on monitor `index` of the open session, if both exist. `change`
/// must not touch `RULER`.
fn with_monitor_mut(index: usize, change: impl FnOnce(&mut Monitor)) {
    RULER.with(|ruler| {
        if let Some(monitor) = ruler
            .borrow_mut()
            .session
            .as_mut()
            .and_then(|session| session.monitors.get_mut(index))
        {
            change(monitor);
        }
    });
}

pub(super) fn apply_prefs(prefs: &Preferences) {
    let config = Config::from_prefs(prefs);
    let language = config.language;
    let (app, toolbar) = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let toolbar = match ruler.session.as_mut() {
            // The open session keeps its tool; a saved tolerance replaces a wheel change.
            Some(session) => {
                session.tolerance = config.tolerance;
                session.config = config.clone();
                session.toolbar
            }
            None => null_mut(),
        };
        ruler.config = config;
        (ruler.app, toolbar)
    });
    if !toolbar.is_null() {
        bar::attach_tips(toolbar, tip_labels(language));
    }
    if !app.is_null() {
        install_hotkey(app);
        sync_timer();
        redraw();
    }
}
pub(super) fn listen(app: HWND) {
    RULER.with(|ruler| ruler.borrow_mut().app = app);
    install_hotkey(app);
}
pub(super) fn pause_hotkeys() {
    let app = RULER.with(|ruler| ruler.borrow().app);
    uninstall_hotkey(app);
}
pub(super) fn shutdown() {
    end_session();
    let app = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let app = ruler.app;
        ruler.app = null_mut();
        app
    });
    uninstall_hotkey(app);
}
pub(super) fn active() -> bool {
    RULER.with(|ruler| ruler.borrow().session.is_some())
}
pub(super) fn hotkey_label() -> Option<String> {
    RULER.with(|ruler| ruler.borrow().hotkey.clone())
}
pub(super) fn toggle() -> io::Result<()> {
    if active() {
        end_session();
        return Ok(());
    }
    open()
}
/// The failure that ended the last session on its own, if any; taken on `ENDED`.
pub(super) fn take_failure() -> Option<io::Error> {
    RULER.with(|ruler| ruler.borrow_mut().failure.take())
}
/// Monitors were added, removed or resized. Overlays, captures and DPI belong to
/// the old layout, so the session is rebuilt with the same tool; marks are dropped.
pub(super) fn on_display_change() {
    let Some(mode) = RULER.with(|ruler| ruler.borrow().session.as_ref().map(|s| s.mode)) else {
        return;
    };
    end_session();
    match open() {
        Ok(()) => set_mode(mode),
        Err(error) => fail(error),
    }
}
/// Continuous capture tick. Only the monitor under the cursor feeds the live edge
/// walk, so only that monitor is recaptured and repainted; kept marks elsewhere
/// do not depend on fresh pixels.
pub(super) fn on_timer() {
    let grabbed = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let Some(session) = ruler.session.as_mut() else {
            return false;
        };
        if !wants_capture(session) {
            return false;
        }
        let Some(index) = cursor_monitor(session) else {
            return false;
        };
        session.monitors.get_mut(index).is_some_and(regrab)
    });
    if grabbed {
        refresh(None);
    }
}
/// Ends the session because of `error` and lets the app report it on `ENDED`.
fn fail(error: io::Error) {
    RULER.with(|ruler| ruler.borrow_mut().failure = Some(error));
    end_session();
    notify_app(ENDED);
}
fn notify_app(message: u32) {
    let app = RULER.with(|ruler| ruler.borrow().app);
    if !app.is_null() {
        unsafe {
            PostMessageW(app, message, 0, 0);
        }
    }
}
/// Refreshes every monitor's frozen pixels in place.
fn recapture() {
    RULER.with(|ruler| {
        if let Some(session) = ruler.borrow_mut().session.as_mut() {
            for monitor in &mut session.monitors {
                regrab(monitor);
            }
        }
    });
}
fn wants_capture(session: &Session) -> bool {
    session.config.continuous && session.mode.edges()
}
fn open() -> io::Result<()> {
    let (config, toolbar_at) = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        (ruler.config.clone(), ruler.toolbar_at)
    });
    let shots = capture()?;
    if shots.is_empty() {
        return Err(Fault::NoDisplay.into());
    }
    register_classes()?;
    let toolbar = create_toolbar(&shots, toolbar_at)?;
    let monitors = shots
        .into_iter()
        .map(|shot| Monitor {
            hwnd: null_mut(),
            origin_x: shot.origin_x,
            origin_y: shot.origin_y,
            width: shot.width,
            height: shot.height,
            dpi: shot.dpi,
            px_to_mm: shot.px_to_mm,
            pixels: shot.pixels,
            marks: Marks::default(),
            dib: None,
            capture: Some(shot.surface),
        })
        .collect();
    let language = config.language;
    let mode = config.mode;
    RULER.with(|ruler| {
        ruler.borrow_mut().session = Some(Session {
            mode,
            tolerance: config.tolerance,
            config,
            monitors,
            toolbar,
            live: None,
        });
    });
    if mode != Mode::None
        && let Err(error) = ensure_overlays()
    {
        // Do not leave a half-built session with a hidden toolbar behind.
        end_session();
        return Err(error);
    }
    sync_timer();
    unsafe {
        ShowWindow(toolbar, SW_SHOW);
        SetForegroundWindow(toolbar);
    }
    bar::attach_tips(toolbar, tip_labels(language));
    redraw();
    Ok(())
}
fn end_session() {
    let (app, session) = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        (ruler.app, ruler.session.take())
    });
    let Some(mut session) = session else {
        return;
    };
    unsafe {
        ClipCursor(null());
        // The timer only ever runs on the app window (`sync_timer`).
        if !app.is_null() {
            KillTimer(app, TIMER);
        }
        if !session.toolbar.is_null() {
            DestroyWindow(session.toolbar);
        }
        for monitor in &session.monitors {
            if !monitor.hwnd.is_null() {
                DestroyWindow(monitor.hwnd);
            }
        }
    }
    for monitor in &mut session.monitors {
        if let Some(dib) = monitor.dib.as_mut() {
            dib.release();
        }
    }
    notify_app(ENDED);
}
fn ensure_overlays() -> io::Result<()> {
    let instance = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()) };
    let (toolbar, pending) = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let Some(session) = ruler.session.as_ref() else {
            return (null_mut(), Vec::new());
        };
        let pending = session
            .monitors
            .iter()
            .enumerate()
            .filter(|(_, monitor)| monitor.hwnd.is_null())
            .map(|(index, monitor)| {
                (
                    index,
                    monitor.origin_x,
                    monitor.origin_y,
                    monitor.width,
                    monitor.height,
                )
            })
            .collect::<Vec<_>>();
        (session.toolbar, pending)
    });
    let created = !pending.is_empty();
    for (index, x, y, width, height) in pending {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                wide(OVERLAY_CLASS).as_ptr(),
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
            return Err(io::Error::last_os_error());
        }
        unsafe {
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
            SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        }
        with_monitor_mut(index, |monitor| monitor.hwnd = hwnd);
    }
    if created && !toolbar.is_null() {
        // A new overlay enters the top of the topmost band and would hide the bar
        // (and its grip) when a tool is chosen after opening with no mode.
        unsafe {
            SetWindowPos(
                toolbar,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    Ok(())
}
fn sync_timer() {
    let (app, on) = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let on = ruler.session.as_ref().is_some_and(wants_capture);
        (ruler.app, on)
    });
    if app.is_null() {
        return;
    }
    unsafe {
        if on {
            SetTimer(app, TIMER, 33, None);
        } else {
            KillTimer(app, TIMER);
        }
    }
}
fn install_hotkey(app: HWND) {
    uninstall_hotkey(app);
    let key = RULER.with(|ruler| ruler.borrow().config.key);
    let active =
        key.register_with_fallback(key == Hotkey::ruler_default(), |key| register(app, key));
    RULER.with(|ruler| ruler.borrow_mut().hotkey = active.map(Hotkey::label));
}
fn register(app: HWND, key: Hotkey) -> bool {
    unsafe { RegisterHotKey(app, HOTKEY, key.win32_mods(), u32::from(key.vk)) != 0 }
}
fn uninstall_hotkey(app: HWND) {
    if !app.is_null() {
        unsafe {
            UnregisterHotKey(app, HOTKEY);
        }
    }
    RULER.with(|ruler| ruler.borrow_mut().hotkey = None);
}
fn register_classes() -> io::Result<()> {
    unsafe {
        let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null());
        let cursor = LoadCursorW(null_mut(), IDC_ARROW);
        for (name, proc) in [
            (
                OVERLAY_CLASS,
                overlay_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            ),
            (TOOLBAR_CLASS, toolbar_proc),
        ] {
            let class_name = wide(name);
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(proc),
                hInstance: instance,
                hCursor: cursor,
                lpszClassName: class_name.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}
fn tip_labels(language: crate::locale::UiLanguage) -> [&'static str; 6] {
    let labels = language.strings();
    [
        labels.ruler_tip_move,
        labels.ruler_tip_bounds,
        labels.ruler_tip_spacing,
        labels.ruler_tip_horizontal,
        labels.ruler_tip_vertical,
        labels.ruler_tip_close,
    ]
}
/// Toolbar button or Ctrl+number: choosing the active tool again clears it.
fn select_tool(mode: Mode) {
    let current = RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .map(|session| session.mode)
            .unwrap_or(Mode::None)
    });
    set_mode(current.toggled(mode));
}
fn set_mode(mode: Mode) {
    let changed = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let Some(session) = ruler.session.as_mut() else {
            return false;
        };
        if session.mode == mode {
            return false;
        }
        session.mode = mode;
        for monitor in &mut session.monitors {
            monitor.marks.clear();
        }
        true
    });
    if !changed {
        return;
    }
    // Continuous capture only runs for the edge tools.
    sync_timer();
    if mode == Mode::None {
        redraw();
        return;
    }
    // Freeze the screen as it is when the tool is chosen, not when the toolbar
    // opened: windows activated in between (for example the PowerKit settings
    // window the ruler was started from) must not reappear in the overlay. The
    // toolbar and overlays are excluded from capture, so they never end up in it.
    recapture();
    match ensure_overlays() {
        Ok(()) => redraw(),
        // A tool without its overlay cannot measure; close and say why.
        Err(error) => fail(error),
    }
}
fn escape() {
    let copy = RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| escape_copies(session.mode))
    });
    if copy {
        copy_measurements(None, None);
    }
    end_session();
}
/// Copies every kept, dragged and `current` span. Each monitor's spans convert
/// with that monitor's own physical size and DPI, so Escape (no clicked monitor)
/// and multi-monitor marks report correct inch, centimetre, millimetre and DIP values.
fn copy_measurements(monitor_index: Option<usize>, current: Option<Span>) {
    let text = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let Some(session) = ruler.session.as_ref() else {
            return String::new();
        };
        let groups = session
            .monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| {
                let current = current.filter(|_| monitor_index == Some(index));
                (monitor.marks.spans(current), monitor.px_to_mm, monitor.dpi)
            })
            .collect::<Vec<_>>();
        clipboard_text_by_display(&groups, session.mode, session.config.unit)
    });
    let app = RULER.with(|ruler| ruler.borrow().app);
    // A clipboard another app keeps busy does not interrupt measuring; the next
    // click or Escape copies again.
    let _ = super::clipboard::set_text(app, &text);
}
