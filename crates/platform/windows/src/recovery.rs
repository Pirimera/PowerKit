//! Bringing PowerKit back after a crash, so PowerMouse and the other tools
//! return without the user having to start PowerKit again.
//!
//! Windows Error Reporting restarts a process registered with
//! `RegisterApplicationRestart` when it crashes or hangs. This covers native
//! crashes and Rust panics alike: a panic in a window procedure ends in a
//! fail-fast abort, which Windows also restarts (checked on Windows 11
//! ARM64). Windows restarts only a process that ran for at least a minute, so
//! a crash at startup cannot loop, and nothing restarts when error reporting
//! is turned off by policy.
//!
//! The crashed process keeps PowerKit's single-instance name while Windows
//! finishes reporting it, so the new process starts with [`RESTARTED`] and
//! waits for the old one instead of taking it for a running PowerKit.
#![expect(unsafe_code, reason = "RegisterApplicationRestart")]
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Recovery::{
    RESTART_NO_PATCH, RESTART_NO_REBOOT, RegisterApplicationRestart,
};

/// Marks a PowerKit started to replace one that crashed.
pub const RESTARTED: &str = "--restarted";
/// How long a restarted PowerKit waits for the crashed one to be gone.
const WAIT_FOR_PREVIOUS: Duration = Duration::from_secs(30);
const WAIT_STEP: Duration = Duration::from_millis(250);

/// Registers the restart. Call once after startup has succeeded; never for
/// the self-test or a worker process. A normal exit, an update (the Store
/// restarts a packaged app itself) and a reboot (the startup task covers
/// that) do not restart PowerKit.
pub fn arm() {
    let command: Vec<u16> = RESTARTED.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: the command line is null-terminated and is copied by the call.
    unsafe {
        RegisterApplicationRestart(command.as_ptr(), RESTART_NO_PATCH | RESTART_NO_REBOOT);
    }
}

pub fn is_restart(args: &[String]) -> bool {
    args.iter().any(|arg| arg == RESTARTED)
}

/// Waits until `previous_running` reports the crashed PowerKit gone, polling
/// a few times a second. `false` when it is still there after the wait: then
/// another PowerKit really is running and this one should step aside.
pub fn wait_for_previous(previous_running: impl FnMut() -> bool) -> bool {
    wait_until_gone(previous_running, WAIT_FOR_PREVIOUS, WAIT_STEP)
}

fn wait_until_gone(mut running: impl FnMut() -> bool, limit: Duration, step: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if !running() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_restart_flag_is_recognized_among_other_arguments() {
        let args = |list: &[&str]| {
            list.iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>()
        };
        assert!(is_restart(&args(&["powerkit.exe", "--restarted"])));
        assert!(!is_restart(&args(&["powerkit.exe"])));
        assert!(!is_restart(&args(&[
            "powerkit.exe",
            "--resize",
            "--restarted.jpg"
        ])));
    }

    #[test]
    fn waiting_ends_when_the_previous_instance_is_gone_or_time_runs_out() {
        let mut polls = 0;
        assert!(wait_until_gone(
            || {
                polls += 1;
                polls < 3
            },
            Duration::from_secs(5),
            Duration::from_millis(1),
        ));
        assert_eq!(polls, 3);
        assert!(!wait_until_gone(
            || true,
            Duration::from_millis(20),
            Duration::from_millis(5)
        ));
    }
}
