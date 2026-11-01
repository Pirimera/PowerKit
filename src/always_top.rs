//! Platform-independent AlwaysTop session. Pins are not persisted across launches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Pin {
    #[default]
    Off,
    On {
        title: String,
    },
}
impl Pin {
    pub fn on(title: impl Into<String>) -> Self {
        Self::On {
            title: sanitize_title(title.into()),
        }
    }
    pub fn active(&self) -> bool {
        matches!(self, Self::On { .. })
    }
    pub fn title(&self) -> Option<&str> {
        match self {
            Self::Off => None,
            Self::On { title } => Some(title),
        }
    }
    pub fn summary(count: usize, last_title: impl Into<String>) -> Self {
        match count {
            0 => Self::Off,
            1 => Self::on(last_title),
            n => Self::on(format!("{n} pencere")),
        }
    }
}
fn sanitize_title(title: String) -> String {
    let title: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    const MAX: usize = 32;
    let mut chars = title.chars();
    let mut clipped: String = chars.by_ref().take(MAX).collect();
    if chars.next().is_some() {
        clipped.push('…');
    }
    if clipped.is_empty() {
        "Pencere".into()
    } else {
        clipped
    }
}
pub const MIN_OPACITY: i32 = 20;
pub const MAX_OPACITY: i32 = 100;
pub const OPACITY_STEP: i32 = 10;
pub const MIN_FRAME_THICKNESS: i32 = 1;
pub const MAX_FRAME_THICKNESS: i32 = 32;
pub const DEFAULT_FRAME_THICKNESS: i32 = 4;
pub const MIN_FRAME_OPACITY: i32 = 10;
pub const MAX_FRAME_OPACITY: i32 = 100;
pub const DEFAULT_FRAME_OPACITY: i32 = 100;
const MOD_WIN_BIT: u8 = 1;
const MOD_CTRL_BIT: u8 = 2;
const MOD_ALT_BIT: u8 = 4;
const MOD_SHIFT_BIT: u8 = 8;
const VK_T: u16 = 0x54;
const VK_M: u16 = 0x4D;
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_MINUS: u16 = 0xBD;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub win: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub vk: u16,
}
impl Hotkey {
    pub const fn pin_default() -> Self {
        Self {
            win: true,
            ctrl: true,
            alt: false,
            shift: false,
            vk: VK_T,
        }
    }
    pub const fn opacity_up_default() -> Self {
        Self {
            win: true,
            ctrl: true,
            alt: false,
            shift: false,
            vk: VK_OEM_PLUS,
        }
    }
    pub const fn opacity_down_default() -> Self {
        Self {
            win: true,
            ctrl: true,
            alt: false,
            shift: false,
            vk: VK_OEM_MINUS,
        }
    }
    pub const fn ruler_default() -> Self {
        Self {
            win: true,
            ctrl: true,
            alt: false,
            shift: true,
            vk: VK_M,
        }
    }
    pub fn new(win: bool, ctrl: bool, alt: bool, shift: bool, vk: u16) -> Option<Self> {
        if vk == 0 || !(win || ctrl || alt || shift) || is_modifier_vk(vk) {
            return None;
        }
        Some(Self {
            win,
            ctrl,
            alt,
            shift,
            vk,
        })
    }
    pub fn from_storage(mods: u8, vk: u16) -> Option<Self> {
        Self::new(
            mods & MOD_WIN_BIT != 0,
            mods & MOD_CTRL_BIT != 0,
            mods & MOD_ALT_BIT != 0,
            mods & MOD_SHIFT_BIT != 0,
            vk,
        )
    }
    pub fn mods_mask(self) -> u8 {
        (u8::from(self.win) * MOD_WIN_BIT)
            | (u8::from(self.ctrl) * MOD_CTRL_BIT)
            | (u8::from(self.alt) * MOD_ALT_BIT)
            | (u8::from(self.shift) * MOD_SHIFT_BIT)
    }
    pub fn win32_mods(self) -> u32 {
        const MOD_ALT: u32 = 0x0001;
        const MOD_CONTROL: u32 = 0x0002;
        const MOD_SHIFT: u32 = 0x0004;
        const MOD_WIN: u32 = 0x0008;
        const MOD_NOREPEAT: u32 = 0x4000;
        let mut mods = MOD_NOREPEAT;
        if self.alt {
            mods |= MOD_ALT;
        }
        if self.ctrl {
            mods |= MOD_CONTROL;
        }
        if self.shift {
            mods |= MOD_SHIFT;
        }
        if self.win {
            mods |= MOD_WIN;
        }
        mods
    }
    pub fn label(self) -> String {
        let mut parts = Vec::new();
        if self.win {
            parts.push("Win");
        }
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(vk_label(self.vk));
        parts.join("+")
    }
}
fn is_modifier_vk(vk: u16) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C)
}
fn vk_label(vk: u16) -> &'static str {
    match vk {
        VK_OEM_PLUS | 0x6B => "+",
        VK_OEM_MINUS | 0x6D => "-",
        0x20 => "Space",
        0x1B => "Esc",
        0x09 => "Tab",
        0x70 => "F1",
        0x71 => "F2",
        0x72 => "F3",
        0x73 => "F4",
        0x74 => "F5",
        0x75 => "F6",
        0x76 => "F7",
        0x77 => "F8",
        0x78 => "F9",
        0x79 => "F10",
        0x7A => "F11",
        0x7B => "F12",
        vk if (b'A' as u16..=b'Z' as u16).contains(&vk) => [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
            "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
        ][(vk - b'A' as u16) as usize],
        vk if (b'0' as u16..=b'9' as u16).contains(&vk) => {
            ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"][(vk - b'0' as u16) as usize]
        }
        _ => "Key",
    }
}
pub fn clamp_frame_thickness(value: i32) -> i32 {
    value.clamp(MIN_FRAME_THICKNESS, MAX_FRAME_THICKNESS)
}
pub fn clamp_frame_opacity(value: i32) -> i32 {
    value.clamp(MIN_FRAME_OPACITY, MAX_FRAME_OPACITY)
}
pub fn step_opacity(current: i32, delta: i32) -> i32 {
    current
        .saturating_add(delta)
        .clamp(MIN_OPACITY, MAX_OPACITY)
}
pub fn alpha_from_percent(percent: i32) -> u8 {
    ((255 * percent.clamp(MIN_OPACITY, MAX_OPACITY)) / 100) as u8
}
pub fn percent_from_alpha(alpha: u8) -> i32 {
    (i32::from(alpha) * 100) / 255
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_control_titles_become_a_visible_label() {
        assert_eq!(
            Pin::on(""),
            Pin::On {
                title: "Pencere".into()
            }
        );
        assert_eq!(
            Pin::on("\u{0}\n\t"),
            Pin::On {
                title: "Pencere".into()
            }
        );
        assert!(!Pin::Off.active());
        assert_eq!(Pin::Off.title(), None);
    }
    #[test]
    fn titles_are_collapsed_and_clipped_for_status() {
        let pin = Pin::on("  Notepad\n  -  untitled  ");
        assert_eq!(pin.title(), Some("Notepad - untitled"));
        assert!(pin.active());
        let long = "A".repeat(40);
        let clipped = Pin::on(long);
        assert_eq!(clipped.title().unwrap().chars().count(), 33);
        assert!(clipped.title().unwrap().ends_with('…'));
    }
    #[test]
    fn opacity_steps_match_powertoys_bounds() {
        assert_eq!(step_opacity(100, -OPACITY_STEP), 90);
        assert_eq!(step_opacity(25, -OPACITY_STEP), MIN_OPACITY);
        assert_eq!(step_opacity(20, OPACITY_STEP), 30);
        assert_eq!(step_opacity(95, OPACITY_STEP), MAX_OPACITY);
        assert_eq!(alpha_from_percent(100), 255);
        assert_eq!(alpha_from_percent(20), 51);
        assert_eq!(percent_from_alpha(255), 100);
        assert_eq!(percent_from_alpha(51), 20);
    }
    #[test]
    fn multiple_pins_use_a_count_label() {
        assert_eq!(Pin::summary(0, "Notepad"), Pin::Off);
        assert_eq!(Pin::summary(1, "Notepad"), Pin::on("Notepad"));
        assert_eq!(Pin::summary(2, "Notepad").title(), Some("2 pencere"));
        assert!(Pin::summary(3, "Explorer").active());
    }
    #[test]
    fn hotkeys_require_a_modifier_and_roundtrip_storage() {
        assert_eq!(Hotkey::pin_default().label(), "Win+Ctrl+T");
        assert_eq!(Hotkey::opacity_up_default().label(), "Win+Ctrl++");
        assert_eq!(Hotkey::opacity_down_default().label(), "Win+Ctrl+-");
        assert_eq!(Hotkey::new(false, false, false, false, VK_T), None);
        assert_eq!(Hotkey::new(true, false, false, false, 0x11), None);
        let key = Hotkey::from_storage(Hotkey::pin_default().mods_mask(), VK_T).unwrap();
        assert_eq!(key, Hotkey::pin_default());
        assert_eq!(key.win32_mods() & 0x4000, 0x4000);
        assert_eq!(clamp_frame_thickness(0), MIN_FRAME_THICKNESS);
        assert_eq!(clamp_frame_thickness(99), MAX_FRAME_THICKNESS);
        assert_eq!(clamp_frame_opacity(0), MIN_FRAME_OPACITY);
    }
}
