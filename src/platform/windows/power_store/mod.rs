//! PowerStore Windows adapters. Phase 1 exposes WinGet through Packaged COM.
pub mod session;
pub mod winget;

pub use session::{Session, Snapshot, Status};
