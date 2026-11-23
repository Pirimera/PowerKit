//! Delivers `powerkit_app::event::Event` values to the app window.
//!
//! Worker threads cannot touch the application state, so they post one
//! message whose wParam names the event; the app window handles it on the UI
//! thread. Posting is thread-safe and never blocks the worker.
//!
//! What Windows itself tells the app window (power, session, clock, device
//! and theme notifications) is translated here, so the shell above handles
//! one vocabulary.
#![expect(
    unsafe_code,
    reason = "PostMessageW, DefWindowProcW and reading a setting name from lParam"
)]
use powerkit_app::event::Event;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        DefWindowProcW, PBT_APMPOWERSTATUSCHANGE, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
        PBT_APMSUSPEND, PostMessageW, WM_APP, WM_DEVICECHANGE, WM_POWERBROADCAST, WM_SETTINGCHANGE,
        WM_TIMECHANGE, WM_WTSSESSION_CHANGE, WTS_CONSOLE_CONNECT, WTS_CONSOLE_DISCONNECT,
        WTS_REMOTE_CONNECT, WTS_REMOTE_DISCONNECT, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
    },
};
/// wParam is the event's position in `Event::ALL`.
pub const EVENT: u32 = WM_APP + 16;

/// Raises `event` for the app window `app`. A null or destroyed window makes
/// the post fail, which loses nothing: the handler would have had nobody to
/// tell.
pub fn post(app: HWND, event: Event) {
    let Some(code) = Event::ALL.iter().position(|item| *item == event) else {
        return;
    };
    // SAFETY: PostMessageW is thread-safe and takes no pointer here.
    unsafe {
        PostMessageW(app, EVENT, code, 0);
    }
}
/// The event a posted wParam names.
pub fn from_wparam(code: usize) -> Option<Event> {
    Event::ALL.get(code).copied()
}

/// The event a window message of the app window stands for; `None` for a
/// message that is not an operating system notification PowerKit uses.
///
/// `l` is read only for `WM_SETTINGCHANGE`, where Windows passes the name of
/// the setting as a NUL-terminated string that lives for the duration of the
/// handler; call this from the window procedure, with its own arguments.
pub fn from_message(msg: u32, w: WPARAM, l: LPARAM) -> Option<Event> {
    match msg {
        WM_POWERBROADCAST => match u32::try_from(w).ok()? {
            PBT_APMSUSPEND => Some(Event::Suspending),
            PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => Some(Event::Resumed),
            PBT_APMPOWERSTATUSCHANGE => Some(Event::PowerSourceChanged),
            _ => None,
        },
        WM_WTSSESSION_CHANGE => match u32::try_from(w).ok()? {
            WTS_SESSION_LOCK => Some(Event::SessionLocked),
            WTS_SESSION_UNLOCK => Some(Event::SessionUnlocked),
            WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => Some(Event::SessionDisconnected),
            WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT => Some(Event::SessionConnected),
            _ => None,
        },
        WM_TIMECHANGE => Some(Event::ClockChanged),
        // Broadcast to every top-level window when a device arrives or leaves.
        WM_DEVICECHANGE => Some(Event::HardwareChanged),
        WM_SETTINGCHANGE => {
            crate::icons::theme_setting(setting_name(l).as_deref()).then_some(Event::ThemeChanged)
        }
        _ => None,
    }
}
/// What the window procedure returns for a message [`from_message`] took,
/// after the event was handled: power broadcasts are granted, and the two
/// broadcasts other code may also want are passed on.
pub fn reply(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_POWERBROADCAST => 1,
        WM_SETTINGCHANGE | WM_DEVICECHANGE => {
            // SAFETY: the window procedure's own arguments, on its thread.
            unsafe { DefWindowProcW(hwnd, msg, w, l) }
        }
        _ => 0,
    }
}
fn setting_name(l: LPARAM) -> Option<String> {
    if l == 0 {
        return None;
    }
    let ptr = l as *const u16;
    let mut len = 0;
    // SAFETY: lParam is a NUL-terminated setting name for the duration of the handler.
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 256 {
                return None;
            }
        }
        String::from_utf16(std::slice::from_raw_parts(ptr, len)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_notifications_become_events_and_the_rest_is_left_alone() {
        let power = |w: u32| from_message(WM_POWERBROADCAST, w as usize, 0);
        assert_eq!(power(PBT_APMSUSPEND), Some(Event::Suspending));
        assert_eq!(power(PBT_APMRESUMEAUTOMATIC), Some(Event::Resumed));
        assert_eq!(power(PBT_APMRESUMESUSPEND), Some(Event::Resumed));
        assert_eq!(
            power(PBT_APMPOWERSTATUSCHANGE),
            Some(Event::PowerSourceChanged)
        );
        assert_eq!(power(0x8013), None, "a power setting change is not used");
        let session = |w: u32| from_message(WM_WTSSESSION_CHANGE, w as usize, 0);
        assert_eq!(session(WTS_SESSION_LOCK), Some(Event::SessionLocked));
        assert_eq!(session(WTS_SESSION_UNLOCK), Some(Event::SessionUnlocked));
        assert_eq!(
            session(WTS_REMOTE_DISCONNECT),
            Some(Event::SessionDisconnected)
        );
        assert_eq!(session(WTS_CONSOLE_CONNECT), Some(Event::SessionConnected));
        assert_eq!(session(0x5), None, "a logon is not used");
        assert_eq!(from_message(WM_TIMECHANGE, 0, 0), Some(Event::ClockChanged));
        assert_eq!(
            from_message(WM_DEVICECHANGE, 7, 0),
            Some(Event::HardwareChanged)
        );
        assert_eq!(from_message(WM_APP, 0, 0), None);
    }

    #[test]
    fn only_the_color_set_setting_is_a_theme_change() {
        let name = |text: &str| -> Vec<u16> { text.encode_utf16().chain([0]).collect() };
        let colors = name("ImmersiveColorSet");
        let other = name("Policy");
        assert_eq!(
            from_message(WM_SETTINGCHANGE, 0, colors.as_ptr() as LPARAM),
            Some(Event::ThemeChanged)
        );
        assert_eq!(
            from_message(WM_SETTINGCHANGE, 0, other.as_ptr() as LPARAM),
            None
        );
        assert_eq!(from_message(WM_SETTINGCHANGE, 0, 0), None);
    }

    #[test]
    fn every_event_has_a_code_and_unknown_codes_are_ignored() {
        for (code, event) in Event::ALL.into_iter().enumerate() {
            assert_eq!(from_wparam(code), Some(event));
        }
        assert_eq!(from_wparam(Event::ALL.len()), None);
    }
}
