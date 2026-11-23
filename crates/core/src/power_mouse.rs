//! Pure PowerMouse remapping: triggers, actions, bindings and the press state machine.
//! Platform capture and SendInput live in the Windows adapter.
pub mod device;
pub mod diagram;
pub mod hidpp;
pub mod ring;

use crate::hotkey::Hotkey;
use crate::primitives::KeyCode;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

pub const LONG_PRESS: Duration = Duration::from_millis(500);
pub const TILT_BURST_GAP: Duration = Duration::from_millis(250);
pub const HOLD_STALE: Duration = Duration::from_secs(10);
pub const GESTURE_HOLD_FOR_SWIPE: Duration = Duration::from_millis(160);
/// Travel at 96 DPI that commits a swipe.
pub const GESTURE_SWIPE_THRESHOLD_96: i32 = 50;
/// Cross-axis allowance at 96 DPI; also grows with 35% of the dominant axis.
pub const GESTURE_SWIPE_DEADZONE_96: i32 = 40;
/// Extra travel in the committed volume direction before it fires again.
pub const GESTURE_REPEAT_DISTANCE_96: i32 = 600;
pub const MAX_BINDING_BYTES: usize = 1024;
/// Longest text one action may type.
pub const TEXT_MAX_CHARS: usize = 120;
/// Most steps one workflow may hold.
pub const WORKFLOW_MAX_STEPS: usize = 8;
/// Longest pause between two workflow steps.
pub const DELAY_MAX_MS: u16 = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Trigger {
    Middle,
    Back,
    Forward,
    TiltLeft,
    TiltRight,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    /// Device controls: reported by a vendor adapter, not by Windows, and
    /// only while the adapter has diverted them.
    GestureButton,
    ModeShift,
    HapticPanel,
    /// The thumb wheel turned, reported by a vendor adapter in place of the
    /// horizontal scroll Windows would otherwise see.
    ThumbLeft,
    ThumbRight,
    /// The eight slots of the Actions Ring, from the top clockwise. Choosing
    /// a slot presses its trigger, so a slot holds an action like any other
    /// single-action binding.
    Ring1,
    Ring2,
    Ring3,
    Ring4,
    Ring5,
    Ring6,
    Ring7,
    Ring8,
}

impl Trigger {
    /// The Actions Ring slots in ring order.
    pub const RING: [Self; ring::SLOTS] = [
        Self::Ring1,
        Self::Ring2,
        Self::Ring3,
        Self::Ring4,
        Self::Ring5,
        Self::Ring6,
        Self::Ring7,
        Self::Ring8,
    ];

    pub fn is_ring_slot(self) -> bool {
        Self::RING.contains(&self)
    }

    /// Every trigger in declaration order, so `ALL[t as usize] == t`; the
    /// tests check each trigger's storage key against it.
    #[cfg(test)]
    pub const ALL: [Self; 30] = [
        Self::Middle,
        Self::Back,
        Self::Forward,
        Self::TiltLeft,
        Self::TiltRight,
        Self::F13,
        Self::F14,
        Self::F15,
        Self::F16,
        Self::F17,
        Self::F18,
        Self::F19,
        Self::F20,
        Self::F21,
        Self::F22,
        Self::F23,
        Self::F24,
        Self::GestureButton,
        Self::ModeShift,
        Self::HapticPanel,
        Self::ThumbLeft,
        Self::ThumbRight,
        Self::Ring1,
        Self::Ring2,
        Self::Ring3,
        Self::Ring4,
        Self::Ring5,
        Self::Ring6,
        Self::Ring7,
        Self::Ring8,
    ];

    /// Buttons a vendor adapter delivers, in the order the page lists them.
    pub const DEVICE: [Self; 3] = [Self::GestureButton, Self::ModeShift, Self::HapticPanel];

    /// The thumb-wheel directions a vendor adapter delivers.
    pub const THUMB_WHEEL: [Self; 2] = [Self::ThumbLeft, Self::ThumbRight];

    /// The HID++ control behind a device trigger.
    pub fn control_id(self) -> Option<u16> {
        match self {
            Self::GestureButton => Some(hidpp::control::GESTURE_BUTTON),
            Self::ModeShift => Some(hidpp::control::MODE_SHIFT),
            Self::HapticPanel => Some(hidpp::control::HAPTIC_PANEL),
            _ => None,
        }
    }

    pub fn from_control_id(control: u16) -> Option<Self> {
        Self::DEVICE
            .into_iter()
            .find(|trigger| trigger.control_id() == Some(control))
    }

    pub fn is_device_control(self) -> bool {
        self.control_id().is_some()
    }

    pub fn storage_key(self) -> &'static str {
        match self {
            Self::GestureButton => "gesture_button",
            Self::ModeShift => "mode_shift",
            Self::HapticPanel => "haptic_panel",
            Self::ThumbLeft => "thumb_left",
            Self::ThumbRight => "thumb_right",
            Self::Ring1 => "ring_1",
            Self::Ring2 => "ring_2",
            Self::Ring3 => "ring_3",
            Self::Ring4 => "ring_4",
            Self::Ring5 => "ring_5",
            Self::Ring6 => "ring_6",
            Self::Ring7 => "ring_7",
            Self::Ring8 => "ring_8",
            Self::Middle => "middle",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::TiltLeft => "tilt_left",
            Self::TiltRight => "tilt_right",
            Self::F13 => "f13",
            Self::F14 => "f14",
            Self::F15 => "f15",
            Self::F16 => "f16",
            Self::F17 => "f17",
            Self::F18 => "f18",
            Self::F19 => "f19",
            Self::F20 => "f20",
            Self::F21 => "f21",
            Self::F22 => "f22",
            Self::F23 => "f23",
            Self::F24 => "f24",
        }
    }

    pub fn parse_key(key: &str) -> Option<Self> {
        match key {
            "middle" => Some(Self::Middle),
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "tilt_left" => Some(Self::TiltLeft),
            "tilt_right" => Some(Self::TiltRight),
            "f13" => Some(Self::F13),
            "f14" => Some(Self::F14),
            "f15" => Some(Self::F15),
            "f16" => Some(Self::F16),
            "f17" => Some(Self::F17),
            "f18" => Some(Self::F18),
            "f19" => Some(Self::F19),
            "f20" => Some(Self::F20),
            "f21" => Some(Self::F21),
            "f22" => Some(Self::F22),
            "f23" => Some(Self::F23),
            "f24" => Some(Self::F24),
            "gesture_button" => Some(Self::GestureButton),
            "mode_shift" => Some(Self::ModeShift),
            "haptic_panel" => Some(Self::HapticPanel),
            "thumb_left" => Some(Self::ThumbLeft),
            "thumb_right" => Some(Self::ThumbRight),
            other => Self::RING
                .into_iter()
                .find(|slot| slot.storage_key() == other),
        }
    }

    /// Scroll-like triggers: a stream of events with no release edge, taken
    /// as one press per uninterrupted burst.
    pub fn is_burst(self) -> bool {
        matches!(self, Self::TiltLeft | Self::TiltRight) || self.is_thumb_wheel()
    }

    /// The thumb wheel of a device whose adapter has diverted it.
    pub fn is_thumb_wheel(self) -> bool {
        matches!(self, Self::ThumbLeft | Self::ThumbRight)
    }

    pub fn is_function_key(self) -> bool {
        matches!(
            self,
            Self::F13
                | Self::F14
                | Self::F15
                | Self::F16
                | Self::F17
                | Self::F18
                | Self::F19
                | Self::F20
                | Self::F21
                | Self::F22
                | Self::F23
                | Self::F24
        )
    }

    /// Buttons that can be held while the pointer moves. A device control
    /// reports the movement itself (raw XY) instead of moving the cursor.
    pub fn supports_gesture(self) -> bool {
        matches!(self, Self::Middle | Self::Back | Self::Forward) || self.is_device_control()
    }

    pub fn supports_long_press(self) -> bool {
        self.supports_gesture() || self.is_function_key()
    }

    /// The keyboard key of an F13–F24 trigger; other triggers are not one key.
    pub fn key(self) -> Option<KeyCode> {
        let offset = FUNCTION_KEY_TRIGGERS
            .iter()
            .position(|trigger| *trigger == self)?;
        KeyCode::function(13 + offset as u8)
    }

    /// The trigger for `key`, when it is one of F13–F24.
    pub fn from_key(key: KeyCode) -> Option<Self> {
        let number = key.function_number()?;
        FUNCTION_KEY_TRIGGERS
            .get(usize::from(number.checked_sub(13)?))
            .copied()
    }
}

/// F13 to F24 in key order.
const FUNCTION_KEY_TRIGGERS: [Trigger; 12] = [
    Trigger::F13,
    Trigger::F14,
    Trigger::F15,
    Trigger::F16,
    Trigger::F17,
    Trigger::F18,
    Trigger::F19,
    Trigger::F20,
    Trigger::F21,
    Trigger::F22,
    Trigger::F23,
    Trigger::F24,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    LeftClick,
    RightClick,
    MiddleClick,
    MouseBack,
    MouseForward,
    BrowserBack,
    BrowserForward,
    TaskView,
    PreviousDesktop,
    NextDesktop,
    ShowDesktop,
    Copy,
    Paste,
    Cut,
    Undo,
    Redo,
    SelectAll,
    Find,
    Save,
    NewTab,
    CloseTab,
    ReopenTab,
    NextTab,
    PrevTab,
    ReloadPage,
    PlayPause,
    NextTrack,
    PrevTrack,
    VolumeUp,
    VolumeDown,
    MuteVolume,
    CaptureRegion,
    LockScreen,
    PinLastWindow,
    StartScreenRuler,
    OpenPowerKit,
    /// Shows the Actions Ring at the cursor.
    ActionsRing,
    /// Device actions: carried out by a vendor adapter on its device, and
    /// doing nothing when no such device is connected.
    /// Easy-Switch to the paired computer in this slot, counted from 1.
    SwitchHost(u8),
    /// One short tap from the device's haptic motor.
    HapticPulse,
    /// The next DPI preset the device supports, wrapping around.
    DpiCycle,
    Custom(Hotkey),
    /// Types the text as Unicode input.
    Text(String),
    /// Runs its steps in order. Steps are any action except another workflow.
    Workflow(Vec<Action>),
    /// A pause in milliseconds; valid only as a workflow step.
    Delay(u16),
}

/// Bytes kept as they are in a stored text; everything else is `%XX`, so a
/// text can never contain the grammar's `|`, `>` or line separators.
fn is_plain_text_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
}

fn encode_text(text: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if is_plain_text_byte(byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn decode_text(value: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        char::from(byte)
            .to_digit(16)
            .and_then(|digit| u8::try_from(digit).ok())
    }
    let mut bytes = Vec::with_capacity(value.len());
    let mut rest = value.bytes();
    while let Some(byte) = rest.next() {
        if byte == b'%' {
            let high = hex(rest.next()?)?;
            let low = hex(rest.next()?)?;
            bytes.push(high << 4 | low);
        } else if is_plain_text_byte(byte) {
            bytes.push(byte);
        } else {
            return None;
        }
    }
    String::from_utf8(bytes).ok()
}

/// DPI values offered on the page and stepped through by the DPI action;
/// a device is offered the ones it supports.
pub const DPI_PRESETS: [u16; 8] = [400, 800, 1000, 1200, 1600, 2400, 3200, 4000];

/// The preset after `current` among those the device supports, wrapping to
/// the lowest; `None` when the device supports no preset.
pub fn next_dpi(current: u16, supported: &hidpp::DpiSupport) -> Option<u16> {
    let presets = || {
        DPI_PRESETS
            .into_iter()
            .filter(|dpi| supported.supports(*dpi))
    };
    presets()
        .find(|dpi| *dpi > current)
        .or_else(|| presets().next())
}

impl Action {
    /// Whether running the action shows the Actions Ring, directly or as a
    /// workflow step.
    pub fn opens_ring(&self) -> bool {
        match self {
            Self::ActionsRing => true,
            Self::Workflow(steps) => steps.contains(&Self::ActionsRing),
            _ => false,
        }
    }

    /// The request a vendor adapter carries out for this action, if it is a
    /// device action.
    pub fn device_action(&self) -> Option<device::DeviceAction> {
        match self {
            // Slots are shown from 1; the device counts hosts from 0.
            Self::SwitchHost(slot) => Some(device::DeviceAction::SwitchHost(slot.checked_sub(1)?)),
            Self::HapticPulse => Some(device::DeviceAction::HapticPulse),
            Self::DpiCycle => Some(device::DeviceAction::DpiCycle),
            _ => None,
        }
    }

    /// Whether the action may be stored: text within its limits, and delays
    /// and workflows only where the grammar allows them.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Delay(_) => false,
            Self::Workflow(steps) => {
                steps.len() <= WORKFLOW_MAX_STEPS && steps.iter().all(Self::is_valid_step)
            }
            other => other.is_valid_step(),
        }
    }

    fn is_valid_step(&self) -> bool {
        match self {
            Self::Workflow(_) => false,
            Self::SwitchHost(slot) => (1..=3).contains(slot),
            Self::Delay(ms) => (1..=DELAY_MAX_MS).contains(ms),
            Self::Text(text) => {
                text.chars().count() <= TEXT_MAX_CHARS
                    && text
                        .chars()
                        .all(|c| !c.is_control() || matches!(c, '\n' | '\t'))
            }
            _ => true,
        }
    }

    pub fn storage_token(&self) -> String {
        match self {
            Self::Text(text) => format!("text:{}", encode_text(text)),
            Self::Delay(ms) => format!("wait:{ms}"),
            Self::Workflow(steps) => {
                let steps: Vec<String> = steps.iter().map(Self::storage_token).collect();
                format!("flow:{}", steps.join(">"))
            }
            Self::None => "none".into(),
            Self::LeftClick => "left_click".into(),
            Self::RightClick => "right_click".into(),
            Self::MiddleClick => "middle_click".into(),
            Self::MouseBack => "mouse_back".into(),
            Self::MouseForward => "mouse_forward".into(),
            Self::BrowserBack => "browser_back".into(),
            Self::BrowserForward => "browser_forward".into(),
            Self::TaskView => "task_view".into(),
            Self::PreviousDesktop => "previous_desktop".into(),
            Self::NextDesktop => "next_desktop".into(),
            Self::ShowDesktop => "show_desktop".into(),
            Self::Copy => "copy".into(),
            Self::Paste => "paste".into(),
            Self::Cut => "cut".into(),
            Self::Undo => "undo".into(),
            Self::Redo => "redo".into(),
            Self::SelectAll => "select_all".into(),
            Self::Find => "find".into(),
            Self::Save => "save".into(),
            Self::NewTab => "new_tab".into(),
            Self::CloseTab => "close_tab".into(),
            Self::ReopenTab => "reopen_tab".into(),
            Self::NextTab => "next_tab".into(),
            Self::PrevTab => "prev_tab".into(),
            Self::ReloadPage => "reload_page".into(),
            Self::PlayPause => "play_pause".into(),
            Self::NextTrack => "next_track".into(),
            Self::PrevTrack => "prev_track".into(),
            Self::VolumeUp => "volume_up".into(),
            Self::VolumeDown => "volume_down".into(),
            Self::MuteVolume => "mute_volume".into(),
            Self::CaptureRegion => "capture_region".into(),
            Self::LockScreen => "lock_screen".into(),
            Self::PinLastWindow => "pin_last_window".into(),
            Self::StartScreenRuler => "start_screen_ruler".into(),
            Self::OpenPowerKit => "open_powerkit".into(),
            Self::ActionsRing => "actions_ring".into(),
            Self::SwitchHost(slot) => format!("host_{slot}"),
            Self::HapticPulse => "haptic_pulse".into(),
            Self::DpiCycle => "dpi_cycle".into(),
            Self::Custom(hotkey) => format!("keys:{},{}", hotkey.mods_mask(), hotkey.key.stored()),
        }
    }

    pub fn parse_token(token: &str) -> Option<Self> {
        let Some(steps) = token.strip_prefix("flow:") else {
            return Self::parse_simple(token);
        };
        if steps.is_empty() {
            return Some(Self::Workflow(Vec::new()));
        }
        let steps: Vec<Self> = steps
            .split('>')
            .map(Self::parse_step)
            .collect::<Option<_>>()?;
        (steps.len() <= WORKFLOW_MAX_STEPS).then_some(Self::Workflow(steps))
    }

    fn parse_step(token: &str) -> Option<Self> {
        let Some(ms) = token.strip_prefix("wait:") else {
            return Self::parse_simple(token);
        };
        let step = Self::Delay(ms.parse().ok()?);
        step.is_valid_step().then_some(step)
    }

    fn parse_simple(token: &str) -> Option<Self> {
        if let Some(text) = token.strip_prefix("text:") {
            let action = Self::Text(decode_text(text)?);
            return action.is_valid_step().then_some(action);
        }
        match token {
            "none" => Some(Self::None),
            "left_click" => Some(Self::LeftClick),
            "right_click" => Some(Self::RightClick),
            "middle_click" => Some(Self::MiddleClick),
            "mouse_back" => Some(Self::MouseBack),
            "mouse_forward" => Some(Self::MouseForward),
            "browser_back" => Some(Self::BrowserBack),
            "browser_forward" => Some(Self::BrowserForward),
            "task_view" => Some(Self::TaskView),
            "previous_desktop" => Some(Self::PreviousDesktop),
            "next_desktop" => Some(Self::NextDesktop),
            "show_desktop" => Some(Self::ShowDesktop),
            "copy" => Some(Self::Copy),
            "paste" => Some(Self::Paste),
            "cut" => Some(Self::Cut),
            "undo" => Some(Self::Undo),
            "redo" => Some(Self::Redo),
            "select_all" => Some(Self::SelectAll),
            "find" => Some(Self::Find),
            "save" => Some(Self::Save),
            "new_tab" => Some(Self::NewTab),
            "close_tab" => Some(Self::CloseTab),
            "reopen_tab" => Some(Self::ReopenTab),
            "next_tab" => Some(Self::NextTab),
            "prev_tab" => Some(Self::PrevTab),
            "reload_page" => Some(Self::ReloadPage),
            "play_pause" => Some(Self::PlayPause),
            "next_track" => Some(Self::NextTrack),
            "prev_track" => Some(Self::PrevTrack),
            "volume_up" => Some(Self::VolumeUp),
            "volume_down" => Some(Self::VolumeDown),
            "mute_volume" => Some(Self::MuteVolume),
            "capture_region" => Some(Self::CaptureRegion),
            "lock_screen" => Some(Self::LockScreen),
            "pin_last_window" => Some(Self::PinLastWindow),
            "start_screen_ruler" => Some(Self::StartScreenRuler),
            "open_powerkit" => Some(Self::OpenPowerKit),
            "actions_ring" => Some(Self::ActionsRing),
            "host_1" => Some(Self::SwitchHost(1)),
            "host_2" => Some(Self::SwitchHost(2)),
            "host_3" => Some(Self::SwitchHost(3)),
            "haptic_pulse" => Some(Self::HapticPulse),
            "dpi_cycle" => Some(Self::DpiCycle),
            other => {
                let rest = other.strip_prefix("keys:")?;
                let (mods, key) = rest.split_once(',')?;
                let mods = mods.parse::<u8>().ok()?;
                let key = key.parse::<u16>().ok()?;
                Some(Self::Custom(Hotkey::from_storage(mods, key)?))
            }
        }
    }

    pub fn is_mouse_button(&self) -> bool {
        matches!(
            self,
            Self::LeftClick
                | Self::RightClick
                | Self::MiddleClick
                | Self::MouseBack
                | Self::MouseForward
        )
    }

    pub fn is_volume_step(&self) -> bool {
        matches!(self, Self::VolumeUp | Self::VolumeDown)
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Whether the action is a button or key chord that can stay down while
    /// its trigger is held. The rest (commands, lock) can only fire once.
    pub fn is_holdable(&self) -> bool {
        !matches!(
            self,
            Self::None
                | Self::LockScreen
                | Self::PinLastWindow
                | Self::StartScreenRuler
                | Self::OpenPowerKit
                | Self::ActionsRing
                | Self::SwitchHost(_)
                | Self::HapticPulse
                | Self::DpiCycle
                | Self::Text(_)
                | Self::Workflow(_)
                | Self::Delay(_)
        )
    }

    /// Payload-free actions shown in the settings picker (Custom is recorded separately).
    pub const CATALOG: &[Action] = &[
        Self::None,
        Self::LeftClick,
        Self::RightClick,
        Self::MiddleClick,
        Self::MouseBack,
        Self::MouseForward,
        Self::BrowserBack,
        Self::BrowserForward,
        Self::TaskView,
        Self::PreviousDesktop,
        Self::NextDesktop,
        Self::ShowDesktop,
        Self::Copy,
        Self::Paste,
        Self::Cut,
        Self::Undo,
        Self::Redo,
        Self::SelectAll,
        Self::Find,
        Self::Save,
        Self::NewTab,
        Self::CloseTab,
        Self::ReopenTab,
        Self::NextTab,
        Self::PrevTab,
        Self::ReloadPage,
        Self::PlayPause,
        Self::NextTrack,
        Self::PrevTrack,
        Self::VolumeUp,
        Self::VolumeDown,
        Self::MuteVolume,
        Self::CaptureRegion,
        Self::LockScreen,
        Self::PinLastWindow,
        Self::StartScreenRuler,
        Self::OpenPowerKit,
        Self::ActionsRing,
        Self::SwitchHost(1),
        Self::SwitchHost(2),
        Self::SwitchHost(3),
        Self::HapticPulse,
        Self::DpiCycle,
    ];

    pub fn catalog_index(&self) -> Option<usize> {
        Self::CATALOG.iter().position(|action| action == self)
    }
}

/// Middle / Back / Forward: single, long-press and gesture.
pub const PAGE_TRIGGERS: &[Trigger] = &[Trigger::Middle, Trigger::Back, Trigger::Forward];
/// Wheel tilt: single action only.
pub const TILT_TRIGGERS: &[Trigger] = &[Trigger::TiltLeft, Trigger::TiltRight];

/// Which action slot inside a binding the settings page is editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionSlot {
    Single,
    Short,
    Long,
    Click,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    PassThrough,
    Single(Action),
    /// Hold-to-talk: the action's keys or button go down on press and stay
    /// down until the trigger is released.
    Hold(Action),
    LongPress {
        short: Action,
        long: Action,
    },
    Gesture {
        click: Action,
        up: Action,
        down: Action,
        left: Action,
        right: Action,
    },
}

impl Binding {
    pub fn is_pass_through(&self) -> bool {
        matches!(self, Self::PassThrough)
    }

    pub fn is_gesture(&self) -> bool {
        matches!(self, Self::Gesture { .. })
    }

    pub fn validate_for(&self, trigger: Trigger) -> bool {
        let kind_fits = match self {
            Self::PassThrough => return true,
            Self::Single(_) => true,
            Self::Hold(_) | Self::LongPress { .. } => trigger.supports_long_press(),
            Self::Gesture { .. } => trigger.supports_gesture(),
        };
        // Whatever is accepted must also be writable, or it would be lost on
        // the next save.
        kind_fits
            && self.actions().iter().all(|action| action.is_valid())
            // A ring slot that opened the ring again would trap the user in it.
            && !(trigger.is_ring_slot() && self.actions().iter().any(|action| action.opens_ring()))
            && self.render().is_some()
    }

    fn actions(&self) -> Vec<&Action> {
        match self {
            Self::PassThrough => Vec::new(),
            Self::Single(action) | Self::Hold(action) => vec![action],
            Self::LongPress { short, long } => vec![short, long],
            Self::Gesture {
                click,
                up,
                down,
                left,
                right,
            } => vec![click, up, down, left, right],
        }
    }

    /// Replaces a slot's action, or one step of the workflow in that slot.
    /// An address that does not exist leaves the binding unchanged.
    pub fn with_action_at(&self, slot: ActionSlot, step: Option<usize>, action: Action) -> Self {
        let Some(index) = step else {
            return self.with_slot(slot, action);
        };
        match self.slot(slot) {
            Some(Action::Workflow(steps)) if index < steps.len() => {
                let mut steps = steps.clone();
                steps[index] = action;
                self.with_slot(slot, Action::Workflow(steps))
            }
            _ => self.clone(),
        }
    }

    pub fn slot(&self, slot: ActionSlot) -> Option<&Action> {
        match (self, slot) {
            (Self::Single(action) | Self::Hold(action), ActionSlot::Single) => Some(action),
            (Self::LongPress { short, .. }, ActionSlot::Short) => Some(short),
            (Self::LongPress { long, .. }, ActionSlot::Long) => Some(long),
            (Self::Gesture { click, .. }, ActionSlot::Click) => Some(click),
            (Self::Gesture { up, .. }, ActionSlot::Up) => Some(up),
            (Self::Gesture { down, .. }, ActionSlot::Down) => Some(down),
            (Self::Gesture { left, .. }, ActionSlot::Left) => Some(left),
            (Self::Gesture { right, .. }, ActionSlot::Right) => Some(right),
            _ => None,
        }
    }

    pub fn with_slot(&self, slot: ActionSlot, action: Action) -> Self {
        match (self, slot) {
            (Self::Single(_), ActionSlot::Single) => Self::Single(action),
            (Self::Hold(_), ActionSlot::Single) => Self::Hold(action),
            (Self::LongPress { long, .. }, ActionSlot::Short) => Self::LongPress {
                short: action,
                long: long.clone(),
            },
            (Self::LongPress { short, .. }, ActionSlot::Long) => Self::LongPress {
                short: short.clone(),
                long: action,
            },
            (
                Self::Gesture {
                    up,
                    down,
                    left,
                    right,
                    ..
                },
                ActionSlot::Click,
            ) => Self::Gesture {
                click: action,
                up: up.clone(),
                down: down.clone(),
                left: left.clone(),
                right: right.clone(),
            },
            (
                Self::Gesture {
                    click,
                    down,
                    left,
                    right,
                    ..
                },
                ActionSlot::Up,
            ) => Self::Gesture {
                click: click.clone(),
                up: action,
                down: down.clone(),
                left: left.clone(),
                right: right.clone(),
            },
            (
                Self::Gesture {
                    click,
                    up,
                    left,
                    right,
                    ..
                },
                ActionSlot::Down,
            ) => Self::Gesture {
                click: click.clone(),
                up: up.clone(),
                down: action,
                left: left.clone(),
                right: right.clone(),
            },
            (
                Self::Gesture {
                    click,
                    up,
                    down,
                    right,
                    ..
                },
                ActionSlot::Left,
            ) => Self::Gesture {
                click: click.clone(),
                up: up.clone(),
                down: down.clone(),
                left: action,
                right: right.clone(),
            },
            (
                Self::Gesture {
                    click,
                    up,
                    down,
                    left,
                    ..
                },
                ActionSlot::Right,
            ) => Self::Gesture {
                click: click.clone(),
                up: up.clone(),
                down: down.clone(),
                left: left.clone(),
                right: action,
            },
            _ => self.clone(),
        }
    }

    pub fn render(&self) -> Option<String> {
        let text = match self {
            Self::PassThrough => return None,
            Self::Single(action) => format!("do:{}", action.storage_token()),
            Self::Hold(action) => format!("hold:{}", action.storage_token()),
            Self::LongPress { short, long } => {
                format!("long:{}|{}", short.storage_token(), long.storage_token())
            }
            Self::Gesture {
                click,
                up,
                down,
                left,
                right,
            } => format!(
                "gesture:{}|{}|{}|{}|{}",
                click.storage_token(),
                up.storage_token(),
                down.storage_token(),
                left.storage_token(),
                right.storage_token()
            ),
        };
        (text.len() <= MAX_BINDING_BYTES).then_some(text)
    }

    pub fn parse(value: &str) -> Option<Self> {
        if value.len() > MAX_BINDING_BYTES {
            return None;
        }
        if let Some(action) = value.strip_prefix("do:") {
            return Some(Self::Single(Action::parse_token(action)?));
        }
        if let Some(action) = value.strip_prefix("hold:") {
            return Some(Self::Hold(Action::parse_token(action)?));
        }
        if let Some(rest) = value.strip_prefix("long:") {
            let (short, long) = rest.split_once('|')?;
            return Some(Self::LongPress {
                short: Action::parse_token(short)?,
                long: Action::parse_token(long)?,
            });
        }
        if let Some(rest) = value.strip_prefix("gesture:") {
            let parts: Vec<_> = rest.split('|').collect();
            if parts.len() != 5 {
                return None;
            }
            return Some(Self::Gesture {
                click: Action::parse_token(parts[0])?,
                up: Action::parse_token(parts[1])?,
                down: Action::parse_token(parts[2])?,
                left: Action::parse_token(parts[3])?,
                right: Action::parse_token(parts[4])?,
            });
        }
        None
    }
}

/// One device control to hand to PowerMouse instead of the device's default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diversion {
    pub trigger: Trigger,
    /// Report pointer movement while held instead of moving the cursor.
    pub raw_xy: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub enabled: bool,
    /// Absent triggers are pass-through. Function keys appear only when bound.
    pub bindings: BTreeMap<Trigger, Binding>,
    /// Device settings a vendor adapter applies; `None` leaves the device's
    /// own value alone.
    pub dpi: Option<u16>,
    pub wheel_mode: Option<hidpp::WheelMode>,
    pub hires_wheel: Option<bool>,
}

/// DPI values a setting may hold; what a device accepts is narrower and is
/// checked against the device.
pub const DPI_RANGE: std::ops::RangeInclusive<u16> = 100..=32_000;

impl Preferences {
    pub fn binding(&self, trigger: Trigger) -> Binding {
        self.bindings
            .get(&trigger)
            .cloned()
            .unwrap_or(Binding::PassThrough)
    }

    pub fn set_binding(&mut self, trigger: Trigger, binding: Binding) -> bool {
        if !binding.validate_for(trigger) {
            return false;
        }
        if binding.is_pass_through() {
            self.bindings.remove(&trigger);
        } else {
            self.bindings.insert(trigger, binding);
        }
        true
    }

    /// The device controls a vendor adapter must divert for these bindings:
    /// every bound device trigger, with raw movement only for a gesture
    /// binding, so other bindings leave the cursor free while held.
    pub fn diversions(&self) -> Vec<Diversion> {
        if !self.enabled {
            return Vec::new();
        }
        self.bindings
            .iter()
            .filter(|(trigger, _)| trigger.is_device_control())
            .map(|(trigger, binding)| Diversion {
                trigger: *trigger,
                raw_xy: binding.is_gesture(),
            })
            .collect()
    }

    /// What a vendor adapter keeps set on its device. The thumb wheel is
    /// reported to PowerMouse only while one of its directions is bound, so
    /// an unbound thumb wheel keeps scrolling sideways.
    pub fn device_settings(&self) -> device::DeviceSettings {
        device::DeviceSettings {
            dpi: self.dpi,
            wheel_mode: self.wheel_mode,
            hires_wheel: self.hires_wheel,
            thumb_wheel: self.enabled
                && self.bindings.keys().any(|trigger| trigger.is_thumb_wheel()),
        }
    }

    pub fn needs_keyboard_hook(&self) -> bool {
        self.enabled
            && self
                .bindings
                .keys()
                .any(|trigger| trigger.is_function_key())
    }

    pub fn append_settings(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(out, "mouse_enabled={}", self.enabled);
        if let Some(dpi) = self.dpi {
            let _ = writeln!(out, "mouse_dpi={dpi}");
        }
        if let Some(mode) = self.wheel_mode {
            let _ = writeln!(out, "mouse_wheel_mode={}", mode.key());
        }
        if let Some(hires) = self.hires_wheel {
            let _ = writeln!(out, "mouse_hires_wheel={hires}");
        }
        for (trigger, binding) in &self.bindings {
            if let Some(value) = binding.render() {
                let _ = writeln!(out, "mouse_{}={}", trigger.storage_key(), value);
            }
        }
    }

    pub fn parse_binding_line(key: &str, value: &str) -> Option<(Trigger, Binding)> {
        let trigger = Trigger::parse_key(key.strip_prefix("mouse_")?)?;
        let binding = Binding::parse(value)?;
        binding.validate_for(trigger).then_some((trigger, binding))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    PassThrough,
    Suppress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fire {
    /// Down and up together (or a non-mouse action).
    Instant(Action),
    /// Button or key chord down for as long as the trigger is held (a Single
    /// mouse-button action or a Hold binding); release with [`Fire::MirrorUp`].
    MirrorDown(Action),
    MirrorUp(Action),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Fire(Fire),
    ArmLongPress { trigger: Trigger },
    DisarmLongPress { trigger: Trigger },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub disposition: Disposition,
    pub outputs: Vec<Output>,
}

impl Decision {
    fn pass() -> Self {
        Self {
            disposition: Disposition::PassThrough,
            outputs: Vec::new(),
        }
    }

    fn suppress(outputs: Vec<Output>) -> Self {
        Self {
            disposition: Disposition::Suppress,
            outputs,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Press(Trigger),
    Release(Trigger),
    /// Relative cursor motion while a gesture hold may be active.
    Move {
        dx: i32,
        dy: i32,
    },
    LongPressElapsed(Trigger),
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HoldKind {
    LongPress,
    Gesture,
}

struct Hold {
    trigger: Trigger,
    kind: HoldKind,
    started: Instant,
    long_fired: bool,
    gesture: SwipeAccumulator,
    committed: Option<Action>,
}

impl Hold {
    fn new(trigger: Trigger, kind: HoldKind, now: Instant) -> Self {
        let mut gesture = SwipeAccumulator::default();
        if kind == HoldKind::Gesture {
            gesture.begin(now);
        }
        Self {
            trigger,
            kind,
            started: now,
            long_fired: false,
            gesture,
            committed: None,
        }
    }
}

#[derive(Default)]
struct SwipeAccumulator {
    held_since: Option<Instant>,
    dx: i32,
    dy: i32,
    fired: bool,
    /// Motion since the last fire; used so volume can reverse mid-hold.
    segment_dx: i32,
    segment_dy: i32,
}

impl SwipeAccumulator {
    fn begin(&mut self, now: Instant) {
        *self = Self {
            held_since: Some(now),
            ..Self::default()
        };
    }

    fn classify(dx: i32, dy: i32) -> Option<(i32, i32, GestureDirection)> {
        if dx == 0 && dy == 0 {
            return None;
        }
        let (dominant, cross, direction) = if dx.abs() >= dy.abs() {
            (
                dx,
                dy.abs(),
                if dx > 0 {
                    GestureDirection::Right
                } else {
                    GestureDirection::Left
                },
            )
        } else {
            (
                dy,
                dx.abs(),
                if dy > 0 {
                    GestureDirection::Down
                } else {
                    GestureDirection::Up
                },
            )
        };
        Some((dominant.abs(), cross, direction))
    }

    fn accumulate(
        &mut self,
        dx: i32,
        dy: i32,
        now: Instant,
        threshold: i32,
        deadzone: i32,
        repeat: i32,
    ) -> Option<GestureDirection> {
        let held_since = self.held_since?;
        self.dx = self.dx.saturating_add(dx);
        self.dy = self.dy.saturating_add(dy);
        if now.saturating_duration_since(held_since) < GESTURE_HOLD_FOR_SWIPE {
            return None;
        }
        if !self.fired {
            let (travel, cross, direction) = Self::classify(self.dx, self.dy)?;
            let allowed_cross = deadzone.max((i64::from(travel) * 35 / 100) as i32);
            if cross > allowed_cross || travel < threshold {
                return None;
            }
            self.fired = true;
            self.segment_dx = 0;
            self.segment_dy = 0;
            return Some(direction);
        }
        self.segment_dx = self.segment_dx.saturating_add(dx);
        self.segment_dy = self.segment_dy.saturating_add(dy);
        let (travel, cross, direction) = Self::classify(self.segment_dx, self.segment_dy)?;
        let allowed_cross = deadzone.max((i64::from(travel) * 35 / 100) as i32);
        if cross > allowed_cross || travel < repeat {
            return None;
        }
        self.segment_dx = 0;
        self.segment_dy = 0;
        Some(direction)
    }

    fn end(&mut self) -> bool {
        let was_click = self.held_since.is_some() && !self.fired;
        *self = Self::default();
        was_click
    }
}

/// Last cursor position Windows actually applied. A swallowed move leaves the
/// cursor where it was, so the next hook point is still relative to this
/// anchor; advancing past a swallowed move would cancel the travel out.
#[derive(Debug, Default)]
pub struct CursorAnchor {
    last: Option<(i32, i32)>,
}

impl CursorAnchor {
    pub const fn new() -> Self {
        Self { last: None }
    }

    /// Travel from the anchor to a hook point; `None` until the first advance.
    pub fn delta_to(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let (last_x, last_y) = self.last?;
        Some((x.saturating_sub(last_x), y.saturating_sub(last_y)))
    }

    /// Record a point the cursor really reached (the event was not swallowed).
    pub fn advance(&mut self, x: i32, y: i32) {
        self.last = Some((x, y));
    }
}

/// Pure remapper. The hook thread owns one instance and feeds it events.
pub struct Engine {
    prefs: Preferences,
    /// Elevated foreground: every trigger passes through.
    pass_all: bool,
    /// 1.0 = 96 DPI.
    dpi_scale: f32,
    suppressed: HashSet<Trigger>,
    /// Single-binding mouse actions still waiting for a matching release.
    mirrored: HashMap<Trigger, Action>,
    hold: Option<Hold>,
    tilt: Option<(Trigger, Instant)>,
}

impl Engine {
    pub fn new(prefs: Preferences) -> Self {
        Self {
            prefs,
            pass_all: false,
            dpi_scale: 1.0,
            suppressed: HashSet::new(),
            mirrored: HashMap::new(),
            hold: None,
            tilt: None,
        }
    }

    pub fn replace_preferences(&mut self, prefs: Preferences) -> Decision {
        self.prefs = prefs;
        if !self.prefs.enabled {
            self.cancel()
        } else {
            Decision::pass()
        }
    }

    pub fn set_pass_all(&mut self, pass_all: bool) -> Decision {
        self.pass_all = pass_all;
        if pass_all {
            self.cancel()
        } else {
            Decision::pass()
        }
    }

    pub fn set_dpi_scale(&mut self, scale: f32) {
        self.dpi_scale = scale.max(0.5);
    }

    pub fn handle(&mut self, event: Event, now: Instant) -> Decision {
        if !self.prefs.enabled || self.pass_all {
            return match event {
                Event::Cancel => self.cancel(),
                _ => Decision::pass(),
            };
        }
        match event {
            Event::Press(trigger) => self.press(trigger, now),
            Event::Release(trigger) => self.release(trigger, now),
            Event::Move { dx, dy } => self.motion(dx, dy, now),
            Event::LongPressElapsed(trigger) => self.long_press_elapsed(trigger),
            Event::Cancel => self.cancel(),
        }
    }

    fn binding(&self, trigger: Trigger) -> Binding {
        self.prefs.binding(trigger)
    }

    fn scaled(&self, value_96: i32) -> i32 {
        ((f64::from(value_96) * f64::from(self.dpi_scale)).round() as i32).max(1)
    }

    fn press(&mut self, trigger: Trigger, now: Instant) -> Decision {
        if trigger.is_burst() {
            return self.tilt_burst(trigger, now);
        }
        let binding = self.binding(trigger);
        if binding.is_pass_through() {
            return Decision::pass();
        }
        if trigger.is_function_key() && self.suppressed.contains(&trigger) {
            // Auto-repeat while held.
            return Decision::suppress(Vec::new());
        }
        self.recover_stale_hold(trigger, now);
        if let Some(hold) = &self.hold {
            if hold.kind == HoldKind::Gesture && binding.is_gesture() && hold.trigger != trigger {
                return Decision::pass();
            }
            if hold.trigger != trigger {
                return self.secondary_press(trigger, binding);
            }
        }
        match binding {
            Binding::PassThrough => Decision::pass(),
            Binding::Single(action) => self.begin_single(trigger, action),
            Binding::Hold(action) => self.begin_held_action(trigger, action),
            Binding::LongPress { short: _, long: _ } => self.begin_long_press(trigger, now),
            Binding::Gesture { .. } => self.begin_hold(trigger, HoldKind::Gesture, now),
        }
    }

    fn secondary_press(&mut self, trigger: Trigger, binding: Binding) -> Decision {
        match binding {
            Binding::PassThrough => Decision::pass(),
            Binding::Single(action) => self.begin_single(trigger, action),
            Binding::Hold(action) => self.begin_held_action(trigger, action),
            Binding::LongPress { short, .. } => self.begin_single(trigger, short),
            Binding::Gesture { .. } => Decision::pass(),
        }
    }

    /// Hold-to-talk: keys or button down now, up on release. An action that
    /// cannot stay down fires once.
    fn begin_held_action(&mut self, trigger: Trigger, action: Action) -> Decision {
        self.suppressed.insert(trigger);
        if action.is_noop() {
            return Decision::suppress(Vec::new());
        }
        if action.is_holdable() {
            self.mirrored.insert(trigger, action.clone());
            Decision::suppress(vec![Output::Fire(Fire::MirrorDown(action))])
        } else {
            Decision::suppress(vec![Output::Fire(Fire::Instant(action))])
        }
    }

    fn begin_hold(&mut self, trigger: Trigger, kind: HoldKind, now: Instant) -> Decision {
        self.suppressed.insert(trigger);
        self.hold = Some(Hold::new(trigger, kind, now));
        Decision::suppress(Vec::new())
    }

    fn begin_single(&mut self, trigger: Trigger, action: Action) -> Decision {
        self.suppressed.insert(trigger);
        if action.is_noop() {
            return Decision::suppress(Vec::new());
        }
        if action.is_mouse_button() {
            self.mirrored.insert(trigger, action.clone());
            Decision::suppress(vec![Output::Fire(Fire::MirrorDown(action))])
        } else {
            Decision::suppress(vec![Output::Fire(Fire::Instant(action))])
        }
    }

    fn begin_long_press(&mut self, trigger: Trigger, now: Instant) -> Decision {
        self.suppressed.insert(trigger);
        self.hold = Some(Hold::new(trigger, HoldKind::LongPress, now));
        Decision::suppress(vec![Output::ArmLongPress { trigger }])
    }

    fn recover_stale_hold(&mut self, trigger: Trigger, now: Instant) {
        let Some(hold) = self.hold.take() else {
            return;
        };
        let stale = now.saturating_duration_since(hold.started) >= HOLD_STALE;
        let same = hold.trigger == trigger;
        if same || stale {
            self.suppressed.remove(&hold.trigger);
            return;
        }
        self.hold = Some(hold);
    }

    fn release(&mut self, trigger: Trigger, _now: Instant) -> Decision {
        if trigger.is_burst() {
            return Decision::pass();
        }
        let was_suppressed = self.suppressed.remove(&trigger);
        if !was_suppressed {
            return Decision::pass();
        }
        let mut outputs = Vec::new();
        if let Some(hold) = self.hold.take_if(|hold| hold.trigger == trigger) {
            match hold.kind {
                HoldKind::LongPress => {
                    outputs.push(Output::DisarmLongPress { trigger });
                    if !hold.long_fired {
                        let Binding::LongPress { short, .. } = self.binding(trigger) else {
                            return Decision::suppress(outputs);
                        };
                        push_click(&mut outputs, short);
                    }
                }
                HoldKind::Gesture => {
                    let was_click = {
                        let mut gesture = hold.gesture;
                        gesture.end()
                    };
                    if was_click {
                        let Binding::Gesture { click, .. } = self.binding(trigger) else {
                            return Decision::suppress(outputs);
                        };
                        push_click(&mut outputs, click);
                    }
                }
            }
        } else if let Some(action) = self.mirrored.remove(&trigger) {
            outputs.push(Output::Fire(Fire::MirrorUp(action)));
        }
        Decision::suppress(outputs)
    }

    fn long_press_elapsed(&mut self, trigger: Trigger) -> Decision {
        let Some(hold) = self.hold.as_mut() else {
            return Decision::pass();
        };
        if hold.trigger != trigger || hold.kind != HoldKind::LongPress || hold.long_fired {
            return Decision::pass();
        }
        hold.long_fired = true;
        let Binding::LongPress { long, .. } = self.binding(trigger) else {
            return Decision::suppress(Vec::new());
        };
        let mut outputs = Vec::new();
        push_click(&mut outputs, long);
        Decision::suppress(outputs)
    }

    fn motion(&mut self, dx: i32, dy: i32, now: Instant) -> Decision {
        if !matches!(
            self.hold.as_ref(),
            Some(Hold {
                kind: HoldKind::Gesture,
                ..
            })
        ) {
            return Decision::pass();
        }
        let threshold = self.scaled(GESTURE_SWIPE_THRESHOLD_96);
        let deadzone = self.scaled(GESTURE_SWIPE_DEADZONE_96);
        let repeat = self.scaled(GESTURE_REPEAT_DISTANCE_96);
        let trigger = self.hold.as_ref().expect("gesture hold").trigger;
        let direction = self
            .hold
            .as_mut()
            .expect("gesture hold")
            .gesture
            .accumulate(dx, dy, now, threshold, deadzone, repeat);
        let Some(direction) = direction else {
            return Decision::suppress(Vec::new());
        };
        let Binding::Gesture {
            up,
            down,
            left,
            right,
            ..
        } = self.binding(trigger)
        else {
            return Decision::suppress(Vec::new());
        };
        let action = match direction {
            GestureDirection::Up => up,
            GestureDirection::Down => down,
            GestureDirection::Left => left,
            GestureDirection::Right => right,
        };
        let hold = self.hold.as_mut().expect("gesture hold");
        if let Some(committed) = &hold.committed {
            let allow_volume = committed.is_volume_step() && action.is_volume_step();
            if !allow_volume {
                return Decision::suppress(Vec::new());
            }
        }
        hold.committed = Some(action.clone());
        let mut outputs = Vec::new();
        push_click(&mut outputs, action);
        Decision::suppress(outputs)
    }

    fn tilt_burst(&mut self, trigger: Trigger, now: Instant) -> Decision {
        let Binding::Single(action) = self.binding(trigger) else {
            return Decision::pass();
        };
        // The gap is measured from the previous event, not from the fire, so a
        // continuous scroll stays one burst however long it lasts.
        let continues = self.tilt.is_some_and(|(last, at)| {
            last == trigger && now.saturating_duration_since(at) < TILT_BURST_GAP
        });
        self.tilt = Some((trigger, now));
        if continues || action.is_noop() {
            return Decision::suppress(Vec::new());
        }
        Decision::suppress(vec![Output::Fire(Fire::Instant(action))])
    }

    fn cancel(&mut self) -> Decision {
        let mut outputs = Vec::new();
        if let Some(hold) = self.hold.take()
            && hold.kind == HoldKind::LongPress
        {
            outputs.push(Output::DisarmLongPress {
                trigger: hold.trigger,
            });
        }
        self.suppressed.clear();
        self.tilt = None;
        for (_, action) in self.mirrored.drain() {
            outputs.push(Output::Fire(Fire::MirrorUp(action)));
        }
        Decision::suppress(outputs)
    }
}

fn push_click(outputs: &mut Vec<Output>, action: Action) {
    if action.is_noop() {
        return;
    }
    outputs.push(Output::Fire(Fire::Instant(action)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn engine_with(binding: Binding) -> Engine {
        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert!(prefs.set_binding(Trigger::Middle, binding));
        Engine::new(prefs)
    }

    #[test]
    fn binding_grammar_round_trips_and_rejects_bad_values() {
        let hotkey = Hotkey::from_storage(3, b'C' as u16).unwrap();
        let cases = [
            Binding::Single(Action::Copy),
            Binding::Single(Action::Custom(hotkey)),
            Binding::LongPress {
                short: Action::Copy,
                long: Action::Paste,
            },
            Binding::Gesture {
                click: Action::MuteVolume,
                up: Action::VolumeUp,
                down: Action::VolumeDown,
                left: Action::PrevTab,
                right: Action::NextTab,
            },
        ];
        for binding in cases {
            let text = binding.render().unwrap();
            assert_eq!(Binding::parse(&text), Some(binding));
        }
        assert!(Binding::parse("do:nope").is_none());
        assert!(Binding::parse("long:copy").is_none());
        assert!(Binding::parse("gesture:a|b|c|d").is_none());
        assert!(Binding::parse(&format!("do:{}", "x".repeat(300))).is_none());
        assert!(
            !Binding::Gesture {
                click: Action::None,
                up: Action::None,
                down: Action::None,
                left: Action::None,
                right: Action::None,
            }
            .validate_for(Trigger::TiltLeft)
        );
        assert!(
            !Binding::LongPress {
                short: Action::Copy,
                long: Action::Paste,
            }
            .validate_for(Trigger::TiltRight)
        );
    }

    #[test]
    fn f_key_codes_and_slots_round_trip() {
        for trigger in [
            Trigger::F13,
            Trigger::F14,
            Trigger::F15,
            Trigger::F16,
            Trigger::F17,
            Trigger::F18,
            Trigger::F19,
            Trigger::F20,
            Trigger::F21,
            Trigger::F22,
            Trigger::F23,
            Trigger::F24,
        ] {
            assert_eq!(Trigger::from_key(trigger.key().unwrap()), Some(trigger));
        }
        // The stored key numbers of F13 and F24 are unchanged.
        assert_eq!(Trigger::F13.key().unwrap().stored(), 0x7C);
        assert_eq!(Trigger::F24.key().unwrap().stored(), 0x87);
        assert_eq!(Trigger::Middle.key(), None);
        assert_eq!(Trigger::from_key(KeyCode::function(12).unwrap()), None);
        assert_eq!(Trigger::from_key(KeyCode::T), None);
        let binding = Binding::LongPress {
            short: Action::Copy,
            long: Action::Paste,
        };
        assert_eq!(binding.slot(ActionSlot::Short), Some(&Action::Copy));
        assert_eq!(
            binding.with_slot(ActionSlot::Long, Action::Undo),
            Binding::LongPress {
                short: Action::Copy,
                long: Action::Undo,
            }
        );
    }

    #[test]
    fn preferences_omit_pass_through_and_require_keyboard_only_for_f_keys() {
        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert!(prefs.set_binding(Trigger::Middle, Binding::Single(Action::Copy)));
        assert!(prefs.set_binding(Trigger::F13, Binding::Single(Action::Paste)));
        assert!(prefs.set_binding(Trigger::Back, Binding::PassThrough));
        assert!(!prefs.bindings.contains_key(&Trigger::Back));
        assert!(prefs.needs_keyboard_hook());
        let mut out = String::new();
        prefs.append_settings(&mut out);
        assert!(out.contains("mouse_enabled=true\n"));
        assert!(out.contains("mouse_middle=do:copy\n"));
        assert!(out.contains("mouse_f13=do:paste\n"));
        assert!(!out.contains("mouse_back="));
        let (trigger, binding) =
            Preferences::parse_binding_line("mouse_middle", "do:task_view").unwrap();
        assert_eq!(trigger, Trigger::Middle);
        assert_eq!(binding, Binding::Single(Action::TaskView));
        assert!(
            Preferences::parse_binding_line("mouse_tilt_left", "gesture:none|none|none|none|none")
                .is_none()
        );
    }

    #[test]
    fn single_mirrors_mouse_buttons_and_clicks_other_actions() {
        let mut engine = engine_with(Binding::Single(Action::MiddleClick));
        let now = t0();
        let down = engine.handle(Event::Press(Trigger::Middle), now);
        assert_eq!(down.disposition, Disposition::Suppress);
        assert_eq!(
            down.outputs,
            vec![Output::Fire(Fire::MirrorDown(Action::MiddleClick))]
        );
        let up = engine.handle(Event::Release(Trigger::Middle), now);
        assert_eq!(
            up.outputs,
            vec![Output::Fire(Fire::MirrorUp(Action::MiddleClick))]
        );

        let mut engine = engine_with(Binding::Single(Action::Copy));
        let press = engine.handle(Event::Press(Trigger::Middle), now);
        assert_eq!(
            press.outputs,
            vec![Output::Fire(Fire::Instant(Action::Copy))]
        );
        assert!(
            engine
                .handle(Event::Release(Trigger::Middle), now)
                .outputs
                .is_empty()
        );
    }

    #[test]
    fn long_press_fires_short_or_long_exactly_once() {
        let mut engine = engine_with(Binding::LongPress {
            short: Action::Copy,
            long: Action::Paste,
        });
        let now = t0();
        let press = engine.handle(Event::Press(Trigger::Middle), now);
        assert_eq!(
            press.outputs,
            vec![Output::ArmLongPress {
                trigger: Trigger::Middle
            }]
        );
        let release = engine.handle(
            Event::Release(Trigger::Middle),
            now + Duration::from_millis(100),
        );
        assert!(
            release
                .outputs
                .contains(&Output::Fire(Fire::Instant(Action::Copy)))
        );
        assert!(release.outputs.contains(&Output::DisarmLongPress {
            trigger: Trigger::Middle
        }));

        let mut engine = engine_with(Binding::LongPress {
            short: Action::Copy,
            long: Action::Paste,
        });
        engine.handle(Event::Press(Trigger::Middle), now);
        let elapsed = engine.handle(Event::LongPressElapsed(Trigger::Middle), now);
        assert_eq!(
            elapsed.outputs,
            vec![Output::Fire(Fire::Instant(Action::Paste))]
        );
        let late = engine.handle(Event::Release(Trigger::Middle), now + LONG_PRESS);
        assert!(!late.outputs.iter().any(|o| matches!(o, Output::Fire(_))));
    }

    #[test]
    fn gesture_click_and_swipe_commit_once_except_volume() {
        let mut engine = engine_with(Binding::Gesture {
            click: Action::MuteVolume,
            up: Action::VolumeUp,
            down: Action::VolumeDown,
            left: Action::PrevTab,
            right: Action::NextTab,
        });
        let now = t0();
        engine.handle(Event::Press(Trigger::Middle), now);
        let click = engine.handle(
            Event::Release(Trigger::Middle),
            now + Duration::from_millis(50),
        );
        assert_eq!(
            click.outputs,
            vec![Output::Fire(Fire::Instant(Action::MuteVolume))]
        );

        let mut engine = engine_with(Binding::Gesture {
            click: Action::MuteVolume,
            up: Action::VolumeUp,
            down: Action::VolumeDown,
            left: Action::PrevTab,
            right: Action::NextTab,
        });
        let start = t0();
        engine.handle(Event::Press(Trigger::Middle), start);
        let early = engine.handle(
            Event::Move { dx: 80, dy: 0 },
            start + Duration::from_millis(50),
        );
        assert!(early.outputs.is_empty());
        let swipe = engine.handle(
            Event::Move { dx: 80, dy: 0 },
            start + GESTURE_HOLD_FOR_SWIPE + Duration::from_millis(10),
        );
        assert_eq!(
            swipe.outputs,
            vec![Output::Fire(Fire::Instant(Action::NextTab))]
        );
        let again = engine.handle(
            Event::Move { dx: 80, dy: 0 },
            start + GESTURE_HOLD_FOR_SWIPE + Duration::from_millis(20),
        );
        assert!(again.outputs.is_empty());

        let mut engine = engine_with(Binding::Gesture {
            click: Action::None,
            up: Action::VolumeUp,
            down: Action::VolumeDown,
            left: Action::None,
            right: Action::None,
        });
        engine.set_dpi_scale(1.0);
        let start = t0();
        engine.handle(Event::Press(Trigger::Middle), start);
        let after_hold = start + GESTURE_HOLD_FOR_SWIPE + Duration::from_millis(10);
        assert_eq!(
            engine
                .handle(Event::Move { dx: 0, dy: -60 }, after_hold)
                .outputs,
            vec![Output::Fire(Fire::Instant(Action::VolumeUp))]
        );
        // Cross the repeat distance while still held.
        let repeat = engine.handle(
            Event::Move {
                dx: 0,
                dy: -GESTURE_REPEAT_DISTANCE_96,
            },
            after_hold + Duration::from_millis(20),
        );
        assert_eq!(
            repeat.outputs,
            vec![Output::Fire(Fire::Instant(Action::VolumeUp))]
        );
        let reverse = engine.handle(
            Event::Move {
                dx: 0,
                dy: GESTURE_REPEAT_DISTANCE_96,
            },
            after_hold + Duration::from_millis(40),
        );
        assert_eq!(
            reverse.outputs,
            vec![Output::Fire(Fire::Instant(Action::VolumeDown))]
        );
    }

    #[test]
    fn release_without_suppressed_press_passes_through() {
        let mut engine = engine_with(Binding::Single(Action::Copy));
        let decision = engine.handle(Event::Release(Trigger::Middle), t0());
        assert_eq!(decision, Decision::pass());
    }

    #[test]
    fn tilt_fires_once_per_burst() {
        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert!(prefs.set_binding(Trigger::TiltLeft, Binding::Single(Action::PrevTab)));
        let mut engine = Engine::new(prefs);
        let now = t0();
        assert_eq!(
            engine.handle(Event::Press(Trigger::TiltLeft), now).outputs,
            vec![Output::Fire(Fire::Instant(Action::PrevTab))]
        );
        assert!(
            engine
                .handle(
                    Event::Press(Trigger::TiltLeft),
                    now + Duration::from_millis(50)
                )
                .outputs
                .is_empty()
        );
        assert_eq!(
            engine
                .handle(
                    Event::Press(Trigger::TiltLeft),
                    now + Duration::from_millis(50) + TILT_BURST_GAP
                )
                .outputs,
            vec![Output::Fire(Fire::Instant(Action::PrevTab))]
        );
    }

    #[test]
    fn continuous_horizontal_scroll_is_one_burst() {
        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert!(prefs.set_binding(Trigger::TiltRight, Binding::Single(Action::NextTab)));
        assert!(prefs.set_binding(Trigger::TiltLeft, Binding::Single(Action::PrevTab)));
        let mut engine = Engine::new(prefs);
        let start = t0();
        let fires: usize = (0..20)
            .map(|step| {
                engine
                    .handle(
                        Event::Press(Trigger::TiltRight),
                        start + Duration::from_millis(100 * step),
                    )
                    .outputs
                    .len()
            })
            .sum();
        assert_eq!(fires, 1, "two seconds of events 100 ms apart fire once");
        // Reversing direction starts a new burst without waiting for the gap.
        assert_eq!(
            engine
                .handle(
                    Event::Press(Trigger::TiltLeft),
                    start + Duration::from_millis(1950)
                )
                .outputs,
            vec![Output::Fire(Fire::Instant(Action::PrevTab))]
        );
    }

    #[test]
    fn swallowed_moves_keep_the_anchor_so_gesture_travel_accumulates() {
        let mut engine = engine_with(Binding::Gesture {
            click: Action::None,
            up: Action::None,
            down: Action::None,
            left: Action::None,
            right: Action::NextTab,
        });
        let mut anchor = CursorAnchor::new();
        assert_eq!(anchor.delta_to(500, 500), None);
        let start = t0();
        anchor.advance(500, 500);
        engine.handle(Event::Press(Trigger::Middle), start);
        // The cursor is frozen at (500, 500): every hook point is that
        // position plus one small step, never the running total.
        let mut fired = Vec::new();
        for step in 0..8 {
            let (x, y) = (510, 500);
            let (dx, dy) = anchor.delta_to(x, y).unwrap();
            let decision = engine.handle(
                Event::Move { dx, dy },
                start + GESTURE_HOLD_FOR_SWIPE + Duration::from_millis(10 * (step + 1)),
            );
            if decision.disposition == Disposition::PassThrough {
                anchor.advance(x, y);
            }
            fired.extend(decision.outputs);
        }
        assert_eq!(
            fired,
            vec![Output::Fire(Fire::Instant(Action::NextTab))],
            "eight 10 px steps cross the 50 px threshold exactly once"
        );
        assert_eq!(anchor.delta_to(510, 500), Some((10, 0)));
    }

    #[test]
    fn function_key_repeat_is_ignored_and_cancel_releases_mirrors() {
        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert!(prefs.set_binding(Trigger::F13, Binding::Single(Action::LeftClick)));
        let mut engine = Engine::new(prefs);
        let now = t0();
        assert_eq!(
            engine.handle(Event::Press(Trigger::F13), now).outputs,
            vec![Output::Fire(Fire::MirrorDown(Action::LeftClick))]
        );
        assert!(
            engine
                .handle(Event::Press(Trigger::F13), now + Duration::from_millis(30))
                .outputs
                .is_empty()
        );
        let cancel = engine.handle(Event::Cancel, now);
        assert_eq!(
            cancel.outputs,
            vec![Output::Fire(Fire::MirrorUp(Action::LeftClick))]
        );
    }

    #[test]
    fn disabled_and_pass_all_leave_input_alone() {
        let mut engine = engine_with(Binding::Single(Action::Copy));
        let mut off = Preferences::default();
        assert!(engine.replace_preferences(off.clone()).outputs.is_empty());
        assert_eq!(
            engine.handle(Event::Press(Trigger::Middle), t0()),
            Decision::pass()
        );
        off.enabled = true;
        assert!(off.set_binding(Trigger::Middle, Binding::Single(Action::Copy)));
        engine.replace_preferences(off);
        engine.set_pass_all(true);
        assert_eq!(
            engine.handle(Event::Press(Trigger::Middle), t0()),
            Decision::pass()
        );
    }

    #[test]
    fn gesture_trigger_during_hold_is_refused_while_single_still_fires() {
        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert!(prefs.set_binding(
            Trigger::Middle,
            Binding::Gesture {
                click: Action::None,
                up: Action::VolumeUp,
                down: Action::VolumeDown,
                left: Action::None,
                right: Action::None,
            },
        ));
        assert!(prefs.set_binding(Trigger::Back, Binding::Single(Action::Copy)));
        assert!(prefs.set_binding(
            Trigger::Forward,
            Binding::Gesture {
                click: Action::MuteVolume,
                up: Action::None,
                down: Action::None,
                left: Action::None,
                right: Action::None,
            },
        ));
        let mut engine = Engine::new(prefs);
        let now = t0();
        engine.handle(Event::Press(Trigger::Middle), now);
        assert_eq!(
            engine.handle(Event::Press(Trigger::Forward), now),
            Decision::pass()
        );
        assert_eq!(
            engine.handle(Event::Press(Trigger::Back), now).outputs,
            vec![Output::Fire(Fire::Instant(Action::Copy))]
        );
    }

    #[test]
    fn text_and_workflow_actions_round_trip_and_reject_what_cannot_be_stored() {
        let hotkey = Hotkey::from_storage(3, b'C' as u16).unwrap();
        let text = Action::Text("Merhaba, dünya! a|b>c=d\n".into());
        let token = text.storage_token();
        assert!(
            token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":%-_.".contains(&b)),
            "{token}"
        );
        assert_eq!(Action::parse_token(&token), Some(text.clone()));

        let flow = Action::Workflow(vec![
            Action::Custom(hotkey),
            Action::Delay(250),
            text,
            Action::Paste,
        ]);
        let binding = Binding::LongPress {
            short: Action::Copy,
            long: flow.clone(),
        };
        let stored = binding.render().unwrap();
        assert!(stored.starts_with("long:copy|flow:keys:3,67>wait:250>text:"));
        assert_eq!(Binding::parse(&stored), Some(binding.clone()));
        assert!(binding.validate_for(Trigger::Middle));
        assert_eq!(Action::parse_token("flow:"), Some(Action::Workflow(vec![])));

        // A pause is a workflow step only, and workflows do not nest.
        assert!(Action::parse_token("wait:100").is_none());
        assert!(Action::parse_token("flow:copy>flow:paste").is_none());
        assert!(Action::parse_token("flow:wait:0").is_none());
        assert!(Action::parse_token("flow:wait:6000").is_none());
        assert!(Action::parse_token("text:a b").is_none());
        assert!(Action::parse_token("text:%4").is_none());
        assert!(Action::parse_token("text:%FF").is_none());
        assert!(!Binding::Single(Action::Delay(100)).validate_for(Trigger::Middle));
        assert!(
            !Binding::Single(Action::Workflow(vec![Action::Workflow(vec![])]))
                .validate_for(Trigger::Middle)
        );
        assert!(
            !Binding::Single(Action::Workflow(vec![Action::Copy; WORKFLOW_MAX_STEPS + 1]))
                .validate_for(Trigger::Middle)
        );
        assert!(
            !Binding::Single(Action::Text("x".repeat(TEXT_MAX_CHARS + 1)))
                .validate_for(Trigger::Middle)
        );
        assert!(!Binding::Single(Action::Text("bell\u{7}".into())).validate_for(Trigger::Middle));

        // A binding too long to store is refused instead of vanishing on save.
        let long_text = Action::Text("ş".repeat(TEXT_MAX_CHARS));
        let too_long = Binding::Single(Action::Workflow(vec![long_text.clone(), long_text]));
        assert!(too_long.render().is_none());
        let mut prefs = Preferences::default();
        assert!(!prefs.set_binding(Trigger::Middle, too_long));
        assert!(prefs.bindings.is_empty());
    }

    #[test]
    fn workflow_steps_are_addressed_inside_their_slot() {
        let binding = Binding::Single(Action::Workflow(vec![Action::Copy, Action::None]));
        assert_eq!(
            binding.with_action_at(ActionSlot::Single, Some(1), Action::Paste),
            Binding::Single(Action::Workflow(vec![Action::Copy, Action::Paste]))
        );
        assert_eq!(
            binding.with_action_at(ActionSlot::Single, None, Action::Undo),
            Binding::Single(Action::Undo)
        );
        // A step that does not exist, or a slot without a workflow: unchanged.
        assert_eq!(
            binding.with_action_at(ActionSlot::Single, Some(2), Action::Paste),
            binding
        );
        let plain = Binding::Single(Action::Copy);
        assert_eq!(
            plain.with_action_at(ActionSlot::Single, Some(0), Action::Paste),
            plain
        );
    }

    #[test]
    fn device_triggers_bind_like_buttons_and_list_what_to_divert() {
        for (index, trigger) in Trigger::ALL.into_iter().enumerate() {
            assert_eq!(trigger as usize, index);
            assert_eq!(Trigger::parse_key(trigger.storage_key()), Some(trigger));
        }
        for trigger in Trigger::DEVICE {
            let control = trigger.control_id().unwrap();
            assert_eq!(Trigger::from_control_id(control), Some(trigger));
            assert!(trigger.supports_gesture() && trigger.supports_long_press());
        }
        assert_eq!(Trigger::Middle.control_id(), None);
        assert_eq!(Trigger::from_control_id(0x0050), None);

        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert!(prefs.set_binding(Trigger::Middle, Binding::Single(Action::Copy)));
        assert!(prefs.set_binding(Trigger::HapticPanel, Binding::Hold(Action::Copy)));
        assert!(prefs.set_binding(
            Trigger::GestureButton,
            Binding::Gesture {
                click: Action::TaskView,
                up: Action::None,
                down: Action::None,
                left: Action::PreviousDesktop,
                right: Action::NextDesktop,
            },
        ));
        assert_eq!(
            prefs.diversions(),
            vec![
                Diversion {
                    trigger: Trigger::GestureButton,
                    raw_xy: true,
                },
                Diversion {
                    trigger: Trigger::HapticPanel,
                    raw_xy: false,
                },
            ]
        );
        prefs.enabled = false;
        assert!(prefs.diversions().is_empty());

        // The device reports the movement, so a swipe needs no cursor travel.
        prefs.enabled = true;
        let mut engine = Engine::new(prefs);
        let start = t0();
        engine.handle(Event::Press(Trigger::GestureButton), start);
        let swipe = engine.handle(
            Event::Move { dx: -80, dy: 5 },
            start + GESTURE_HOLD_FOR_SWIPE + Duration::from_millis(10),
        );
        assert_eq!(
            swipe.outputs,
            vec![Output::Fire(Fire::Instant(Action::PreviousDesktop))]
        );
        assert!(
            engine
                .handle(Event::Release(Trigger::GestureButton), start)
                .outputs
                .is_empty()
        );
    }

    #[test]
    fn thumb_wheel_turns_fire_once_per_burst_and_ask_for_diversion() {
        let mut prefs = Preferences {
            enabled: true,
            dpi: Some(1600),
            ..Preferences::default()
        };
        assert!(!prefs.device_settings().thumb_wheel);
        assert!(prefs.set_binding(Trigger::ThumbRight, Binding::Single(Action::NextTab)));
        // A turn has no release edge, so only a single action fits.
        assert!(!prefs.set_binding(Trigger::ThumbLeft, Binding::Hold(Action::Copy)));
        let settings = prefs.device_settings();
        assert!(settings.thumb_wheel);
        assert_eq!(settings.dpi, Some(1600));
        // The thumb wheel is not a button: nothing to divert as a control.
        assert!(prefs.diversions().is_empty());

        let mut engine = Engine::new(prefs.clone());
        let start = t0();
        let fires: usize = (0..10)
            .map(|step| {
                engine
                    .handle(
                        Event::Press(Trigger::ThumbRight),
                        start + Duration::from_millis(40 * step),
                    )
                    .outputs
                    .len()
            })
            .sum();
        assert_eq!(fires, 1);
        // The unbound direction does nothing, and a release is not an event.
        assert_eq!(
            engine.handle(Event::Press(Trigger::ThumbLeft), start),
            Decision::pass()
        );
        assert_eq!(
            engine.handle(Event::Release(Trigger::ThumbRight), start),
            Decision::pass()
        );

        prefs.enabled = false;
        assert!(!prefs.device_settings().thumb_wheel);
    }

    #[test]
    fn device_actions_store_and_map_to_adapter_requests() {
        use device::DeviceAction;
        for (action, token, request) in [
            (Action::SwitchHost(1), "host_1", DeviceAction::SwitchHost(0)),
            (Action::SwitchHost(3), "host_3", DeviceAction::SwitchHost(2)),
            (
                Action::HapticPulse,
                "haptic_pulse",
                DeviceAction::HapticPulse,
            ),
            (Action::DpiCycle, "dpi_cycle", DeviceAction::DpiCycle),
        ] {
            assert_eq!(action.storage_token(), token);
            assert_eq!(Action::parse_token(token), Some(action.clone()));
            assert_eq!(action.device_action(), Some(request));
            assert!(!action.is_holdable());
            assert!(action.catalog_index().is_some());
        }
        assert_eq!(Action::Copy.device_action(), None);
        assert!(Action::parse_token("host_0").is_none());
        assert!(!Binding::Single(Action::SwitchHost(4)).validate_for(Trigger::Middle));
        // A workflow may end by switching computers.
        assert_eq!(
            Action::parse_token("flow:copy>haptic_pulse>host_2"),
            Some(Action::Workflow(vec![
                Action::Copy,
                Action::HapticPulse,
                Action::SwitchHost(2),
            ]))
        );

        // The DPI action steps through the presets the device supports.
        let mut list = [0u8; hidpp::PAYLOAD_LEN];
        list[1..7].copy_from_slice(&[0x03, 0x20, 0xE0, 0x32, 0x06, 0x40]);
        let support = hidpp::DpiSupport::from_list(&list);
        assert_eq!(next_dpi(800, &support), Some(1000));
        assert_eq!(next_dpi(1000, &support), Some(1200));
        assert_eq!(next_dpi(1250, &support), Some(1600));
        assert_eq!(next_dpi(1600, &support), Some(800), "wraps to the lowest");
        assert_eq!(next_dpi(800, &hidpp::DpiSupport::default()), None);
    }

    #[test]
    fn ring_slots_hold_one_action_and_fire_it_when_chosen() {
        let mut prefs = Preferences {
            enabled: true,
            ..Preferences::default()
        };
        assert_eq!(Trigger::parse_key("ring_8"), Some(Trigger::Ring8));
        assert_eq!(Trigger::parse_key("ring_9"), None);
        assert!(prefs.set_binding(Trigger::Ring1, Binding::Single(Action::Copy)));
        assert!(prefs.set_binding(Trigger::Ring3, Binding::Single(Action::LeftClick)));
        // A slot is chosen, not held, and must not open the ring again.
        assert!(!prefs.set_binding(Trigger::Ring2, Binding::Hold(Action::Copy)));
        assert!(!prefs.set_binding(Trigger::Ring2, Binding::Single(Action::ActionsRing)));
        assert!(!prefs.set_binding(
            Trigger::Ring2,
            Binding::Single(Action::Workflow(vec![Action::Copy, Action::ActionsRing])),
        ));
        // Any other trigger may open it.
        assert!(prefs.set_binding(Trigger::Middle, Binding::Single(Action::ActionsRing)));
        assert_eq!(
            Action::parse_token("actions_ring"),
            Some(Action::ActionsRing)
        );

        let mut engine = Engine::new(prefs);
        let now = t0();
        assert_eq!(
            engine.handle(Event::Press(Trigger::Middle), now).outputs,
            vec![Output::Fire(Fire::Instant(Action::ActionsRing))]
        );
        // Choosing a slot is a press and a release of its trigger.
        assert_eq!(
            engine.handle(Event::Press(Trigger::Ring1), now).outputs,
            vec![Output::Fire(Fire::Instant(Action::Copy))]
        );
        assert!(
            engine
                .handle(Event::Release(Trigger::Ring1), now)
                .outputs
                .is_empty()
        );
        // A mouse button in a slot becomes a full click.
        assert_eq!(
            engine.handle(Event::Press(Trigger::Ring3), now).outputs,
            vec![Output::Fire(Fire::MirrorDown(Action::LeftClick))]
        );
        assert_eq!(
            engine.handle(Event::Release(Trigger::Ring3), now).outputs,
            vec![Output::Fire(Fire::MirrorUp(Action::LeftClick))]
        );
        // An empty slot does nothing.
        assert_eq!(
            engine.handle(Event::Press(Trigger::Ring2), now),
            Decision::pass()
        );
    }

    #[test]
    fn hold_binding_keeps_the_action_down_until_release() {
        let binding = Binding::Hold(Action::Copy);
        assert_eq!(binding.render().as_deref(), Some("hold:copy"));
        assert_eq!(Binding::parse("hold:copy"), Some(binding.clone()));
        assert!(binding.validate_for(Trigger::F13));
        assert!(!binding.validate_for(Trigger::TiltLeft));

        let mut engine = engine_with(binding);
        let now = t0();
        assert_eq!(
            engine.handle(Event::Press(Trigger::Middle), now).outputs,
            vec![Output::Fire(Fire::MirrorDown(Action::Copy))]
        );
        let release = engine.handle(Event::Release(Trigger::Middle), now);
        assert_eq!(release.disposition, Disposition::Suppress);
        assert_eq!(
            release.outputs,
            vec![Output::Fire(Fire::MirrorUp(Action::Copy))]
        );

        // An action that cannot stay down fires once and releases nothing.
        let mut engine = engine_with(Binding::Hold(Action::LockScreen));
        assert_eq!(
            engine.handle(Event::Press(Trigger::Middle), now).outputs,
            vec![Output::Fire(Fire::Instant(Action::LockScreen))]
        );
        assert!(
            engine
                .handle(Event::Release(Trigger::Middle), now)
                .outputs
                .is_empty()
        );
    }

    #[test]
    fn dpi_scale_raises_swipe_threshold() {
        let mut engine = engine_with(Binding::Gesture {
            click: Action::None,
            up: Action::None,
            down: Action::None,
            left: Action::None,
            right: Action::NextTab,
        });
        engine.set_dpi_scale(2.0);
        let start = t0();
        engine.handle(Event::Press(Trigger::Middle), start);
        let after = start + GESTURE_HOLD_FOR_SWIPE + Duration::from_millis(10);
        assert!(
            engine
                .handle(Event::Move { dx: 60, dy: 0 }, after)
                .outputs
                .is_empty(),
            "60 px is below the 100 px threshold at 200% DPI"
        );
        assert_eq!(
            engine.handle(Event::Move { dx: 50, dy: 0 }, after).outputs,
            vec![Output::Fire(Fire::Instant(Action::NextTab))]
        );
    }
}
