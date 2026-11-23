//! The contract between the application and a user interface.
//!
//! A UI renders a [`Snapshot`] and answers with [`Intent`] values; it reads
//! and writes nothing else. What it may ask of the platform shell that hosts
//! it is [`Host`]. The shell rebuilds the snapshot from the application state
//! after each change and decides how both travel. Nothing here names a UI
//! kit: icons, layout and wording of a page belong to the UI.
//!
//! [`Snapshot`]: crate::view::Snapshot
use crate::controller::image_resizer::Job;
use crate::intent::Intent;
use std::{rc::Rc, sync::Arc};
/// What a UI asks of the platform shell that hosts it.
pub trait Host {
    /// Hands a request to the application. It runs after the UI's own event
    /// handler has returned, in the order requests were sent.
    fn send(&self, intent: Intent);
    /// Runs an Image Resizer preview or write in the background. Progress and
    /// the result arrive in the job's shared state; the platform tells the
    /// dialog when to look.
    fn start_resize(&self, job: Job);
    /// A render of a recognized mouse, if the platform has it ready. The
    /// platform fetches renders in the background and bumps
    /// `Snapshot::mouse_assets_rev` when one arrives, so this never waits.
    fn mouse_image(&self, depot: &'static str, kind: MouseImageKind) -> Option<DeviceImage>;
}

/// Which render angle of a mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseImageKind {
    /// Angled top and side render, drawn behind the button callouts.
    Side,
    /// Straight top-down render, drawn in the compact device header.
    Front,
}

/// An encoded PNG and its pixel size.
#[derive(Clone)]
pub struct DeviceImage {
    pub bytes: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}

/// A UI's handle to its host. Two handles are equal when they name the same
/// host, which lets a UI kit compare component inputs.
#[derive(Clone)]
pub struct HostHandle(Rc<dyn Host>);
impl HostHandle {
    pub fn new(host: impl Host + 'static) -> Self {
        Self(Rc::new(host))
    }
}
impl std::ops::Deref for HostHandle {
    type Target = dyn Host;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl PartialEq for HostHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
