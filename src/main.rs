#![cfg_attr(windows, windows_subsystem = "windows")]
mod always_on;
mod settings;
#[cfg(windows)]
mod windows;
fn main() {
    #[cfg(windows)]
    if let Err(error) = windows::run() {
        if !std::env::args().any(|a| a == "--smoke-test") {
            windows::report(&error.to_string());
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
