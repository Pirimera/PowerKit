//! Start with Windows: a per-user Run key for unpackaged builds, or the manifest
//! `windows.startupTask` when running from the MSIX package.
use std::io;
use windows::{
    ApplicationModel::{StartupTask, StartupTaskState},
    core::HSTRING,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "PowerKit";
/// Must equal `TaskId` in packaging/msix/AppxManifest.xml.
const STARTUP_TASK_ID: &str = "PowerKitStartup";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn set_enabled(on: bool) -> io::Result<()> {
    if super::package::is_packaged() {
        set_startup_task(on)
    } else {
        set_run_key(on)
    }
}

/// StartupTask is WinRT. The calls run on a short-lived MTA thread so the UI
/// thread never blocks its own apartment while waiting for an async result.
fn set_startup_task(on: bool) -> io::Result<()> {
    std::thread::spawn(move || {
        let _apartment = super::shell_package::WinRt::new()?;
        let task = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))
            .and_then(|operation| operation.join())
            .map_err(io::Error::other)?;
        if !on {
            return task.Disable().map_err(io::Error::other);
        }
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
    })
    .join()
    .unwrap_or_else(|_| Err(io::Error::other("startup task thread panicked")))
}

/// Writes or clears `HKCU\...\Run\PowerKit` for the current executable.
fn set_run_key(on: bool) -> io::Result<()> {
    if on {
        let exe = std::env::current_exe()?;
        let value = format!("\"{}\"", exe.display());
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
