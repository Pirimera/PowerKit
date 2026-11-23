//! PowerKit domain logic: tool state machines, planners, parsers, the settings
//! codec and the string catalogs.
//!
//! This crate has no operating system, UI or network dependency and must build
//! and pass its tests on every platform. File access, FFI and UI belong to the
//! crates that depend on it.
#![forbid(unsafe_code)]

pub mod always_on;
pub mod always_top;
pub mod changelog;
pub mod hotkey;
pub mod image_resizer;
pub mod json;
pub mod locale;
pub mod media_extensions;
pub mod media_renamer;
pub mod power_mouse;
pub mod power_store;
pub mod primitives;
pub mod screen_ruler;
pub mod settings;
pub mod tray_layout;
