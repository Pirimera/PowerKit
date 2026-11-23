//! PowerStore on Windows. The session is `powerkit_app::controller::power_store`; this module is
//! its catalog: WinGet Packaged COM through a worker child, the CLI managers, logos
//! and file dialogs.
pub mod arp;
pub mod broker;
pub mod catalog;
pub mod cli_process;
pub mod file_dialog;
pub mod file_write;
pub mod icon_db;
pub mod icons;
mod port;
pub mod store_managed;
pub mod winget;

pub use catalog::managers;
pub use port::session;
pub use powerkit_app::controller::power_store::{Action, Notice, Session};
