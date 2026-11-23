//! Per-tool state machines and workflows, and the application controller
//! that owns them.
//!
//! A controller decides; it reaches the operating system only through the
//! traits in [`crate::ports`]. [`application::App`] holds the preferences and
//! the workspace sessions and answers every intent with a step for the
//! platform shell.
pub mod always_on;
pub mod always_top;
pub mod application;
pub mod hotkeys;
pub mod image_resizer;
pub mod media_renamer;
pub mod power_mouse;
pub mod power_store;
pub mod settings;
