//! Platform-independent AlwaysOn session state. Deadlines use monotonic uptime.
//!
//! Wall-clock targets are [`UtcTicks`]; the platform adapter reads the clock and
//! converts local calendar input before it reaches this module.
use crate::primitives::UtcTicks;

pub const MINUTE_MS: u64 = 60_000;
/// Tray and settings presets, in minutes.
pub const PRESET_MINUTES: [u32; 3] = [30, 60, 120];
pub const DEFAULT_INTERVAL_MINUTES: u32 = 240;
pub const MAX_INTERVAL_HOURS: u32 = 99;
pub const MAX_INTERVAL_MINUTES: u32 = MAX_INTERVAL_HOURS * 60 + 59;
/// The end-of-session notice fires this long before a deadline.
pub const WARN_LEAD_MS: u64 = 5 * MINUTE_MS;
pub const EXTEND_MINUTES: u32 = 30;
/// How often a process-bound session checks that its process still runs.
pub const PROCESS_POLL_MS: u64 = 5_000;
pub const BATTERY_FLOOR_STEP: u8 = 10;
pub const MAX_BATTERY_FLOOR: u8 = 90;

pub fn clamp_interval(minutes: i64) -> u32 {
    minutes.clamp(1, i64::from(MAX_INTERVAL_MINUTES)) as u32
}
/// 0 disables the floor; otherwise a multiple of the step up to the maximum.
pub fn clamp_battery_floor(percent: i32) -> u8 {
    let step = i32::from(BATTERY_FLOOR_STEP);
    (percent.clamp(0, i32::from(MAX_BATTERY_FLOOR)) / step * step) as u8
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Indefinite,
    Until(u64),
}
impl Mode {
    pub fn expired(self, now_ms: u64) -> bool {
        matches!(self, Self::Until(end) if now_ms >= end)
    }
    pub fn active(self) -> bool {
        self != Self::Off
    }
}

/// What the user started. `Mode` drives the power request; the session also
/// remembers how a deadline was chosen so the tray and settings can show it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Session {
    #[default]
    Off,
    Indefinite,
    /// A preset or the custom interval, in minutes.
    Interval {
        end: u64,
        minutes: u32,
        custom: bool,
    },
    /// Keep awake until a wall-clock instant; `end` is its uptime projection.
    Expiry {
        end: u64,
        target: UtcTicks,
    },
    /// Keep awake while a process runs; the adapter owns its handle.
    Process {
        pid: u32,
    },
}
impl Session {
    pub fn interval(now_ms: u64, minutes: u32, custom: bool) -> Self {
        Self::Interval {
            end: now_ms.saturating_add(u64::from(minutes) * MINUTE_MS),
            minutes,
            custom,
        }
    }
    /// `None` when the target is not in the future.
    pub fn expiry(now_ms: u64, now_utc: UtcTicks, target: UtcTicks) -> Option<Self> {
        let remaining_ms = target.ms_after(now_utc)?;
        Some(Self::Expiry {
            end: now_ms.saturating_add(remaining_ms),
            target,
        })
    }
    pub fn mode(self) -> Mode {
        match self {
            Self::Off => Mode::Off,
            Self::Indefinite | Self::Process { .. } => Mode::Indefinite,
            Self::Interval { end, .. } | Self::Expiry { end, .. } => Mode::Until(end),
        }
    }
    pub fn deadline(self) -> Option<u64> {
        match self.mode() {
            Mode::Until(end) => Some(end),
            _ => None,
        }
    }
    /// Pushes a deadline later; sessions without one are unchanged.
    pub fn extended(self, minutes: u32) -> Self {
        let ms = u64::from(minutes) * MINUTE_MS;
        match self {
            Self::Interval {
                end,
                minutes: chosen,
                custom,
            } => Self::Interval {
                end: end.saturating_add(ms),
                minutes: chosen,
                custom,
            },
            Self::Expiry { end, target } => Self::Expiry {
                end: end.saturating_add(ms),
                target: target.plus_ms(ms),
            },
            other => other,
        }
    }
    /// Delay until the shown minute count changes, the deadline passes, or a
    /// bound process should be polled. `None` needs no timer.
    pub fn next_tick_ms(self, now_ms: u64) -> Option<u64> {
        if let Self::Process { .. } = self {
            return Some(PROCESS_POLL_MS);
        }
        let remaining = self.deadline()?.saturating_sub(now_ms);
        let to_minute = match remaining % MINUTE_MS {
            0 => MINUTE_MS,
            partial => partial,
        };
        Some(to_minute.min(remaining).max(1))
    }
    /// True once a deadline is within the notice lead.
    pub fn warning_due(self, now_ms: u64) -> bool {
        self.deadline()
            .is_some_and(|end| end.saturating_sub(now_ms) <= WARN_LEAD_MS)
    }
    pub fn active(self) -> bool {
        self.mode().active()
    }
    /// Ends a session whose deadline passed; never extends it.
    pub fn at(self, now_ms: u64) -> Self {
        if self.mode().expired(now_ms) {
            Self::Off
        } else {
            self
        }
    }
    /// Re-projects a wall-clock target after the system clock or time zone changed.
    pub fn retimed(self, now_ms: u64, now_utc: UtcTicks) -> Self {
        match self {
            Self::Expiry { target, .. } => {
                Self::expiry(now_ms, now_utc, target).unwrap_or(Self::Off)
            }
            other => other.at(now_ms),
        }
    }
    pub fn remaining_minutes(self, now_ms: u64) -> u64 {
        match self.mode() {
            Mode::Until(end) => end.saturating_sub(now_ms).div_ceil(MINUTE_MS),
            _ => 0,
        }
    }
}

/// How a session was chosen: the entry a mode list selects and the choice the
/// toggle shortcut starts again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Off,
    Indefinite,
    /// One of [`PRESET_MINUTES`].
    Preset(u32),
    /// The user's custom interval length.
    CustomInterval,
    Expiry,
    Process,
}
impl Choice {
    /// The choice that represents a running session.
    pub fn of(session: Session) -> Self {
        match session {
            Session::Off => Self::Off,
            Session::Indefinite => Self::Indefinite,
            Session::Interval {
                minutes,
                custom: false,
                ..
            } if PRESET_MINUTES.contains(&minutes) => Self::Preset(minutes),
            // A custom interval equal to a preset still shows as the custom choice.
            Session::Interval { .. } => Self::CustomInterval,
            Session::Expiry { .. } => Self::Expiry,
            Session::Process { .. } => Self::Process,
        }
    }
    /// The session this choice starts now. An expiry needs a picked time and a
    /// process a pid, so neither starts from the choice alone.
    pub fn start(self, now_ms: u64, interval_minutes: u32) -> Option<Session> {
        Some(match self {
            Self::Off => Session::Off,
            Self::Indefinite => Session::Indefinite,
            Self::Preset(minutes) => Session::interval(now_ms, minutes, false),
            Self::CustomInterval => Session::interval(now_ms, interval_minutes, true),
            Self::Expiry | Self::Process => return None,
        })
    }
}

/// What the battery currently reports. `battery` is `None` without a battery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerSource {
    pub on_ac: bool,
    pub battery: Option<u8>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatteryPolicy {
    pub ac_only: bool,
    /// Stop below this percentage on battery; 0 disables the floor.
    pub floor: u8,
}
impl BatteryPolicy {
    pub fn allows(self, source: PowerSource) -> bool {
        if source.on_ac {
            return true;
        }
        if self.ac_only {
            return false;
        }
        match source.battery {
            Some(percent) => self.floor == 0 || percent >= self.floor,
            None => true,
        }
    }
}

/// A command-line AlwaysOn request: `--always-on off|on|indefinite|MINUTES`
/// or `--always-on-pid PID`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Off,
    Indefinite,
    Minutes(u32),
    Process(u32),
}
impl Request {
    pub fn parse(args: &[String]) -> Option<Self> {
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--always-on" => {
                    return match args.next()?.to_ascii_lowercase().as_str() {
                        "off" => Some(Self::Off),
                        "on" | "indefinite" => Some(Self::Indefinite),
                        minutes => {
                            let minutes = minutes.parse::<i64>().ok().filter(|m| *m > 0)?;
                            Some(Self::Minutes(clamp_interval(minutes)))
                        }
                    };
                }
                "--always-on-pid" => {
                    let pid = args.next()?.parse::<u32>().ok().filter(|pid| *pid != 0)?;
                    return Some(Self::Process(pid));
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MINUTE_TICKS: i64 = 600_000_000;
    /// An instant `minutes` minutes and `ticks` ticks after the epoch.
    fn utc(minutes: i64, ticks: i64) -> UtcTicks {
        UtcTicks(minutes * MINUTE_TICKS + ticks)
    }
    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }
    #[test]
    fn choices_start_their_sessions_and_name_running_ones() {
        let [half, hour, two] = PRESET_MINUTES;
        for choice in [
            Choice::Off,
            Choice::Indefinite,
            Choice::Preset(half),
            Choice::Preset(hour),
            Choice::Preset(two),
            Choice::CustomInterval,
        ] {
            let session = choice.start(1_000, 45).unwrap();
            assert_eq!(Choice::of(session), choice);
        }
        assert_eq!(
            Choice::CustomInterval.start(0, 195).map(Session::mode),
            Some(Mode::Until(195 * MINUTE_MS))
        );
        // A custom interval equal to a preset stays the custom choice.
        assert_eq!(
            Choice::of(Session::interval(0, hour, true)),
            Choice::CustomInterval
        );
        // A non-preset length that was not marked custom is still not a preset.
        assert_eq!(
            Choice::of(Session::interval(0, 45, false)),
            Choice::CustomInterval
        );
        assert_eq!(
            Choice::of(Session::Expiry {
                end: 1,
                target: utc(0, 1)
            }),
            Choice::Expiry
        );
        assert_eq!(Choice::of(Session::Process { pid: 8 }), Choice::Process);
        assert_eq!(Choice::Expiry.start(0, 60), None);
        assert_eq!(Choice::Process.start(0, 60), None);
    }
    #[test]
    fn extension_moves_deadlines_only() {
        let interval = Session::interval(0, 30, false);
        assert_eq!(
            interval.extended(EXTEND_MINUTES).deadline(),
            Some(60 * MINUTE_MS)
        );
        // The radio choice (preset minutes) survives an extension.
        assert!(matches!(
            interval.extended(30),
            Session::Interval {
                minutes: 30,
                custom: false,
                ..
            }
        ));
        let expiry = Session::expiry(0, utc(0, 0), utc(1, 0)).unwrap();
        assert_eq!(
            expiry.extended(30),
            Session::Expiry {
                end: 31 * MINUTE_MS,
                target: utc(31, 0)
            }
        );
        assert_eq!(Session::Indefinite.extended(30), Session::Indefinite);
        assert_eq!(
            Session::Process { pid: 4 }.extended(30),
            Session::Process { pid: 4 }
        );
    }
    #[test]
    fn timer_wakes_at_minute_changes_deadlines_and_process_polls() {
        let session = Session::interval(0, 10, false);
        assert_eq!(session.next_tick_ms(0), Some(MINUTE_MS));
        assert_eq!(session.next_tick_ms(1_500), Some(MINUTE_MS - 1_500));
        assert_eq!(session.next_tick_ms(10 * MINUTE_MS - 200), Some(200));
        assert_eq!(session.next_tick_ms(10 * MINUTE_MS), Some(1));
        assert_eq!(Session::Indefinite.next_tick_ms(0), None);
        assert_eq!(Session::Off.next_tick_ms(0), None);
        assert_eq!(
            Session::Process { pid: 9 }.next_tick_ms(0),
            Some(PROCESS_POLL_MS)
        );
    }
    #[test]
    fn warning_is_due_within_the_lead() {
        let session = Session::interval(0, 10, false);
        assert!(!session.warning_due(5 * MINUTE_MS - 1));
        assert!(session.warning_due(5 * MINUTE_MS));
        assert!(!Session::Indefinite.warning_due(u64::MAX));
        assert!(!Session::Process { pid: 1 }.warning_due(u64::MAX));
    }
    #[test]
    fn battery_policy_never_stops_on_ac_or_without_a_battery() {
        let ac = PowerSource {
            on_ac: true,
            battery: Some(5),
        };
        let low = PowerSource {
            on_ac: false,
            battery: Some(15),
        };
        let none = PowerSource {
            on_ac: false,
            battery: None,
        };
        let strict = BatteryPolicy {
            ac_only: true,
            floor: 0,
        };
        let floor = BatteryPolicy {
            ac_only: false,
            floor: 20,
        };
        assert!(strict.allows(ac));
        assert!(!strict.allows(low));
        assert!(!strict.allows(none));
        assert!(floor.allows(ac));
        assert!(!floor.allows(low));
        assert!(floor.allows(PowerSource {
            battery: Some(20),
            ..low
        }));
        assert!(floor.allows(none));
        assert!(BatteryPolicy::default().allows(low));
        assert_eq!(clamp_battery_floor(-5), 0);
        assert_eq!(clamp_battery_floor(25), 20);
        assert_eq!(clamp_battery_floor(200), MAX_BATTERY_FLOOR);
    }
    #[test]
    fn command_line_requests_parse_and_round_trip() {
        assert_eq!(
            Request::parse(&args("powerkit.exe --always-on 90")),
            Some(Request::Minutes(90))
        );
        assert_eq!(
            Request::parse(&args("x --always-on ON")),
            Some(Request::Indefinite)
        );
        assert_eq!(
            Request::parse(&args("x --always-on off")),
            Some(Request::Off)
        );
        assert_eq!(
            Request::parse(&args("x --always-on 999999")),
            Some(Request::Minutes(MAX_INTERVAL_MINUTES))
        );
        assert_eq!(
            Request::parse(&args("x --always-on-pid 4242")),
            Some(Request::Process(4242))
        );
        for bad in [
            "x",
            "x --always-on",
            "x --always-on 0",
            "x --always-on soon",
            "x --always-on-pid 0",
            "x --always-on-pid -3",
        ] {
            assert_eq!(Request::parse(&args(bad)), None, "{bad}");
        }
    }

    #[test]
    fn deadline_boundary_and_resume_past_deadline() {
        let mode = Mode::Until(1100);
        assert!(!mode.expired(1099));
        assert!(mode.expired(1100));
        assert!(mode.expired(9000));
    }
    #[test]
    fn resume_reconciles_deadline_without_extending_session() {
        let session = Session::interval(100, 1, false);
        let end = 100 + MINUTE_MS;
        assert_eq!(session.at(500), session);
        assert_eq!(session.at(end), Session::Off);
        assert_eq!(session.at(end * 9), Session::Off);
        assert_eq!(Session::Indefinite.at(u64::MAX), Session::Indefinite);
    }
    #[test]
    fn persistent_modes_do_not_expire() {
        assert!(!Mode::Off.active());
        assert!(Mode::Indefinite.active());
        assert!(!Mode::Indefinite.expired(u64::MAX));
        assert!(!Mode::Off.expired(u64::MAX));
    }
    #[test]
    fn deadline_does_not_wrap() {
        assert_eq!(
            Session::interval(u64::MAX - 1, 30, false).mode(),
            Mode::Until(u64::MAX)
        );
    }
    #[test]
    fn interval_keeps_its_origin_and_expires() {
        let custom = Session::interval(1_000, 195, true);
        assert_eq!(custom.mode(), Mode::Until(1_000 + 195 * MINUTE_MS));
        assert_eq!(custom.remaining_minutes(1_000), 195);
        assert_eq!(custom.remaining_minutes(1_001), 195);
        assert_eq!(custom.at(1_000 + 195 * MINUTE_MS), Session::Off);
        assert_ne!(custom, Session::interval(1_000, 195, false));
    }
    #[test]
    fn expiry_rejects_past_targets_and_rounds_up() {
        let now_utc = utc(1_000, 0);
        assert_eq!(Session::expiry(0, now_utc, now_utc), None);
        assert_eq!(Session::expiry(0, now_utc, utc(1_000, -1)), None);
        assert_eq!(
            Session::expiry(5, now_utc, utc(1_000, 1)),
            Some(Session::Expiry {
                end: 6,
                target: utc(1_000, 1)
            })
        );
        let session = Session::expiry(0, now_utc, utc(1_090, 0)).unwrap();
        assert_eq!(session.remaining_minutes(0), 90);
    }
    #[test]
    fn clock_change_reprojects_expiry_but_not_intervals() {
        let now_utc = utc(1_000, 0);
        let target = utc(1_060, 0);
        let session = Session::expiry(0, now_utc, target).unwrap();
        // The clock jumped 30 minutes forward while uptime advanced one second.
        let moved = session.retimed(1_000, utc(1_030, 0));
        assert_eq!(moved.remaining_minutes(1_000), 30);
        assert_eq!(session.retimed(1_000, target), Session::Off);
        let interval = Session::interval(0, 60, true);
        assert_eq!(interval.retimed(1_000, utc(1_030, 0)), interval);
        assert_eq!(interval.retimed(60 * MINUTE_MS, now_utc), Session::Off);
    }
    #[test]
    fn interval_bounds() {
        assert_eq!(clamp_interval(0), 1);
        assert_eq!(clamp_interval(-5), 1);
        assert_eq!(clamp_interval(90), 90);
        assert_eq!(clamp_interval(i64::MAX), MAX_INTERVAL_MINUTES);
    }
}
