//! Start with Windows: a per-user Run key for unpackaged builds, or the manifest
//! `windows.startupTask` when running from the MSIX package.
use std::io;
use windows::{
    ApplicationModel::{Activation::ActivationKind, AppInstance, StartupTask, StartupTaskState},
    core::HSTRING,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "PowerKit";
/// Must equal `TaskId` in packaging/msix/AppxManifest.xml.
const STARTUP_TASK_ID: &str = "PowerKitStartup";
/// Unpackaged Run-key launches pass this so the settings shell stays hidden.
pub const BACKGROUND: &str = "--background";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// True when this process should stay in the tray: a Windows logon start, a
/// crash restart, or an explicit `--background` (written into the Run key).
pub fn start_in_tray(args: &[String]) -> bool {
    if args.iter().any(|arg| arg == BACKGROUND) {
        return true;
    }
    if super::recovery::is_restart(args) {
        return true;
    }
    packaged_startup_activation()
}

fn packaged_startup_activation() -> bool {
    if !super::package::is_packaged() {
        return false;
    }
    AppInstance::GetActivatedEventArgs()
        .and_then(|args| args.Kind())
        .is_ok_and(|kind| kind == ActivationKind::StartupTask)
}

pub fn set_enabled(on: bool) -> io::Result<()> {
    if super::package::is_packaged() {
        set_startup_task(on)
    } else {
        set_run_key(on)
    }
}

/// Applies the manifest `StartupTask`. Call from the UI thread: Microsoft docs
/// require `RequestEnableAsync` there, and a short-lived MTA helper caused
/// `combase.dll` access violations (`0xc0000005`) when enabling the setting in
/// a packaged build.
fn set_startup_task(on: bool) -> io::Result<()> {
    let task = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))
        .and_then(|operation| operation.join())
        .map_err(io::Error::other)?;
    let state = task.State().map_err(io::Error::other)?;
    if !on {
        return match state {
            StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => {
                task.Disable().map_err(io::Error::other)
            }
            _ => Ok(()),
        };
    }
    match state {
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => Ok(()),
        StartupTaskState::DisabledByUser => Err(io::Error::other(
            "disabled by the user in Task Manager or Settings > Apps > Startup",
        )),
        StartupTaskState::Disabled => {
            let state = task
                .RequestEnableAsync()
                .and_then(|operation| operation.join())
                .map_err(io::Error::other)?;
            match state {
                StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => Ok(()),
                StartupTaskState::DisabledByUser => Err(io::Error::other(
                    "disabled by the user in Task Manager or Settings > Apps > Startup",
                )),
                _ => Err(io::Error::other("startup task is disabled by policy")),
            }
        }
        _ => Err(io::Error::other("startup task is disabled by policy")),
    }
}

/// Writes or clears `HKCU\...\Run\PowerKit` for the current executable.
fn set_run_key(on: bool) -> io::Result<()> {
    if on {
        let exe = std::env::current_exe()?;
        let value = format!("\"{}\" {BACKGROUND}", exe.display());
        let subkey = wide(RUN_KEY);
        let name = wide(VALUE_NAME);
        let mut data = wide(&value);
        // SAFETY: NUL-terminated registry strings live for this call; no handle is kept.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_mut_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
    } else {
        let subkey = wide(RUN_KEY);
        let name = wide(VALUE_NAME);
        // SAFETY: NUL-terminated paths; missing value is not an error for the toggle.
        let status =
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), name.as_ptr()) };
        if status != 0 && status != 2 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_string()).collect()
    }

    #[test]
    fn background_flag_requests_a_tray_start() {
        assert!(start_in_tray(&args(&["powerkit.exe", BACKGROUND])));
        assert!(!start_in_tray(&args(&["powerkit.exe"])));
        assert!(!start_in_tray(&args(&[
            "powerkit.exe",
            "--resize",
            "photo--background.jpg"
        ])));
    }

    #[test]
    fn crash_restart_also_stays_in_the_tray() {
        assert!(start_in_tray(&args(&[
            "powerkit.exe",
            super::super::recovery::RESTARTED
        ])));
    }
}
