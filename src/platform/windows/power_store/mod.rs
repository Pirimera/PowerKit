//! PowerStore Windows adapters: WinGet Packaged COM plus CLI managers through phase 3.
pub mod arp;
pub mod broker;
pub mod cargo_mgr;
pub mod catalog;
pub mod chocolatey;
pub mod cli_process;
pub mod dotnet_tool;
pub mod file_dialog;
pub mod icon_db;
pub mod icons;
pub mod manager_config;
pub mod manager_probe;
pub mod npm;
pub mod pip;
pub mod powershell_mgr;
pub mod scoop;
pub mod session;
pub mod store_managed;
pub mod vcpkg;
pub mod winget;

pub use crate::power_store::ManagerKind;
pub use catalog::ManagerPresence;
#[allow(unused_imports)]
pub use catalog::ManagerStatus;
pub use session::{
    Action, DetailsView, DrawerView, InspectorView, Notice, OpResult, ProbeView, Session, Snapshot,
    Status,
};
