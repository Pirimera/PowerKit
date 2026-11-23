//! PowerKit's settings UI in WinUI 3, rendered by windows-reactor.
//!
//! The UI renders a `powerkit_app::view::Snapshot` and answers through the
//! `Host` it is given; it does not know the platform crate that hosts it.
//! `winui_host` is the Win32 side of the windows this UI creates (claiming
//! their handles, hide-on-close, key capture) and reports to the host's app
//! window with the messages it defines.
#![cfg(windows)]

pub mod resize_dialog;
mod settings_ui;
pub mod window;
pub mod winui_host;
