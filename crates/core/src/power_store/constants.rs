//! PowerStore limits, sentinel messages and fixed option lists.

/// PowerKit's own package id is never offered for update or uninstall.
pub const OWN_PACKAGE_ID: &str = "PiriMedya.PowerKit";

/// Sentinel `Status::Unavailable` detail when App Installer is not present.
pub const WINGET_UNAVAILABLE_MISSING: &str = "app-installer-missing";

/// Sentinel when App Installer is present but WinGet Packaged COM is not registered.
pub const WINGET_UNAVAILABLE_TOO_OLD: &str = "app-installer-too-old";

/// Sentinel `Status::Unavailable` detail when no package manager is switched on and present.
pub const NO_MANAGERS_AVAILABLE: &str = "no-managers-available";

/// Sentinel `Status::Failed` detail when no manager answered a Discover search.
pub const NO_SEARCH_ANSWER: &str = "no-search-answer";

/// Sentinel `Status::Failed` detail when the list worker ends without an answer.
pub const LIST_WORKER_STOPPED: &str = "list-worker-stopped";

pub const DEFAULT_CHECK_MINUTES: u32 = 360;

pub const MIN_CHECK_MINUTES: u32 = 15;

pub const MAX_CHECK_MINUTES: u32 = 7 * 24 * 60;

pub const MAX_IGNORE_ENTRIES: usize = 512;

pub const MAX_SKIP_ENTRIES: usize = 512;

pub const MAX_HISTORY_OPERATIONS: usize = 200;

pub const MAX_HISTORY_LOG_BYTES: usize = 4096;

pub const MAX_OPTION_ENTRIES: usize = 512;

pub const MAX_EXECUTABLE_CHARS: usize = 1024;

pub const INSTALL_SCOPES: [&str; 2] = ["user", "machine"];

pub const ARCHITECTURES: [&str; 4] = ["x64", "arm64", "x86", "arm"];

/// Triplets vcpkg ships for Windows desktop (`<arch>-windows[-static[-md]]`).
pub const VCPKG_TRIPLETS: [&str; 9] = [
    "x64-windows",
    "x64-windows-static",
    "x64-windows-static-md",
    "arm64-windows",
    "arm64-windows-static",
    "arm64-windows-static-md",
    "x86-windows",
    "x86-windows-static",
    "x86-windows-static-md",
];
