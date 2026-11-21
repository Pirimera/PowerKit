#![cfg_attr(windows, windows_subsystem = "windows")]
mod always_on;
mod always_top;
mod changelog;
mod hotkey;
mod image_resizer;
mod json;
mod locale;
mod media_extensions;
mod media_renamer;
#[cfg(windows)]
mod platform;
mod power_mouse;
mod power_store;
mod screen_ruler;
mod settings;
mod tray_layout;
#[cfg(windows)]
use platform::windows;
fn main() {
    #[cfg(windows)]
    if let Some(code) = windows::run_power_store_worker() {
        std::process::exit(code);
    }
    #[cfg(windows)]
    if let Err(error) = windows::run() {
        if !std::env::args().any(|a| a == "--smoke-test") {
            windows::report_error(&error);
        }
        eprintln!("{error}");
        if let Some(path) = std::env::var_os("POWERKIT_DIAGNOSTIC_FILE") {
            let _ = std::fs::write(path, error.to_string());
        }
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    eprintln!("PowerKit requires Windows 11.");
}
