//! Keyboard chords shared by AlwaysTop, AlwaysOn, ScreenRuler and PowerMouse: storage
//! form, labels, built-in defaults and the Win+Alt fallback for a taken default.
const MOD_WIN_BIT: u8 = 1;
const MOD_CTRL_BIT: u8 = 2;
const MOD_ALT_BIT: u8 = 4;
const MOD_SHIFT_BIT: u8 = 8;
const VK_T: u16 = 0x54;
const VK_M: u16 = 0x4D;
const VK_A: u16 = 0x41;
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
    /// Toggles AlwaysOn; PowerToys Awake has no shortcut to collide with.
    pub const fn always_on_default() -> Self {
        Self {
            win: true,
            ctrl: true,
            alt: false,
            shift: true,
            vk: VK_A,
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
    /// The same chord with Alt instead of Ctrl, tried when a default Win+Ctrl chord is taken.
    fn alt_fallback(self) -> Option<Self> {
        if self.ctrl && !self.alt {
            Self::new(self.win, false, true, self.shift, self.vk)
        } else {
            None
        }
    }
    /// Registers `self` through `register`; a built-in default (`is_default`) that is
    /// taken falls back to its Win+Alt form. Returns the chord that is actually active,
    /// or `None` when nothing was registered, so callers never show an inactive chord.
    pub fn register_with_fallback(
        self,
        is_default: bool,
        mut register: impl FnMut(Self) -> bool,
    ) -> Option<Self> {
        if register(self) {
            return Some(self);
        }
        self.alt_fallback()
            .filter(|_| is_default)
            .filter(|&fallback| register(fallback))
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_reports_only_the_chord_that_is_active() {
        let default = Hotkey::pin_default();
        let alt = Hotkey::new(true, false, true, false, VK_T).unwrap();
        assert_eq!(
            default.register_with_fallback(true, |_| true),
            Some(default)
        );
        // A taken default falls back to Win+Alt.
        assert_eq!(
            default.register_with_fallback(true, |key| key != default),
            Some(alt)
        );
        // A user chord never silently moves, and total failure reports nothing.
        assert_eq!(
            default.register_with_fallback(false, |key| key != default),
            None
        );
        assert_eq!(default.register_with_fallback(true, |_| false), None);
        let mut tried = Vec::new();
        let _ = alt.register_with_fallback(true, |key| {
            tried.push(key);
            false
        });
        assert_eq!(tried, [alt]);
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
    }
}
