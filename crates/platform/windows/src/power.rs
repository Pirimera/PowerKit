//! UI-thread power request ownership and cleanup.
//!
//! `SetThreadExecutionState` is the request Windows honours for this thread. A
//! named power request mirrors it so `powercfg /requests` shows why PowerKit
//! keeps the machine awake; failures of that mirror are not fatal.
use powerkit_core::always_on::{Mode, PowerSource};
use std::{cell::RefCell, io};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Power::*,
        Threading::{POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT},
    },
};

const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;
const REASON: &str = "PowerKit AlwaysOn";

#[derive(Default)]
struct Named {
    handle: Option<HANDLE>,
    system: bool,
    display: bool,
}
thread_local! {
    static NAMED: RefCell<Named> = RefCell::new(Named::default());
}

pub fn power(mode: Mode, display: bool) -> io::Result<()> {
    let system = mode.active();
    let display = system && display;
    let mut flags = ES_CONTINUOUS;
    if system {
        flags |= ES_SYSTEM_REQUIRED;
        if display {
            flags |= ES_DISPLAY_REQUIRED;
        }
    }
    // SAFETY: documented flag combination, called only on the owning UI thread.
    if unsafe { SetThreadExecutionState(flags) } == 0 {
        return Err(io::Error::other("SetThreadExecutionState failed"));
    }
    mirror(system, display);
    Ok(())
}
fn create_named() -> Option<HANDLE> {
    let mut reason: Vec<u16> = REASON.encode_utf16().chain(Some(0)).collect();
    let mut context = REASON_CONTEXT {
        Version: POWER_REQUEST_CONTEXT_VERSION,
        Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
        ..Default::default()
    };
    context.Reason.SimpleReasonString = reason.as_mut_ptr();
    // SAFETY: the context and its NUL-terminated string outlive the call; Windows
    // copies the reason. The returned handle is owned by NAMED on this thread.
    let handle = unsafe { PowerCreateRequest(&context) };
    (!handle.is_null() && handle != INVALID_HANDLE_VALUE).then_some(handle)
}
/// Sets or clears each request type at most once; requests are reference counted.
fn mirror(system: bool, display: bool) {
    NAMED.with(|named| {
        let mut named = named.borrow_mut();
        if named.handle.is_none() && (system || display) {
            named.handle = create_named();
        }
        let Named {
            handle: Some(handle),
            system: held_system,
            display: held_display,
        } = &mut *named
        else {
            return;
        };
        let handle = *handle;
        for (want, kind, held) in [
            (system, PowerRequestSystemRequired, held_system),
            (display, PowerRequestDisplayRequired, held_display),
        ] {
            if want == *held {
                continue;
            }
            // SAFETY: handle is the live request created on this thread.
            let ok = unsafe {
                if want {
                    PowerSetRequest(handle, kind)
                } else {
                    PowerClearRequest(handle, kind)
                }
            } != 0;
            if ok {
                *held = want;
            }
        }
    });
}
fn close_named() {
    NAMED.with(|named| {
        let mut named = named.borrow_mut();
        if let Some(handle) = named.handle.take() {
            // SAFETY: closing the request this thread created also drops any set types.
            unsafe { CloseHandle(handle) };
        }
        named.system = false;
        named.display = false;
    });
}
/// AC line and battery level; unknown AC status counts as AC so it never stops a session.
pub fn source() -> PowerSource {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: writes one struct into a live local.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return PowerSource {
            on_ac: true,
            battery: None,
        };
    }
    const NO_BATTERY: u8 = 128;
    const UNKNOWN: u8 = 255;
    PowerSource {
        on_ac: status.ACLineStatus != 0,
        battery: (status.BatteryFlag & NO_BATTERY == 0
            && status.BatteryFlag != UNKNOWN
            && status.BatteryLifePercent <= 100)
            .then_some(status.BatteryLifePercent),
    }
}
// Rc's marker makes the guard !Send and !Sync: cleanup must stay on its creator's thread.
#[derive(Default)]
pub struct PowerGuard(std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for PowerGuard {
    fn drop(&mut self) {
        let _ = power(Mode::Off, false);
        close_named();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_power_flags_toggle_and_release_on_owning_thread() {
        // Native ARM64 integration: Windows returns the previous flags for this thread.
        let _guard = PowerGuard::default();
        power(Mode::Off, false).unwrap();
        power(Mode::Indefinite, false).unwrap();
        let previous = unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
        };
        assert_ne!(previous & ES_SYSTEM_REQUIRED, 0);
        assert_eq!(previous & ES_DISPLAY_REQUIRED, 0);
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        assert_ne!(previous & ES_SYSTEM_REQUIRED, 0);
        assert_ne!(previous & ES_DISPLAY_REQUIRED, 0);
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        assert_eq!(previous & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED), 0);
    }
    #[test]
    fn guard_clears_request_on_scope_exit() {
        {
            let _guard = PowerGuard::default();
            power(Mode::Indefinite, false).unwrap();
        }
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        assert_eq!(previous & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED), 0);
        NAMED.with(|named| assert!(named.borrow().handle.is_none()));
    }
    #[test]
    fn named_request_follows_the_mode_without_double_counting() {
        let _guard = PowerGuard::default();
        power(Mode::Indefinite, true).unwrap();
        power(Mode::Indefinite, true).unwrap();
        NAMED.with(|named| {
            let named = named.borrow();
            assert!(named.handle.is_some());
            assert!(named.system && named.display);
        });
        power(Mode::Indefinite, false).unwrap();
        NAMED.with(|named| assert!(named.borrow().system && !named.borrow().display));
        power(Mode::Off, true).unwrap();
        NAMED.with(|named| assert!(!named.borrow().system && !named.borrow().display));
    }
    #[test]
    fn power_source_reports_a_plausible_battery() {
        if let Some(percent) = source().battery {
            assert!(percent <= 100);
        }
    }
}
