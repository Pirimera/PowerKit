//! PowerKit's operating system ports, implemented with Win32 and WinRT.
//!
//! Each module gives one `powerkit_app` port (or a Windows-only service the
//! application shell needs) its Windows form: power requests, global
//! shortcuts, pinned windows, the screen ruler, the mouse hook and HID++
//! transport, package managers, image and media files, the tray menu, the
//! settings file. Nothing here knows the settings UI; what a module needs
//! from the application shell that hosts it (redrawing the tray and window,
//! a tray notification, the icon of a tool) it asks through [`Shell`].
//!
//! All UI and power calls run on the thread that owns the app window.
#![cfg(windows)]
// Window, menu and icon handles are raw pointer types in windows-sys, but they
// are opaque handles that Windows validates; these functions pass them on and
// never read through them. Functions that do read through a pointer are
// `unsafe` and say what the caller owes.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod awake;
pub mod clipboard;
pub mod clock;
pub mod color;
pub mod command;
pub mod events;
pub mod fault;
pub mod frame;
pub mod hotkeys;
pub mod icons;
pub mod instance;
pub mod intents;
pub mod keys;
pub mod language;
pub mod media_renamer;
pub mod menu;
pub mod menu_hint;
pub mod menu_icon;
pub mod package;
pub mod power;
pub mod power_mouse;
pub mod power_store;
pub mod recovery;
pub mod resize;
pub mod resize_io;
pub mod resize_wic;
pub mod ruler;
pub mod settings_file;
pub mod shell_package;
pub mod startup;
pub mod theme;
pub mod top;

use powerkit_core::locale::Strings;
use powerkit_core::settings::AppLanguage;
use powerkit_core::tray_layout::TrayTool;
use std::{
    io,
    ptr::null_mut,
    sync::{OnceLock, RwLock},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
};

/// Tools Windows cannot offer: none. Another platform lists here what it has
/// no port for, and the UI leaves those tools out.
pub const UNAVAILABLE_TOOLS: &[powerkit_app::controller::settings::Tool] = &[];

/// What a click on the last tray balloon means.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Balloon {
    #[default]
    Info,
    AlwaysOnEnding,
    PowerStoreUpdates,
}

/// What the ports ask of the application shell. The shell installs it once,
/// before it creates the app window.
#[derive(Clone, Copy)]
pub struct Shell {
    /// Rebuild the tray caption and the settings snapshot; `force` skips the
    /// unchanged check.
    pub update_view: fn(hwnd: HWND, force: bool),
    /// Show a tray notification and remember what a click on it means.
    pub tray_balloon: fn(hwnd: HWND, title: &str, body: &str, kind: Balloon),
    /// The Segoe Fluent glyph the settings UI uses for a tool, so the tray
    /// menu shows the same icon.
    pub tool_glyph: fn(tool: TrayTool) -> &'static str,
}
static SHELL: OnceLock<Shell> = OnceLock::new();
static LANGUAGE: RwLock<AppLanguage> = RwLock::new(AppLanguage::System);

/// Installs the shell's callbacks. Later calls are ignored.
pub fn install_shell(shell: Shell) {
    let _ = SHELL.set(shell);
}
fn shell() -> &'static Shell {
    SHELL
        .get()
        .expect("the application shell installs its callbacks first")
}
pub fn update_view(hwnd: HWND, force: bool) {
    (shell().update_view)(hwnd, force);
}
pub fn tray_balloon(hwnd: HWND, title: &str, body: &str, kind: Balloon) {
    (shell().tray_balloon)(hwnd, title, body, kind);
}
pub fn tool_glyph(tool: TrayTool) -> &'static str {
    (shell().tool_glyph)(tool)
}
/// Records the stored language preference; the shell calls this whenever it
/// adopts preferences. Worker threads read it too.
pub fn set_language(preference: AppLanguage) {
    if let Ok(mut language) = LANGUAGE.write() {
        *language = preference;
    }
}
/// The strings of the current UI language.
pub fn ui_strings() -> &'static Strings {
    let preference = LANGUAGE
        .read()
        .map(|language| *language)
        .unwrap_or_default();
    language::resolve(preference).strings()
}
/// A NUL-terminated UTF-16 copy of `s` for a Win32 call.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
/// Shows `error` in the current UI language (see [`fault::describe`]).
pub fn report_error(error: &io::Error) {
    report(&fault::describe(error, ui_strings()));
}
pub fn report(message: &str) {
    // SAFETY: valid temporary UTF-16 strings, no window ownership transferred.
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            wide("PowerKit").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
