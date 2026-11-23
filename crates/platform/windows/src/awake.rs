//! AlwaysOn adapter: gives the `powerkit_app` controller its Windows ports
//! (the thread's power request, a window timer, process handles, the clocks)
//! and carries out what the controller decides.
//! Everything here runs on the app window's UI thread; the power request is
//! made and released there and nowhere else.
use super::{clock, power, top};
use powerkit_app::controller::always_on::{Controller, Error, Notice, Refresh, Update};
use powerkit_app::ports::{Clock, KeepAwake, PowerStatus, Processes, WakeTimer, WatchedProcess};
use powerkit_core::always_on::{Mode, PowerSource, Request, Session};
use powerkit_core::primitives::UtcTicks;
use powerkit_core::settings::Preferences;
use std::{cell::RefCell, io};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, WAIT_OBJECT_0},
    System::{SystemInformation::GetTickCount64, Threading::*},
    UI::WindowsAndMessaging::{GetWindowThreadProcessId, KillTimer, SetTimer},
};

pub const TIMER: usize = 1;

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
impl WatchedProcess for Watched {
    fn exited(&self) -> bool {
        Watched::exited(self)
    }
    fn name(&self) -> &str {
        &self.name
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

/// The controller's ports on the app window's thread. `HWND` keeps it `!Send`,
/// so the power request cannot be touched from another thread.
struct Host {
    hwnd: HWND,
}
impl Clock for Host {
    fn uptime_ms(&self) -> u64 {
        uptime_ms()
    }
    fn now_utc(&self) -> UtcTicks {
        clock::now_utc()
    }
}
impl KeepAwake for Host {
    fn keep_awake(&mut self, mode: Mode, display: bool) -> io::Result<()> {
        power::power(mode, display)
    }
}
impl PowerStatus for Host {
    fn power_source(&self) -> PowerSource {
        power::source()
    }
}
impl Processes for Host {
    fn current_pid(&self) -> u32 {
        std::process::id()
    }
    fn watch(&mut self, pid: u32) -> Option<Box<dyn WatchedProcess>> {
        Watched::open(pid).map(|watched| Box::new(watched) as Box<dyn WatchedProcess>)
    }
}
impl WakeTimer for Host {
    fn wake_after(&mut self, delay_ms: Option<u64>) -> io::Result<()> {
        match delay_ms {
            Some(delay) => {
                let delay = u32::try_from(delay).unwrap_or(u32::MAX);
                // SAFETY: hwnd is the live app window; re-setting an id replaces its period.
                if unsafe { SetTimer(self.hwnd, TIMER, delay, None) } == 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            // SAFETY: killing a timer that is not set is harmless.
            None => unsafe {
                KillTimer(self.hwnd, TIMER);
            },
        }
        Ok(())
    }
}

#[derive(Default)]
struct Awake {
    controller: Controller,
}
thread_local! {
    static AWAKE: RefCell<Awake> = RefCell::new(Awake::default());
}
fn with<R>(f: impl FnOnce(&mut Awake) -> R) -> R {
    AWAKE.with(|awake| f(&mut awake.borrow_mut()))
}
fn strings() -> &'static powerkit_core::locale::Strings {
    super::ui_strings()
}
pub fn uptime_ms() -> u64 {
    // SAFETY: reads the tick count, which includes time spent asleep.
    unsafe { GetTickCount64() }
}

/// Runs one controller call, then refreshes the view and shows notices. The
/// view reads this module's state, so that happens after the borrow ends.
fn drive(
    hwnd: HWND,
    call: impl FnOnce(&mut Controller, &mut Host) -> Result<Update, Error>,
) -> io::Result<()> {
    let update = with(|a| call(&mut a.controller, &mut Host { hwnd })).map_err(worded)?;
    carry_out(hwnd, update);
    Ok(())
}
fn carry_out(hwnd: HWND, update: Update) {
    match update.refresh {
        Refresh::None => {}
        Refresh::IfChanged => super::update_view(hwnd, false),
        Refresh::Forced => super::update_view(hwnd, true),
    }
    for notice in update.notices {
        let (body, kind) = match notice {
            Notice::Ending => (strings().warn_body, super::Balloon::AlwaysOnEnding),
            Notice::BatteryStopped => (strings().battery_stopped, super::Balloon::Info),
        };
        super::tray_balloon(hwnd, "PowerKit", body, kind);
    }
}
/// The controller's refusal in the user's language; OS errors keep their text.
fn worded(error: Error) -> io::Error {
    let t = strings();
    match error {
        Error::Power => io::Error::other(t.err_power),
        Error::BatteryBlocked => io::Error::other(t.battery_blocked),
        Error::NoProcess => io::Error::other(t.bind_none),
        Error::ProcessUnavailable => io::Error::other(t.bind_failed),
        Error::Timer(error) => error,
    }
}

pub fn session() -> Session {
    with(|a| a.controller.session())
}
pub fn bound_app() -> Option<String> {
    with(|a| a.controller.bound_app().map(str::to_owned))
}
/// Local end time while an expiry session runs.
pub fn expiry_label() -> Option<String> {
    match session() {
        Session::Expiry { target, .. } => Some(clock::label(target, clock::now_utc())),
        _ => None,
    }
}
pub fn status(t: &powerkit_core::locale::Strings) -> String {
    use powerkit_core::locale::StatusDetail;
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

/// Hands the preferences to the controller.
pub fn apply_prefs(hwnd: HWND, prefs: &Preferences) {
    let update = with(|a| a.controller.configure(&mut Host { hwnd }, prefs));
    carry_out(hwnd, update);
}
/// Releases the power request, timer and process handle.
pub fn shutdown(hwnd: HWND) -> io::Result<()> {
    with(|a| a.controller.shutdown(&mut Host { hwnd })).map_err(worded)
}

/// Starts a session the user asked for.
pub fn start(hwnd: HWND, session: Session) -> io::Result<()> {
    drive(hwnd, |controller, host| controller.start(host, session))
}
/// Re-applies the current session with a different display preference.
pub fn set_display(hwnd: HWND, display: bool) -> io::Result<()> {
    drive(hwnd, |controller, host| {
        controller.set_display(host, display)
    })
}
/// Ends a passed deadline and re-projects expiry targets after resume or a clock change.
pub fn reconcile(hwnd: HWND) -> io::Result<()> {
    drive(hwnd, |controller, host| controller.reconcile(host))
}
pub fn toggle(hwnd: HWND) -> io::Result<()> {
    drive(hwnd, |controller, host| controller.toggle(host))
}
pub fn extend(hwnd: HWND) -> io::Result<()> {
    drive(hwnd, |controller, host| controller.extend(host))
}
/// Binds to the process that owns the last eligible foreground window.
pub fn bind_last(hwnd: HWND) -> io::Result<()> {
    let target = top::remember_foreground(hwnd);
    let mut pid = 0;
    if !target.is_null() {
        // SAFETY: target is a live top-level window; pid is a live local.
        unsafe { GetWindowThreadProcessId(target, &mut pid) };
    }
    if pid == 0 {
        return Err(io::Error::other(strings().bind_none));
    }
    bind(hwnd, pid)
}
pub fn bind(hwnd: HWND, pid: u32) -> io::Result<()> {
    drive(hwnd, |controller, host| controller.bind(host, pid))
}
pub fn request(hwnd: HWND, request: Request) -> io::Result<()> {
    drive(hwnd, |controller, host| controller.request(host, request))
}
/// The session timer fired.
pub fn on_timer(hwnd: HWND) -> io::Result<()> {
    drive(hwnd, |controller, host| controller.on_wake(host))
}
/// Stops an active session when the battery policy no longer allows it.
pub fn enforce_policy(hwnd: HWND) {
    let update = with(|a| a.controller.enforce_policy(&mut Host { hwnd }));
    carry_out(hwnd, update);
}
