//! Wall-clock conversions for AlwaysOn expiry: local calendar input to UTC FILETIME ticks.
use windows_sys::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    System::{
        SystemInformation::GetSystemTimeAsFileTime,
        Time::{
            FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
            TzSpecificLocalTimeToSystemTime,
        },
    },
};

const TICKS_PER_MINUTE: i64 = 600_000_000;
const TICKS_PER_DAY: i64 = 24 * 60 * TICKS_PER_MINUTE;

pub(super) fn now_utc() -> i64 {
    let mut time = FILETIME::default();
    // SAFETY: writes one FILETIME into a live local.
    unsafe { GetSystemTimeAsFileTime(&mut time) };
    ticks(time)
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
pub(super) fn expiry_target(date: Option<i64>, minute_of_day: u32, now: i64) -> Option<i64> {
    let minute_of_day = minute_of_day.min(24 * 60 - 1);
    let target = on_local_day(date.unwrap_or(now), minute_of_day)?;
    if target > now {
        return Some(target);
    }
    if date.is_some() {
        return None;
    }
    let tomorrow = on_local_day(target + TICKS_PER_DAY, minute_of_day)?;
    (tomorrow > now).then_some(tomorrow)
}
/// "18:30" for today, otherwise "2026-11-16 18:30" in local time.
pub(super) fn label(target: i64, now: i64) -> String {
    let (Some(at), Some(today)) = (to_local(target), to_local(now)) else {
        return String::new();
    };
    let time = format!("{:02}:{:02}", at.wHour, at.wMinute);
    if (at.wYear, at.wMonth, at.wDay) == (today.wYear, today.wMonth, today.wDay) {
        time
    } else {
        format!("{:04}-{:02}-{:02} {time}", at.wYear, at.wMonth, at.wDay)
    }
}
/// Minutes since local midnight from a TimePicker value (100 ns ticks).
pub(super) fn minute_of_day(time_of_day: i64) -> Option<u32> {
    let minutes = time_of_day.div_euclid(TICKS_PER_MINUTE);
    (0..24 * 60).contains(&minutes).then_some(minutes as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local_minute(utc: i64) -> u32 {
        let local = to_local(utc).unwrap();
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
        assert!(target - now <= TICKS_PER_DAY + 60 * TICKS_PER_MINUTE);
        assert_eq!(local_minute(target), earlier);
        let later = (current + 2) % (24 * 60);
        let soon = expiry_target(None, later, now).unwrap();
        assert!(soon > now && soon - now <= 3 * TICKS_PER_MINUTE + TICKS_PER_DAY);
        assert_eq!(local_minute(soon), later);
    }
    #[test]
    fn explicit_past_date_is_rejected_and_future_date_is_kept() {
        let now = now_utc();
        assert_eq!(expiry_target(Some(now - 2 * TICKS_PER_DAY), 0, now), None);
        let future = expiry_target(Some(now + 2 * TICKS_PER_DAY), 9 * 60, now).unwrap();
        assert!(future > now + TICKS_PER_DAY);
        assert_eq!(local_minute(future), 9 * 60);
    }
    #[test]
    fn label_omits_today_and_shows_other_dates() {
        let now = now_utc();
        let today = label(now, now);
        assert_eq!(today.len(), 5);
        assert_eq!(&today[2..3], ":");
        let other = label(now + 3 * TICKS_PER_DAY, now);
        assert_eq!(other.len(), 16);
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
