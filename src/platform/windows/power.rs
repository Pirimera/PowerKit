//! UI-thread power request ownership and cleanup.
use crate::always_on::Mode;
use std::io;
use windows_sys::Win32::System::Power::*;

pub(super) fn power(mode: Mode, display: bool) -> io::Result<()> {
    let mut flags = ES_CONTINUOUS;
    if mode.active() {
        flags |= ES_SYSTEM_REQUIRED;
        if display {
            flags |= ES_DISPLAY_REQUIRED;
        }
    }
    // SAFETY: documented flag combination, called only on the owning UI thread.
    if unsafe { SetThreadExecutionState(flags) } == 0 {
        Err(io::Error::other("Windows güç isteğini uygulayamadı"))
    } else {
        Ok(())
    }
}
// Rc's marker makes the guard !Send and !Sync: cleanup must stay on its creator's thread.
#[derive(Default)]
pub(super) struct PowerGuard(std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for PowerGuard {
    fn drop(&mut self) {
        let _ = power(Mode::Off, false);
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
    }
}
