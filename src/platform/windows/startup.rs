//! Optional per-user Run key so PowerKit can start with Windows.
use std::io;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "PowerKit";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Writes or clears `HKCU\...\Run\PowerKit` for the current executable.
pub(super) fn set_enabled(on: bool) -> io::Result<()> {
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
