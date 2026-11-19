//! AlwaysOn adapter: owns the running session, its single deadline-aligned
//! timer, the toggle hotkey, the end-of-session notice and a bound process.
//! Everything here runs on the app window's UI thread.
use super::{clock, power, top};
use crate::always_on::{BatteryPolicy, EXTEND_MINUTES, Request, Session};
use crate::hotkey::Hotkey;
use crate::settings::Preferences;
use std::{cell::RefCell, io, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, WAIT_OBJECT_0},
    System::{SystemInformation::GetTickCount64, Threading::*},
    UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey},
        WindowsAndMessaging::{GetWindowThreadProcessId, KillTimer, SetTimer},
    },
};

pub(super) const TIMER: usize = 1;
pub(super) const HOTKEY: i32 = 5;

/// A process handle kept open so its exit can be observed without a pid race.
struct Watched {
    handle: HANDLE,
    name: String,
}
impl Watched {
    fn open(pid: u32) -> Option<Self> {
        // SAFETY: plain query; a null result means no access or no such process.
        let handle = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        };
        if handle.is_null() {
            return None;
        }
        let watched = Self {
            handle,
            name: image_name(handle).unwrap_or_else(|| format!("PID {pid}")),
        };
        (!watched.exited()).then_some(watched)
    }
    fn exited(&self) -> bool {
        // SAFETY: handle is owned by self and opened with SYNCHRONIZE.
        unsafe { WaitForSingleObject(self.handle, 0) == WAIT_OBJECT_0 }
    }
}
impl Drop for Watched {
    fn drop(&mut self) {
        // SAFETY: self owns the handle returned by OpenProcess.
        unsafe { CloseHandle(self.handle) };
    }
}
fn image_name(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: buffer and length describe the same live local array.
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    if ok == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    std::path::Path::new(&path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
}

struct Awake {
    app: HWND,
    session: Session,
    /// The mode command the hotkey restarts; kept in memory only.
    last_start: super::AppCommand,
    key: Hotkey,
    hotkey: Option<String>,
    display: bool,
    interval_minutes: u32,
    warn: bool,
    policy: BatteryPolicy,
    warned: bool,
    process: Option<Watched>,
}
impl Default for Awake {
    fn default() -> Self {
        let prefs = Preferences::default();
        Self {
            app: null_mut(),
            session: Session::Off,
            last_start: super::AppCommand::AlwaysOnForever,
            key: prefs.always_on_hotkey,
            hotkey: None,
            display: prefs.display,
            interval_minutes: prefs.interval_minutes,
            warn: prefs.always_on_warn,
            policy: prefs.always_on_battery,
            warned: false,
            process: None,
        }
    }
}
thread_local! {
    static AWAKE: RefCell<Awake> = RefCell::new(Awake::default());
}
fn with<R>(f: impl FnOnce(&mut Awake) -> R) -> R {
    AWAKE.with(|awake| f(&mut awake.borrow_mut()))
}
fn strings() -> &'static crate::locale::Strings {
    super::ui_strings()
}
pub(super) fn uptime_ms() -> u64 {
    // SAFETY: reads the tick count, which includes time spent asleep.
    unsafe { GetTickCount64() }
}

pub(super) fn session() -> Session {
    with(|a| a.session)
}
pub(super) fn hotkey_label() -> Option<String> {
    with(|a| a.hotkey.clone())
}
pub(super) fn bound_app() -> Option<String> {
    with(|a| a.process.as_ref().map(|p| p.name.clone()))
}
/// Local end time while an expiry session runs.
pub(super) fn expiry_label() -> Option<String> {
    match session() {
        Session::Expiry { target, .. } => Some(clock::label(target, clock::now_utc())),
        _ => None,
    }
}
pub(super) fn status(t: &crate::locale::Strings) -> String {
    use crate::locale::StatusDetail;
    let session = session();
    let expiry = expiry_label();
    let app = bound_app();
    let detail = match session {
        Session::Interval { .. } => StatusDetail::Remaining(session.remaining_minutes(uptime_ms())),
        Session::Expiry { .. } => expiry
            .as_deref()
            .map_or(StatusDetail::None, StatusDetail::Until),
        Session::Process { .. } => app.as_deref().map_or(StatusDetail::None, StatusDetail::App),
        Session::Off | Session::Indefinite => StatusDetail::None,
    };
    t.always_on_status(session.active(), detail)
}

/// Caches preferences and re-registers the hotkey when it changed.
pub(super) fn apply_prefs(hwnd: HWND, prefs: &Preferences) {
    let rekey = with(|a| {
        let rekey = a.key != prefs.always_on_hotkey;
        a.key = prefs.always_on_hotkey;
        a.display = prefs.display;
        a.interval_minutes = prefs.interval_minutes;
        a.warn = prefs.always_on_warn;
        a.policy = prefs.always_on_battery;
        rekey || a.hotkey.is_none()
    });
    if rekey && !with(|a| a.app).is_null() {
        install_hotkey();
    }
    enforce_policy(hwnd);
}
pub(super) fn listen(app: HWND) {
    with(|a| a.app = app);
    install_hotkey();
}
pub(super) fn pause_hotkeys() {
    uninstall_hotkey();
}
/// Releases the power request, timer, process handle and hotkey.
pub(super) fn shutdown(hwnd: HWND) -> io::Result<()> {
    // SAFETY: killing a timer that is not set is harmless.
    unsafe { KillTimer(hwnd, TIMER) };
    uninstall_hotkey();
    with(|a| {
        a.app = null_mut();
        a.session = Session::Off;
        a.process = None;
    });
    power::power(Session::Off.mode(), false).map_err(|_| io::Error::other(strings().err_power))
}

/// Starts a session the user asked for: checks power conditions and remembers
/// mode commands for the hotkey.
pub(super) fn start(hwnd: HWND, session: Session) -> io::Result<()> {
    if session.active() && !allowed() {
        return Err(io::Error::other(strings().battery_blocked));
    }
    apply(hwnd, session)?;
    let command = super::AppCommand::for_session(session);
    with(|a| {
        a.warned = session.warning_due(uptime_ms());
        if session.active() && command.always_on_session(0, a.interval_minutes).is_some() {
            a.last_start = command;
        }
    });
    Ok(())
}
/// Re-applies the current session with a different display preference.
pub(super) fn set_display(hwnd: HWND, display: bool) -> io::Result<()> {
    let previous = with(|a| std::mem::replace(&mut a.display, display));
    apply(hwnd, session()).inspect_err(|_| with(|a| a.display = previous))
}
/// Ends a passed deadline and re-projects expiry targets after resume or a clock change.
pub(super) fn reconcile(hwnd: HWND) -> io::Result<()> {
    let next = session().retimed(uptime_ms(), clock::now_utc());
    apply(hwnd, next)
}
pub(super) fn toggle(hwnd: HWND) -> io::Result<()> {
    if session().active() {
        return start(hwnd, Session::Off);
    }
    let (command, minutes) = with(|a| (a.last_start, a.interval_minutes));
    match command.always_on_session(uptime_ms(), minutes) {
        Some(next) => start(hwnd, next),
        None => start(hwnd, Session::Indefinite),
    }
}
pub(super) fn extend(hwnd: HWND) -> io::Result<()> {
    let next = session().extended(EXTEND_MINUTES);
    if next == session() {
        return Ok(());
    }
    apply(hwnd, next)?;
    with(|a| a.warned = next.warning_due(uptime_ms()));
    Ok(())
}
/// Binds to the process that owns the last eligible foreground window.
pub(super) fn bind_last(hwnd: HWND) -> io::Result<()> {
    let target = top::remember_foreground(hwnd);
    let mut pid = 0;
    if !target.is_null() {
        // SAFETY: target is a live top-level window; pid is a live local.
        unsafe { GetWindowThreadProcessId(target, &mut pid) };
    }
    if pid == 0 || pid == std::process::id() {
        return Err(io::Error::other(strings().bind_none));
    }
    bind(hwnd, pid)
}
pub(super) fn bind(hwnd: HWND, pid: u32) -> io::Result<()> {
    if pid == std::process::id() {
        return Err(io::Error::other(strings().bind_none));
    }
    let watched = Watched::open(pid).ok_or_else(|| io::Error::other(strings().bind_failed))?;
    let previous = with(|a| a.process.replace(watched));
    match start(hwnd, Session::Process { pid }) {
        Ok(()) => Ok(()),
        Err(error) => {
            with(|a| a.process = previous);
            Err(error)
        }
    }
}
pub(super) fn request(hwnd: HWND, request: Request) -> io::Result<()> {
    match request {
        Request::Off => start(hwnd, Session::Off),
        Request::Indefinite => start(hwnd, Session::Indefinite),
        Request::Minutes(minutes) => start(hwnd, Session::interval(uptime_ms(), minutes, false)),
        Request::Process(pid) => bind(hwnd, pid),
    }
}
/// Ends finished sessions, enforces power conditions and fires the end notice.
pub(super) fn on_timer(hwnd: HWND) -> io::Result<()> {
    let session = session();
    let now = uptime_ms();
    let process_gone = matches!(session, Session::Process { .. })
        && with(|a| a.process.as_ref().is_none_or(Watched::exited));
    if process_gone {
        apply(hwnd, Session::Off)?;
    } else if session.mode().expired(now) {
        reconcile(hwnd)?;
    } else {
        schedule(hwnd)?;
        super::update_view(hwnd, false);
    }
    enforce_policy(hwnd);
    let notify = with(|a| {
        let due = a.warn && !a.warned && a.session.warning_due(now);
        if due {
            a.warned = true;
        }
        due
    });
    if notify {
        super::tray_balloon(
            hwnd,
            "PowerKit",
            strings().warn_body,
            super::Balloon::AlwaysOnEnding,
        );
    }
    Ok(())
}
/// Stops an active session when the battery policy no longer allows it.
pub(super) fn enforce_policy(hwnd: HWND) {
    if !session().active() || allowed() {
        return;
    }
    if apply(hwnd, Session::Off).is_ok() {
        super::tray_balloon(
            hwnd,
            "PowerKit",
            strings().battery_stopped,
            super::Balloon::Info,
        );
    }
}
fn allowed() -> bool {
    with(|a| a.policy).allows(power::source())
}

fn apply(hwnd: HWND, session: Session) -> io::Result<()> {
    let (previous, display) = with(|a| (a.session, a.display));
    power::power(session.mode(), display).map_err(|_| io::Error::other(strings().err_power))?;
    with(|a| {
        a.session = session;
        if !matches!(session, Session::Process { .. }) {
            a.process = None;
        }
    });
    if let Err(error) = schedule(hwnd) {
        with(|a| a.session = previous);
        let _ = power::power(previous.mode(), display);
        return Err(error);
    }
    super::update_view(hwnd, true);
    Ok(())
}
fn schedule(hwnd: HWND) -> io::Result<()> {
    match session().next_tick_ms(uptime_ms()) {
        Some(delay) => {
            let delay = u32::try_from(delay).unwrap_or(u32::MAX);
            // SAFETY: hwnd is the live app window; re-setting an id replaces its period.
            if unsafe { SetTimer(hwnd, TIMER, delay, None) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        // SAFETY: killing a timer that is not set is harmless.
        None => unsafe {
            KillTimer(hwnd, TIMER);
        },
    }
    Ok(())
}

fn install_hotkey() {
    uninstall_hotkey();
    let (app, key) = with(|a| (a.app, a.key));
    if app.is_null() {
        return;
    }
    let active =
        key.register_with_fallback(key == Hotkey::always_on_default(), |key| register(app, key));
    with(|a| a.hotkey = active.map(Hotkey::label));
}
fn register(app: HWND, key: Hotkey) -> bool {
    // SAFETY: app is the live window that receives WM_HOTKEY on this thread.
    unsafe { RegisterHotKey(app, HOTKEY, key.win32_mods(), u32::from(key.vk)) != 0 }
}
fn uninstall_hotkey() {
    let app = with(|a| {
        a.hotkey = None;
        a.app
    });
    if !app.is_null() {
        // SAFETY: unregistering an id this window may not own is harmless.
        unsafe { UnregisterHotKey(app, HOTKEY) };
    }
}
