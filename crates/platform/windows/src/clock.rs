//! The Windows clock and calendar behind the neutral [`UtcTicks`]: the system
//! time, local calendar input for an AlwaysOn expiry and local date text.
//! `UtcTicks` and `FILETIME` share an epoch and unit, so the conversion is exact.
use powerkit_core::media_renamer::time::WallClock;
use powerkit_core::primitives::UtcTicks;
use windows_sys::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    System::{
        SystemInformation::GetSystemTimeAsFileTime,
        Time::{
            DYNAMIC_TIME_ZONE_INFORMATION, FileTimeToSystemTime, GetDynamicTimeZoneInformation,
            SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
            SystemTimeToTzSpecificLocalTimeEx, TIME_ZONE_ID_INVALID,
            TzSpecificLocalTimeToSystemTime,
        },
    },
};

const TICKS_PER_SECOND: i64 = 10_000_000;
/// FILETIME ticks at 1970-01-01 UTC.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
const TICKS_PER_MINUTE: i64 = 600_000_000;
const TICKS_PER_DAY: i64 = 24 * 60 * TICKS_PER_MINUTE;

pub fn now_utc() -> UtcTicks {
    let mut time = FILETIME::default();
    // SAFETY: writes one FILETIME into a live local.
    unsafe { GetSystemTimeAsFileTime(&mut time) };
    UtcTicks(ticks(time))
}
/// A file time as `std::os::windows::fs::MetadataExt` reports it.
pub fn file_time(raw: u64) -> UtcTicks {
    UtcTicks(raw as i64)
}
fn ticks(time: FILETIME) -> i64 {
    ((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)) as i64
}
fn filetime(ticks: i64) -> FILETIME {
    FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks as u64 >> 32) as u32,
    }
}
/// Calendar fields of a tick count, without any time zone conversion.
fn fields(ticks: i64) -> Option<SYSTEMTIME> {
    let mut fields = SYSTEMTIME::default();
    // SAFETY: both pointers reference live locals for the duration of the call.
    (unsafe { FileTimeToSystemTime(&filetime(ticks), &mut fields) } != 0).then_some(fields)
}
fn from_fields(fields: &SYSTEMTIME) -> Option<i64> {
    let mut time = FILETIME::default();
    // SAFETY: both pointers reference live locals for the duration of the call.
    (unsafe { SystemTimeToFileTime(fields, &mut time) } != 0).then(|| ticks(time))
}
fn to_local(utc: i64) -> Option<SYSTEMTIME> {
    let mut local = SYSTEMTIME::default();
    // SAFETY: a null zone selects the current one; pointers reference live locals.
    (unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &fields(utc)?, &mut local) } != 0)
        .then_some(local)
}
fn to_utc(local: &SYSTEMTIME) -> Option<i64> {
    let mut utc = SYSTEMTIME::default();
    // SAFETY: a null zone selects the current one; pointers reference live locals.
    if unsafe { TzSpecificLocalTimeToSystemTime(std::ptr::null(), local, &mut utc) } == 0 {
        return None;
    }
    from_fields(&utc)
}
/// UTC ticks of `minute_of_day` on the local calendar day that contains `day`.
fn on_local_day(day: i64, minute_of_day: u32) -> Option<i64> {
    let mut local = to_local(day)?;
    local.wHour = (minute_of_day / 60) as u16;
    local.wMinute = (minute_of_day % 60) as u16;
    local.wSecond = 0;
    local.wMilliseconds = 0;
    to_utc(&local)
}
/// The next wall-clock instant for a local date and time of day.
///
/// `date` is any instant on the chosen local day (DatePicker output). Without a
/// date, a time that already passed today means tomorrow. Returns `None` for a
/// target that is not in the future.
pub fn expiry_target(
    date: Option<UtcTicks>,
    minute_of_day: u32,
    now: UtcTicks,
) -> Option<UtcTicks> {
    let now = now.0;
    let minute_of_day = minute_of_day.min(24 * 60 - 1);
    let target = on_local_day(date.map_or(now, |date| date.0), minute_of_day)?;
    if target > now {
        return Some(UtcTicks(target));
    }
    if date.is_some() {
        return None;
    }
    let tomorrow = on_local_day(target + TICKS_PER_DAY, minute_of_day)?;
    (tomorrow > now).then_some(UtcTicks(tomorrow))
}
/// "18:30" for today, otherwise "2026-11-16 18:30" in local time.
pub fn label(target: UtcTicks, now: UtcTicks) -> String {
    let (Some(at), Some(today)) = (to_local(target.0), to_local(now.0)) else {
        return String::new();
    };
    let time = format!("{:02}:{:02}", at.wHour, at.wMinute);
    if (at.wYear, at.wMonth, at.wDay) == (today.wYear, today.wMonth, today.wDay) {
        time
    } else {
        format!("{:04}-{:02}-{:02} {time}", at.wYear, at.wMonth, at.wDay)
    }
}
/// "2026-11-16" in local time, for package install and update dates.
pub fn local_date(utc: UtcTicks) -> Option<String> {
    (utc.0 > 0).then_some(())?;
    let at = to_local(utc.0)?;
    (at.wYear >= 1980).then(|| format!("{:04}-{:02}-{:02}", at.wYear, at.wMonth, at.wDay))
}
pub fn filetime_local_date(time: FILETIME) -> Option<String> {
    local_date(UtcTicks(ticks(time)))
}
/// "2026-11-16 18:30" in local time, for history rows and check times.
pub fn local_stamp(utc: UtcTicks) -> String {
    to_local(utc.0)
        .map(|at| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                at.wYear, at.wMonth, at.wDay, at.wHour, at.wMinute
            )
        })
        .unwrap_or_default()
}
/// ISO 8601 UTC text in the shape `winget export` writes for `CreationDate`.
pub fn iso_utc(utc: UtcTicks) -> String {
    fields(utc.0)
        .map(|at| {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}-00:00",
                at.wYear, at.wMonth, at.wDay, at.wHour, at.wMinute, at.wSecond, at.wMilliseconds
            )
        })
        .unwrap_or_default()
}
pub fn unix_to_ticks(seconds: i64) -> Option<UtcTicks> {
    seconds
        .checked_mul(TICKS_PER_SECOND)?
        .checked_add(UNIX_EPOCH_TICKS)
        .map(UtcTicks)
}
/// Local wall-clock time of a UTC instant, using the time-zone rules for that date
/// (dynamic DST), not today's offset.
pub fn local_wall_clock(utc: UtcTicks) -> Option<WallClock> {
    let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // SAFETY: writes one structure into a live local.
    if unsafe { GetDynamicTimeZoneInformation(&mut zone) } == TIME_ZONE_ID_INVALID {
        return None;
    }
    let mut local = SYSTEMTIME::default();
    // SAFETY: all three pointers reference live locals for the duration of the call.
    if unsafe { SystemTimeToTzSpecificLocalTimeEx(&zone, &fields(utc.0)?, &mut local) } == 0 {
        return None;
    }
    WallClock::new(
        local.wYear,
        local.wMonth as u8,
        local.wDay as u8,
        local.wHour as u8,
        local.wMinute as u8,
        local.wSecond as u8,
    )
}
/// Minutes since local midnight from a TimePicker value (100 ns ticks).
pub fn minute_of_day(time_of_day: i64) -> Option<u32> {
    let minutes = time_of_day.div_euclid(TICKS_PER_MINUTE);
    (0..24 * 60).contains(&minutes).then_some(minutes as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local_minute(utc: UtcTicks) -> u32 {
        let local = to_local(utc.0).unwrap();
        u32::from(local.wHour) * 60 + u32::from(local.wMinute)
    }
    #[test]
    fn time_without_date_rolls_to_the_next_occurrence() {
        // Native integration with the machine's current time zone.
        let now = now_utc();
        let current = local_minute(now);
        let earlier = (current + 24 * 60 - 1) % (24 * 60);
        let target = expiry_target(None, earlier, now).unwrap();
        assert!(target > now);
        assert!(target.0 - now.0 <= TICKS_PER_DAY + 60 * TICKS_PER_MINUTE);
        assert_eq!(local_minute(target), earlier);
        let later = (current + 2) % (24 * 60);
        let soon = expiry_target(None, later, now).unwrap();
        assert!(soon > now && soon.0 - now.0 <= 3 * TICKS_PER_MINUTE + TICKS_PER_DAY);
        assert_eq!(local_minute(soon), later);
    }
    #[test]
    fn explicit_past_date_is_rejected_and_future_date_is_kept() {
        let now = now_utc();
        let days = |days: i64| UtcTicks(now.0 + days * TICKS_PER_DAY);
        assert_eq!(expiry_target(Some(days(-2)), 0, now), None);
        let future = expiry_target(Some(days(2)), 9 * 60, now).unwrap();
        assert!(future > days(1));
        assert_eq!(local_minute(future), 9 * 60);
    }
    #[test]
    fn label_omits_today_and_shows_other_dates() {
        let now = now_utc();
        let today = label(now, now);
        assert_eq!(today.len(), 5);
        assert_eq!(&today[2..3], ":");
        let other = label(UtcTicks(now.0 + 3 * TICKS_PER_DAY), now);
        assert_eq!(other.len(), 16);
    }
    #[test]
    fn stamps_have_fixed_shapes() {
        let now = now_utc();
        assert_eq!(local_stamp(now).len(), 16);
        let iso = iso_utc(now);
        assert_eq!(iso.len(), 29);
        assert!(iso.ends_with("-00:00") && &iso[10..11] == "T");
    }
    #[test]
    fn utc_instants_convert_with_that_dates_rules() {
        let summer = local_wall_clock(unix_to_ticks(1_782_910_800).unwrap()).unwrap();
        let winter = local_wall_clock(unix_to_ticks(1_798_848_000).unwrap()).unwrap();
        assert_eq!((summer.year, summer.month), (2026, 7));
        assert_eq!(winter.year, 2027);
        assert_eq!(local_wall_clock(UtcTicks(-1)), None);
        // The adapter and the neutral type agree on the Unix epoch.
        assert_eq!(
            unix_to_ticks(1_782_910_800),
            Some(UtcTicks::from_unix_ms(1_782_910_800_000))
        );
    }
    #[test]
    fn time_picker_values_map_to_minutes() {
        assert_eq!(minute_of_day(0), Some(0));
        assert_eq!(
            minute_of_day(18 * 60 * TICKS_PER_MINUTE + 30 * TICKS_PER_MINUTE),
            Some(1110)
        );
        assert_eq!(minute_of_day(24 * 60 * TICKS_PER_MINUTE), None);
        assert_eq!(minute_of_day(-1), None);
    }
}
