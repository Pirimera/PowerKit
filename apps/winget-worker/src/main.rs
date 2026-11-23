//! PowerStore's WinGet worker.
//!
//! WinGet's COM API cannot be called from the process that hosts WinUI: the
//! Windows App Runtime's dynamic package dependency breaks in-process
//! `Microsoft.Management.Deployment` calls. PowerKit therefore starts this
//! executable for each WinGet request and reads the answer from its standard
//! output. It links no UI crate, and `Microsoft.Management.Deployment.winmd`
//! must sit beside it. The protocol is `powerkit_platform_windows::power_store::broker`.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        // Anything but a worker request is a usage error.
        let code = powerkit_platform_windows::power_store::broker::run_worker(&args).unwrap_or(2);
        std::process::exit(code);
    }
    #[cfg(not(windows))]
    eprintln!("The WinGet worker requires Windows.");
}
