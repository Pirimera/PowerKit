//! AlwaysOn controller: owns the running session and decides when the system
//! is kept awake, when a session ends and what the user is told.
//!
//! The controller never persists a session and has no way to: a launch always
//! starts with [`Session::Off`]. It holds the keep-awake request only through
//! [`KeepAwake`], and every path that ends a session, including a failed
//! start, goes through [`Controller::apply`] or [`Controller::shutdown`], so
//! the request and the session never disagree.
use crate::ports::{Clock, KeepAwake, PowerStatus, Processes, WakeTimer, WatchedProcess};
use powerkit_core::always_on::{BatteryPolicy, Choice, EXTEND_MINUTES, Request, Session};
use powerkit_core::settings::Preferences;
use std::{fmt, io};

/// The services AlwaysOn needs. Any type with these ports is a host.
pub trait Host: Clock + KeepAwake + PowerStatus + Processes + WakeTimer {}
impl<T: Clock + KeepAwake + PowerStatus + Processes + WakeTimer> Host for T {}

/// Why a request was refused. The platform words these for the user.
#[derive(Debug)]
pub enum Error {
    /// The keep-awake request could not be changed.
    Power,
    /// The battery policy does not allow a session right now.
    BatteryBlocked,
    /// There is no other process to bind to.
    NoProcess,
    /// The process could not be opened or has already exited.
    ProcessUnavailable,
    /// The wake-up for the session could not be scheduled.
    Timer(io::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Power => f.write_str("the keep-awake request could not be changed"),
            Self::BatteryBlocked => f.write_str("the battery policy blocks keep-awake"),
            Self::NoProcess => f.write_str("there is no other process to bind to"),
            Self::ProcessUnavailable => f.write_str("the process could not be watched"),
            Self::Timer(error) => write!(f, "the session wake-up could not be set: {error}"),
        }
    }
}
impl std::error::Error for Error {}

/// Something to tell the user outside the settings window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// The session ends within the notice lead.
    Ending,
    /// The battery policy stopped a running session.
    BatteryStopped,
}

/// How much of the UI a change touches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Refresh {
    #[default]
    None,
    /// Only what depends on the time left, such as a countdown.
    IfChanged,
    /// The session itself changed.
    Forced,
}

/// What the caller must carry out after a controller call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Update {
    pub refresh: Refresh,
    pub notices: Vec<Notice>,
}
impl Update {
    fn forced() -> Self {
        Self {
            refresh: Refresh::Forced,
            notices: Vec::new(),
        }
    }
    fn absorb(&mut self, other: Self) {
        self.refresh = self.refresh.max(other.refresh);
        self.notices.extend(other.notices);
    }
}

pub struct Controller {
    session: Session,
    /// The choice the toggle shortcut starts again; kept in memory only.
    last_start: Choice,
    display: bool,
    interval_minutes: u32,
    warn: bool,
    policy: BatteryPolicy,
    /// The end notice already fired for the current deadline.
    warned: bool,
    process: Option<Box<dyn WatchedProcess>>,
}
impl Default for Controller {
    fn default() -> Self {
        let prefs = Preferences::default();
        Self {
            session: Session::Off,
            last_start: Choice::Indefinite,
            display: prefs.display,
            interval_minutes: prefs.interval_minutes,
            warn: prefs.always_on_warn,
            policy: prefs.always_on_battery,
            warned: false,
            process: None,
        }
    }
}
impl Controller {
    pub fn session(&self) -> Session {
        self.session
    }
    /// Name of the process a process-bound session follows.
    pub fn bound_app(&self) -> Option<&str> {
        self.process.as_deref().map(WatchedProcess::name)
    }

    /// Takes the user's preferences and stops a session they no longer allow.
    pub fn configure(&mut self, host: &mut impl Host, prefs: &Preferences) -> Update {
        self.display = prefs.display;
        self.interval_minutes = prefs.interval_minutes;
        self.warn = prefs.always_on_warn;
        self.policy = prefs.always_on_battery;
        self.enforce_policy(host)
    }

    /// Starts a session the user asked for: checks the battery policy and
    /// remembers a restartable choice for the toggle shortcut.
    pub fn start(&mut self, host: &mut impl Host, session: Session) -> Result<Update, Error> {
        if session.active() && !self.allowed(host) {
            return Err(Error::BatteryBlocked);
        }
        let update = self.apply(host, session)?;
        self.warned = session.warning_due(host.uptime_ms());
        let choice = Choice::of(session);
        if session.active() && choice.start(0, self.interval_minutes).is_some() {
            self.last_start = choice;
        }
        Ok(update)
    }

    /// Re-applies the current session with a different display preference.
    pub fn set_display(&mut self, host: &mut impl Host, display: bool) -> Result<Update, Error> {
        let previous = std::mem::replace(&mut self.display, display);
        self.apply(host, self.session)
            .inspect_err(|_| self.display = previous)
    }

    /// Ends a passed deadline and re-projects an expiry target after a resume
    /// or a clock change. Never extends a session.
    pub fn reconcile(&mut self, host: &mut impl Host) -> Result<Update, Error> {
        let next = self.session.retimed(host.uptime_ms(), host.now_utc());
        self.apply(host, next)
    }

    /// Stops a running session, or starts the last restartable choice again.
    pub fn toggle(&mut self, host: &mut impl Host) -> Result<Update, Error> {
        if self.session.active() {
            return self.start(host, Session::Off);
        }
        let next = self
            .last_start
            .start(host.uptime_ms(), self.interval_minutes)
            .unwrap_or(Session::Indefinite);
        self.start(host, next)
    }

    /// Pushes the deadline later; a session without one is left alone.
    pub fn extend(&mut self, host: &mut impl Host) -> Result<Update, Error> {
        let next = self.session.extended(EXTEND_MINUTES);
        if next == self.session {
            return Ok(Update::default());
        }
        let update = self.apply(host, next)?;
        self.warned = next.warning_due(host.uptime_ms());
        Ok(update)
    }

    /// Keeps awake while process `pid` runs. PowerKit never binds to itself.
    pub fn bind(&mut self, host: &mut impl Host, pid: u32) -> Result<Update, Error> {
        if pid == host.current_pid() {
            return Err(Error::NoProcess);
        }
        let watched = host.watch(pid).ok_or(Error::ProcessUnavailable)?;
        let previous = self.process.replace(watched);
        match self.start(host, Session::Process { pid }) {
            Ok(update) => Ok(update),
            Err(error) => {
                self.process = previous;
                Err(error)
            }
        }
    }

    /// Applies a command-line request from this or a later instance.
    pub fn request(&mut self, host: &mut impl Host, request: Request) -> Result<Update, Error> {
        match request {
            Request::Off => self.start(host, Session::Off),
            Request::Indefinite => self.start(host, Session::Indefinite),
            Request::Minutes(minutes) => {
                let session = Session::interval(host.uptime_ms(), minutes, false);
                self.start(host, session)
            }
            Request::Process(pid) => self.bind(host, pid),
        }
    }

    /// The scheduled wake-up: ends finished sessions, enforces the battery
    /// policy and fires the end notice once per deadline.
    pub fn on_wake(&mut self, host: &mut impl Host) -> Result<Update, Error> {
        let session = self.session;
        let now = host.uptime_ms();
        let process_gone = matches!(session, Session::Process { .. })
            && self.process.as_ref().is_none_or(|process| process.exited());
        let mut update = if process_gone {
            self.apply(host, Session::Off)?
        } else if session.mode().expired(now) {
            self.reconcile(host)?
        } else {
            self.schedule(host)?;
            Update {
                refresh: Refresh::IfChanged,
                notices: Vec::new(),
            }
        };
        update.absorb(self.enforce_policy(host));
        if self.warn && !self.warned && self.session.warning_due(now) {
            self.warned = true;
            update.notices.push(Notice::Ending);
        }
        Ok(update)
    }

    /// Stops an active session when the battery policy no longer allows it.
    pub fn enforce_policy(&mut self, host: &mut impl Host) -> Update {
        if !self.session.active() || self.allowed(host) {
            return Update::default();
        }
        match self.apply(host, Session::Off) {
            Ok(mut update) => {
                update.notices.push(Notice::BatteryStopped);
                update
            }
            Err(_) => Update::default(),
        }
    }

    /// Ends the session and releases the keep-awake request, the wake-up and
    /// the watched process. The state is cleared even when the release fails.
    pub fn shutdown(&mut self, host: &mut impl Host) -> Result<(), Error> {
        let _ = host.wake_after(None);
        self.session = Session::Off;
        self.process = None;
        host.keep_awake(Session::Off.mode(), false)
            .map_err(|_| Error::Power)
    }

    fn allowed(&self, host: &impl Host) -> bool {
        self.policy.allows(host.power_source())
    }

    /// Moves to `session`: the keep-awake request first, then the state, then
    /// the wake-up. A wake-up that cannot be set restores the previous request.
    fn apply(&mut self, host: &mut impl Host, session: Session) -> Result<Update, Error> {
        let previous = self.session;
        host.keep_awake(session.mode(), self.display)
            .map_err(|_| Error::Power)?;
        self.session = session;
        if !matches!(session, Session::Process { .. }) {
            self.process = None;
        }
        if let Err(error) = self.schedule(host) {
            self.session = previous;
            let _ = host.keep_awake(previous.mode(), self.display);
            return Err(error);
        }
        Ok(Update::forced())
    }

    fn schedule(&mut self, host: &mut impl Host) -> Result<(), Error> {
        host.wake_after(self.session.next_tick_ms(host.uptime_ms()))
            .map_err(Error::Timer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeHost;
    use powerkit_core::always_on::{MINUTE_MS, Mode, PROCESS_POLL_MS, PowerSource, WARN_LEAD_MS};
    use powerkit_core::primitives::UtcTicks;

    const MINUTE_TICKS: i64 = 600_000_000;

    fn started(host: &mut FakeHost, session: Session) -> Controller {
        let mut controller = Controller::default();
        controller.start(host, session).unwrap();
        controller
    }
    fn on_battery(percent: u8) -> PowerSource {
        PowerSource {
            on_ac: false,
            battery: Some(percent),
        }
    }

    #[test]
    fn a_new_controller_is_off_and_holds_nothing() {
        let controller = Controller::default();
        assert_eq!(controller.session(), Session::Off);
        assert_eq!(controller.bound_app(), None);
    }

    #[test]
    fn starting_and_stopping_moves_the_request_with_the_session() {
        let mut host = FakeHost::default();
        let mut controller = Controller::default();
        let update = controller.start(&mut host, Session::Indefinite).unwrap();
        assert_eq!(update, Update::forced());
        assert_eq!(host.awake, (Mode::Indefinite, false));
        assert_eq!(host.wake, None);
        let interval = Session::interval(host.uptime, 30, false);
        controller.start(&mut host, interval).unwrap();
        assert_eq!(host.awake.0, Mode::Until(host.uptime + 30 * MINUTE_MS));
        assert_eq!(host.wake, Some(MINUTE_MS));
        controller.start(&mut host, Session::Off).unwrap();
        assert_eq!(host.awake, (Mode::Off, false));
        assert_eq!(host.wake, None);
    }

    #[test]
    fn a_failed_request_leaves_the_session_untouched() {
        let mut host = FakeHost::default();
        let mut controller = started(&mut host, Session::Indefinite);
        host.fail_keep_awake = true;
        assert!(matches!(
            controller.start(&mut host, Session::Off),
            Err(Error::Power)
        ));
        assert_eq!(controller.session(), Session::Indefinite);
        assert_eq!(host.awake.0, Mode::Indefinite);
    }

    #[test]
    fn a_wake_up_that_cannot_be_set_restores_the_previous_request() {
        let mut host = FakeHost::default();
        let mut controller = Controller::default();
        host.fail_wake = true;
        let interval = Session::interval(host.uptime, 30, false);
        assert!(matches!(
            controller.start(&mut host, interval),
            Err(Error::Timer(_))
        ));
        assert_eq!(controller.session(), Session::Off);
        assert_eq!(host.awake.0, Mode::Off);
    }

    #[test]
    fn display_preference_reapplies_and_rolls_back_on_failure() {
        let mut host = FakeHost::default();
        let mut controller = started(&mut host, Session::Indefinite);
        controller.set_display(&mut host, true).unwrap();
        assert_eq!(host.awake, (Mode::Indefinite, true));
        host.fail_keep_awake = true;
        assert!(controller.set_display(&mut host, false).is_err());
        host.fail_keep_awake = false;
        // The rejected preference did not stick.
        controller.start(&mut host, Session::Indefinite).unwrap();
        assert_eq!(host.awake, (Mode::Indefinite, true));
    }

    #[test]
    fn the_deadline_ends_the_session_and_is_never_extended_by_a_wake() {
        let mut host = FakeHost::default();
        let mut controller = started(&mut host, Session::interval(0, 30, false));
        host.uptime = 29 * MINUTE_MS;
        let update = controller.on_wake(&mut host).unwrap();
        assert_eq!(update.refresh, Refresh::IfChanged);
        assert!(controller.session().active());
        host.uptime = 30 * MINUTE_MS;
        let update = controller.on_wake(&mut host).unwrap();
        assert_eq!(update.refresh, Refresh::Forced);
        assert_eq!(controller.session(), Session::Off);
        assert_eq!(host.awake.0, Mode::Off);
        assert_eq!(host.wake, None);
    }

    #[test]
    fn the_end_notice_fires_once_and_not_for_short_sessions() {
        let mut host = FakeHost::default();
        let mut controller = started(&mut host, Session::interval(0, 30, false));
        host.uptime = 30 * MINUTE_MS - WARN_LEAD_MS - 1;
        assert_eq!(controller.on_wake(&mut host).unwrap().notices, []);
        host.uptime += 1;
        assert_eq!(
            controller.on_wake(&mut host).unwrap().notices,
            [Notice::Ending]
        );
        host.uptime += MINUTE_MS;
        assert_eq!(controller.on_wake(&mut host).unwrap().notices, []);
        // Extending moves the deadline out of the lead, so the notice can fire again.
        controller.extend(&mut host).unwrap();
        assert_eq!(controller.on_wake(&mut host).unwrap().notices, []);
        host.uptime = 60 * MINUTE_MS - WARN_LEAD_MS;
        assert_eq!(
            controller.on_wake(&mut host).unwrap().notices,
            [Notice::Ending]
        );
        // A session that starts inside the lead never announces its end.
        let short_session = Session::interval(host.uptime, 3, true);
        let mut short = started(&mut host, short_session);
        host.uptime += MINUTE_MS;
        assert_eq!(short.on_wake(&mut host).unwrap().notices, []);
    }

    #[test]
    fn the_notice_can_be_switched_off() {
        let mut host = FakeHost::default();
        let mut controller = Controller::default();
        let prefs = Preferences {
            always_on_warn: false,
            ..Preferences::default()
        };
        controller.configure(&mut host, &prefs);
        controller
            .start(&mut host, Session::interval(0, 30, false))
            .unwrap();
        host.uptime = 30 * MINUTE_MS - 1;
        assert_eq!(controller.on_wake(&mut host).unwrap().notices, []);
    }

    #[test]
    fn extend_moves_a_deadline_and_ignores_sessions_without_one() {
        let mut host = FakeHost::default();
        let mut controller = started(&mut host, Session::Indefinite);
        assert_eq!(controller.extend(&mut host).unwrap(), Update::default());
        controller
            .start(&mut host, Session::interval(0, 30, false))
            .unwrap();
        controller.extend(&mut host).unwrap();
        assert_eq!(
            controller.session().deadline(),
            Some((30 + u64::from(EXTEND_MINUTES)) * MINUTE_MS)
        );
    }

    #[test]
    fn toggle_stops_then_restarts_the_last_restartable_choice() {
        let mut host = FakeHost::default();
        let mut controller = Controller::default();
        // Nothing was started yet: the shortcut keeps awake indefinitely.
        controller.toggle(&mut host).unwrap();
        assert_eq!(controller.session(), Session::Indefinite);
        controller
            .start(&mut host, Session::interval(0, 60, false))
            .unwrap();
        controller.toggle(&mut host).unwrap();
        assert_eq!(controller.session(), Session::Off);
        host.uptime = 5_000;
        controller.toggle(&mut host).unwrap();
        // A fresh hour from now, not the old deadline.
        assert_eq!(controller.session(), Session::interval(5_000, 60, false));
        // An expiry cannot restart from the shortcut, so the hour is kept.
        let target = UtcTicks(host.utc.0 + 10 * MINUTE_TICKS);
        let expiry = Session::expiry(host.uptime, host.utc, target).unwrap();
        controller.start(&mut host, expiry).unwrap();
        controller.toggle(&mut host).unwrap();
        controller.toggle(&mut host).unwrap();
        assert!(matches!(
            controller.session(),
            Session::Interval { minutes: 60, .. }
        ));
    }

    #[test]
    fn a_custom_interval_restarts_with_the_current_length() {
        let mut host = FakeHost::default();
        let mut controller = started(&mut host, Session::interval(0, 45, true));
        controller.toggle(&mut host).unwrap();
        let prefs = Preferences {
            interval_minutes: 90,
            ..Preferences::default()
        };
        controller.configure(&mut host, &prefs);
        controller.toggle(&mut host).unwrap();
        assert_eq!(controller.session(), Session::interval(0, 90, true));
    }

    #[test]
    fn a_clock_change_reprojects_an_expiry_and_a_passed_target_ends_it() {
        let mut host = FakeHost::default();
        let target = UtcTicks(host.utc.0 + 60 * MINUTE_TICKS);
        let expiry = Session::expiry(host.uptime, host.utc, target).unwrap();
        let mut controller = started(&mut host, expiry);
        // The wall clock jumped 30 minutes while uptime advanced one second.
        host.uptime += 1_000;
        host.utc = UtcTicks(host.utc.0 + 30 * MINUTE_TICKS);
        controller.reconcile(&mut host).unwrap();
        assert_eq!(controller.session().remaining_minutes(host.uptime), 30);
        host.utc = target;
        controller.reconcile(&mut host).unwrap();
        assert_eq!(controller.session(), Session::Off);
        assert_eq!(host.awake.0, Mode::Off);
    }

    #[test]
    fn a_resume_never_extends_an_interval() {
        let mut host = FakeHost::default();
        let mut controller = started(&mut host, Session::interval(0, 30, false));
        // Uptime includes the time asleep, so the deadline has passed.
        host.uptime = 45 * MINUTE_MS;
        controller.reconcile(&mut host).unwrap();
        assert_eq!(controller.session(), Session::Off);
    }

    #[test]
    fn battery_policy_blocks_a_start_and_stops_a_running_session() {
        let mut host = FakeHost::default();
        let mut controller = Controller::default();
        let prefs = Preferences {
            always_on_battery: BatteryPolicy {
                ac_only: false,
                floor: 20,
            },
            ..Preferences::default()
        };
        controller.configure(&mut host, &prefs);
        host.source = on_battery(10);
        assert!(matches!(
            controller.start(&mut host, Session::Indefinite),
            Err(Error::BatteryBlocked)
        ));
        assert_eq!(host.awake.0, Mode::Off);
        // Stopping is always allowed.
        controller.start(&mut host, Session::Off).unwrap();
        host.source = on_battery(50);
        controller.start(&mut host, Session::Indefinite).unwrap();
        host.source = on_battery(19);
        let update = controller.enforce_policy(&mut host);
        assert_eq!(update.notices, [Notice::BatteryStopped]);
        assert_eq!(update.refresh, Refresh::Forced);
        assert_eq!(controller.session(), Session::Off);
        assert_eq!(host.awake.0, Mode::Off);
        // Nothing left to stop: no second notice.
        assert_eq!(controller.enforce_policy(&mut host), Update::default());
    }

    #[test]
    fn a_stricter_policy_applies_as_soon_as_it_is_configured() {
        let mut host = FakeHost::default();
        host.source = on_battery(80);
        let mut controller = started(&mut host, Session::Indefinite);
        let prefs = Preferences {
            always_on_battery: BatteryPolicy {
                ac_only: true,
                floor: 0,
            },
            ..Preferences::default()
        };
        let update = controller.configure(&mut host, &prefs);
        assert_eq!(update.notices, [Notice::BatteryStopped]);
        assert_eq!(controller.session(), Session::Off);
    }

    #[test]
    fn a_bound_session_follows_its_process_and_polls_it() {
        let mut host = FakeHost::default();
        let exited = host.add_process(42, "editor");
        let mut controller = Controller::default();
        controller.bind(&mut host, 42).unwrap();
        assert_eq!(controller.session(), Session::Process { pid: 42 });
        assert_eq!(controller.bound_app(), Some("editor"));
        assert_eq!(host.awake.0, Mode::Indefinite);
        assert_eq!(host.wake, Some(PROCESS_POLL_MS));
        assert!(controller.on_wake(&mut host).unwrap().notices.is_empty());
        assert!(controller.session().active());
        exited.set(true);
        controller.on_wake(&mut host).unwrap();
        assert_eq!(controller.session(), Session::Off);
        assert_eq!(controller.bound_app(), None);
        assert_eq!(host.awake.0, Mode::Off);
    }

    #[test]
    fn binding_refuses_itself_missing_processes_and_keeps_the_old_binding_on_failure() {
        let mut host = FakeHost::default();
        host.add_process(42, "editor");
        host.add_process(43, "player");
        let mut controller = Controller::default();
        let own = host.pid;
        assert!(matches!(
            controller.bind(&mut host, own),
            Err(Error::NoProcess)
        ));
        assert!(matches!(
            controller.bind(&mut host, 999),
            Err(Error::ProcessUnavailable)
        ));
        assert_eq!(controller.session(), Session::Off);
        controller.bind(&mut host, 42).unwrap();
        host.fail_keep_awake = true;
        assert!(controller.bind(&mut host, 43).is_err());
        // The session and the process it follows are unchanged.
        assert_eq!(controller.session(), Session::Process { pid: 42 });
        assert_eq!(controller.bound_app(), Some("editor"));
    }

    #[test]
    fn another_session_drops_the_watched_process() {
        let mut host = FakeHost::default();
        host.add_process(42, "editor");
        let mut controller = Controller::default();
        controller.bind(&mut host, 42).unwrap();
        controller.start(&mut host, Session::Indefinite).unwrap();
        assert_eq!(controller.bound_app(), None);
    }

    #[test]
    fn command_line_requests_map_to_sessions() {
        let mut host = FakeHost::default();
        host.add_process(42, "editor");
        let mut controller = Controller::default();
        controller.request(&mut host, Request::Minutes(15)).unwrap();
        assert_eq!(controller.session(), Session::interval(0, 15, false));
        controller.request(&mut host, Request::Indefinite).unwrap();
        assert_eq!(controller.session(), Session::Indefinite);
        controller.request(&mut host, Request::Process(42)).unwrap();
        assert_eq!(controller.session(), Session::Process { pid: 42 });
        controller.request(&mut host, Request::Off).unwrap();
        assert_eq!(controller.session(), Session::Off);
    }

    #[test]
    fn shutdown_releases_everything_even_when_the_release_fails() {
        let mut host = FakeHost::default();
        host.add_process(42, "editor");
        let mut controller = Controller::default();
        controller.bind(&mut host, 42).unwrap();
        controller.shutdown(&mut host).unwrap();
        assert_eq!(controller.session(), Session::Off);
        assert_eq!(controller.bound_app(), None);
        assert_eq!(host.awake, (Mode::Off, false));
        assert_eq!(host.wake, None);
        let mut controller = started(&mut host, Session::Indefinite);
        host.fail_keep_awake = true;
        assert!(matches!(controller.shutdown(&mut host), Err(Error::Power)));
        assert_eq!(controller.session(), Session::Off);
    }
}
