//! What a user interface asks the application to do.
//!
//! An [`Intent`] carries everything the request needs, so a UI kit hands over
//! one value and keeps no side channel for the data that goes with it. The
//! platform shell decides how intents travel to the thread that owns the
//! application state; nothing here names a window message or a widget.
use crate::command::Command;
use crate::controller::hotkeys::Slot;
use crate::controller::settings::{Edit, Tool};
use crate::controller::{media_renamer, power_store};
use crate::view::Page;
use powerkit_core::image_resizer::ResizeSettings;
use powerkit_core::power_mouse::{self, hidpp::WheelMode};
use powerkit_core::primitives::{Rgb, UtcTicks};
use powerkit_core::tray_layout::TrayEdit;

#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// A request that needs no value; the tray sends these too.
    Command(Command),
    /// A global shortcut was pressed. It acts on the window in front, where
    /// the same request from the tray acts on the last window used.
    Shortcut(Slot),
    /// Settings-window opacity in percent.
    ShellOpacity(i32),
    FrameColor(Rgb),
    RulerColor(Rgb),
    /// The custom AlwaysOn interval in minutes.
    Interval(u32),
    /// Keep awake until the picked date and time.
    Expiry(ExpiryPick),
    /// The Image Resizer filename pattern as typed; it is validated on arrival.
    ResizeFilename(String),
    PowerStore(power_store::Action),
    MediaRenamer(media_renamer::Action),
    PowerMouse(MouseAction),
    Tray(TrayEdit),
    /// Bring the settings window forward on this page.
    ShowPage(Page),
    /// Keep the choices of the resize dialog for the next resize.
    RememberResize(ResizeSettings),
}
impl Intent {
    /// The tool whose Home switch must be on for the intent to run; `None`
    /// for app-wide requests.
    pub fn tool(&self) -> Option<Tool> {
        match self {
            Self::Command(command) => command.tool(),
            Self::Shortcut(slot) => Some(slot.tool()),
            Self::ShellOpacity(_) | Self::Tray(_) | Self::ShowPage(_) => None,
            Self::FrameColor(_) => Some(Tool::AlwaysTop),
            Self::RulerColor(_) => Some(Tool::ScreenRuler),
            Self::Interval(_) | Self::Expiry(_) => Some(Tool::AlwaysOn),
            Self::ResizeFilename(_) | Self::RememberResize(_) => Some(Tool::ImageResizer),
            Self::PowerStore(_) => Some(Tool::PowerStore),
            Self::MediaRenamer(_) => Some(Tool::MediaRenamer),
            Self::PowerMouse(_) => Some(Tool::PowerMouse),
        }
    }
}
impl From<Command> for Intent {
    fn from(command: Command) -> Self {
        Self::Command(command)
    }
}

/// A date and a time of day picked for an AlwaysOn expiry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpiryPick {
    /// Any instant on the chosen local day; `None` means the next occurrence
    /// of the time.
    pub date: Option<UtcTicks>,
    /// Time since local midnight, in 100 ns ticks.
    pub time_of_day: i64,
}

/// A change requested from the PowerMouse page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MouseAction {
    SetBinding {
        trigger: power_mouse::Trigger,
        binding: power_mouse::Binding,
    },
    ResetDefaults,
    /// Device settings; `None` stops managing the setting and leaves the
    /// device's own value.
    SetDpi(Option<u16>),
    SetWheelMode(Option<WheelMode>),
    SetHiresWheel(bool),
    /// Record a key combination for this slot.
    BeginCustomCapture(MouseSlot),
    /// Record the function key a new binding is for.
    BeginAddFKey,
    RemoveFKey {
        trigger: power_mouse::Trigger,
    },
}

impl MouseAction {
    /// The preference edit the action stands for; `None` for the two that
    /// start recording from the keyboard instead.
    pub fn edit(self) -> Option<Edit> {
        Some(match self {
            Self::SetBinding { trigger, binding } => Edit::MouseBinding { trigger, binding },
            Self::ResetDefaults => Edit::MouseResetDefaults,
            Self::SetDpi(dpi) => Edit::MouseDpi(dpi),
            Self::SetWheelMode(mode) => Edit::MouseWheelMode(mode),
            Self::SetHiresWheel(on) => Edit::MouseHiresWheel(on),
            Self::RemoveFKey { trigger } => Edit::MouseRemoveFunctionKey(trigger),
            Self::BeginCustomCapture(_) | Self::BeginAddFKey => return None,
        })
    }
}

/// One editable action of a PowerMouse binding: a slot of the trigger's
/// binding, or one step of the workflow in that slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseSlot {
    pub trigger: power_mouse::Trigger,
    pub slot: power_mouse::ActionSlot,
    pub step: Option<u8>,
}
impl MouseSlot {
    /// The binding with this slot (or step) replaced by `action`.
    pub fn replace(
        self,
        binding: &power_mouse::Binding,
        action: power_mouse::Action,
    ) -> power_mouse::Binding {
        binding.with_action_at(self.slot, self.step.map(usize::from), action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_gated_by_the_tool_it_belongs_to() {
        let rgb = Rgb::new(1, 2, 3);
        assert_eq!(Intent::FrameColor(rgb).tool(), Some(Tool::AlwaysTop));
        assert_eq!(Intent::RulerColor(rgb).tool(), Some(Tool::ScreenRuler));
        assert_eq!(Intent::Interval(45).tool(), Some(Tool::AlwaysOn));
        assert_eq!(
            Intent::Expiry(ExpiryPick {
                date: None,
                time_of_day: 0
            })
            .tool(),
            Some(Tool::AlwaysOn)
        );
        assert_eq!(
            Intent::ResizeFilename(String::new()).tool(),
            Some(Tool::ImageResizer)
        );
        assert_eq!(
            Intent::PowerMouse(MouseAction::ResetDefaults).tool(),
            Some(Tool::PowerMouse)
        );
        // The window's own look and the tray layout belong to no tool.
        assert_eq!(Intent::ShellOpacity(80).tool(), None);
    }

    #[test]
    fn a_command_keeps_its_own_gate() {
        assert_eq!(Intent::from(Command::Exit).tool(), None);
        assert_eq!(
            Intent::from(Command::TogglePin).tool(),
            Some(Tool::AlwaysTop)
        );
    }
}
