//! PowerKit application layer.
//!
//! A controller owns one tool's running state and decides what happens; it
//! reaches the operating system only through the traits in [`ports`], which a
//! platform crate implements. Nothing here names an operating system or a UI
//! kit, so every controller is tested with the fakes in [`testkit`] and runs
//! the same on each platform.
#![forbid(unsafe_code)]

pub mod command;
pub mod controller;
pub mod event;
pub mod fault;
pub mod instance;
pub mod intent;
pub mod ports;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod ui;
pub mod view;
