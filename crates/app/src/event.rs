//! What happens outside the UI that the application reacts to.
//!
//! An intent comes from the user; an [`Event`] comes from the platform: its
//! background work (a device list was rescanned, a job made progress) or the
//! operating system (the machine resumed, the session was locked). Worker
//! threads raise events, a platform translates its own notifications into
//! them, and the platform shell delivers both on the thread that owns the
//! application state. An event carries no data: it says where to look, and
//! the handler reads the current state there.
use crate::controller::settings::Tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The connected mice, what their adapters report, or a device render
    /// changed.
    MouseDevicesChanged,
    /// A MediaRenamer scan, preview or rename job has news; poll the session.
    RenamerProgress,
    /// An Image Resizer preview or write job has news for its dialog.
    ResizeProgress,
    /// The machine is about to sleep.
    Suspending,
    /// The machine woke up; timers and deadlines may be stale.
    Resumed,
    /// The machine went on or off battery, or its battery level changed.
    PowerSourceChanged,
    /// The wall clock was set; deadlines kept as wall-clock times moved.
    ClockChanged,
    /// The user locked the session.
    SessionLocked,
    SessionUnlocked,
    /// The session left this seat: a remote or console disconnect.
    SessionDisconnected,
    SessionConnected,
    /// A device arrived or left; which one is not known yet.
    HardwareChanged,
    /// The system colors or the light and dark choice changed.
    ThemeChanged,
}
impl Event {
    pub const ALL: [Self; 13] = [
        Self::MouseDevicesChanged,
        Self::RenamerProgress,
        Self::ResizeProgress,
        Self::Suspending,
        Self::Resumed,
        Self::PowerSourceChanged,
        Self::ClockChanged,
        Self::SessionLocked,
        Self::SessionUnlocked,
        Self::SessionDisconnected,
        Self::SessionConnected,
        Self::HardwareChanged,
        Self::ThemeChanged,
    ];
    /// The tool whose Home switch must be on for the event to matter. A job
    /// that is already running reports whether or not its tool was switched
    /// off meanwhile, and what the operating system reports reaches every
    /// tool.
    pub fn tool(self) -> Option<Tool> {
        match self {
            Self::MouseDevicesChanged => Some(Tool::PowerMouse),
            _ => None,
        }
    }
}
