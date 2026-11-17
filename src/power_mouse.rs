//! Pure PowerMouse remapping: triggers, actions, bindings and the press state machine.
//! Platform capture and SendInput live in the Windows adapter.
use crate::always_top::Hotkey;
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
pub const MAX_BINDING_BYTES: usize = 256;

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
}

impl Trigger {
    pub fn storage_key(self) -> &'static str {
        match self {
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
            _ => None,
        }
    }

    pub fn is_tilt(self) -> bool {
        matches!(self, Self::TiltLeft | Self::TiltRight)
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

    pub fn supports_gesture(self) -> bool {
        matches!(self, Self::Middle | Self::Back | Self::Forward)
    }

    pub fn supports_long_press(self) -> bool {
        self.supports_gesture() || self.is_function_key()
    }

    /// Virtual-key for F13–F24; other triggers have no single VK.
    pub fn vk(self) -> Option<u16> {
        const VK_F13: u16 = 0x7C;
        Some(match self {
            Self::F13 => VK_F13,
            Self::F14 => VK_F13 + 1,
            Self::F15 => VK_F13 + 2,
            Self::F16 => VK_F13 + 3,
            Self::F17 => VK_F13 + 4,
            Self::F18 => VK_F13 + 5,
            Self::F19 => VK_F13 + 6,
            Self::F20 => VK_F13 + 7,
            Self::F21 => VK_F13 + 8,
            Self::F22 => VK_F13 + 9,
            Self::F23 => VK_F13 + 10,
            Self::F24 => VK_F13 + 11,
            _ => return None,
        })
    }

    pub fn from_vk(vk: u16) -> Option<Self> {
        const VK_F13: u16 = 0x7C;
        const VK_F24: u16 = 0x87;
        if !(VK_F13..=VK_F24).contains(&vk) {
            return None;
        }
        [
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
        ]
        .get(usize::from(vk - VK_F13))
        .copied()
    }
}

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
    Custom(Hotkey),
}

impl Action {
    pub fn storage_token(&self) -> String {
        match self {
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
            Self::Custom(hotkey) => format!("keys:{},{}", hotkey.mods_mask(), hotkey.vk),
        }
    }

    pub fn parse_token(token: &str) -> Option<Self> {
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
            other => {
                let rest = other.strip_prefix("keys:")?;
                let (mods, vk) = rest.split_once(',')?;
                let mods = mods.parse::<u8>().ok()?;
                let vk = vk.parse::<u16>().ok()?;
                Some(Self::Custom(Hotkey::from_storage(mods, vk)?))
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
        match self {
            Self::PassThrough => true,
            Self::Single(_) => true,
            Self::LongPress { .. } => trigger.supports_long_press(),
            Self::Gesture { .. } => trigger.supports_gesture(),
        }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "settings page edits slots via with_slot")
    )]
    pub fn slot(&self, slot: ActionSlot) -> Option<&Action> {
        match (self, slot) {
            (Self::Single(action), ActionSlot::Single) => Some(action),
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub enabled: bool,
    /// Absent triggers are pass-through. Function keys appear only when bound.
    pub bindings: BTreeMap<Trigger, Binding>,
}

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
    /// Mouse-button down for a Single binding; release later with [`Fire::MirrorUp`].
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
        if trigger.is_tilt() {
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
            Binding::LongPress { short: _, long: _ } => self.begin_long_press(trigger, now),
            Binding::Gesture { .. } => self.begin_gesture(trigger, now),
        }
    }

    fn secondary_press(&mut self, trigger: Trigger, binding: Binding) -> Decision {
        match binding {
            Binding::PassThrough => Decision::pass(),
            Binding::Single(action) => self.begin_single(trigger, action),
            Binding::LongPress { short, .. } => self.begin_single(trigger, short),
            Binding::Gesture { .. } => Decision::pass(),
        }
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
        self.hold = Some(Hold {
            trigger,
            kind: HoldKind::LongPress,
            started: now,
            long_fired: false,
            gesture: SwipeAccumulator::default(),
            committed: None,
        });
        Decision::suppress(vec![Output::ArmLongPress { trigger }])
    }

    fn begin_gesture(&mut self, trigger: Trigger, now: Instant) -> Decision {
        self.suppressed.insert(trigger);
        let mut gesture = SwipeAccumulator::default();
        gesture.begin(now);
        self.hold = Some(Hold {
            trigger,
            kind: HoldKind::Gesture,
            started: now,
            long_fired: false,
            gesture,
            committed: None,
        });
        Decision::suppress(Vec::new())
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
        if trigger.is_tilt() {
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
        if let Some((last, at)) = self.tilt
            && last == trigger
            && now.saturating_duration_since(at) < TILT_BURST_GAP
        {
            return Decision::suppress(Vec::new());
        }
        self.tilt = Some((trigger, now));
        if action.is_noop() {
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
    fn f_key_virtual_keys_and_slots_round_trip() {
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
            assert_eq!(Trigger::from_vk(trigger.vk().unwrap()), Some(trigger));
        }
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
                    now + TILT_BURST_GAP + Duration::from_millis(1)
                )
                .outputs,
            vec![Output::Fire(Fire::Instant(Action::PrevTab))]
        );
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
