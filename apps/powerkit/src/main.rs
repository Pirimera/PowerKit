#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod shell;
fn main() {
    #[cfg(windows)]
    if let Err(error) = shell::run() {
        if !std::env::args().any(|a| a == "--smoke-test") {
            shell::report_error(&error);
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
