//! Keyboard chords shared by AlwaysTop, AlwaysOn, ScreenRuler and PowerMouse: storage
//! form, labels, built-in defaults and the Win+Alt fallback for a taken default.
//!
//! A chord is platform-neutral: `meta` is the Win key on Windows (Command on
//! macOS, Super on Linux) and the key is a [`KeyCode`]. Registering a chord with
//! the system belongs to the platform crate.
const MOD_META_BIT: u8 = 1;
const MOD_CTRL_BIT: u8 = 2;
const MOD_ALT_BIT: u8 = 4;
const MOD_SHIFT_BIT: u8 = 8;
use crate::primitives::KeyCode;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub meta: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: KeyCode,
}
impl Hotkey {
    pub const fn pin_default() -> Self {
        Self {
            meta: true,
            ctrl: true,
            alt: false,
            shift: false,
            key: KeyCode::T,
        }
    }
    pub const fn opacity_up_default() -> Self {
        Self {
            meta: true,
            ctrl: true,
            alt: false,
            shift: false,
            key: KeyCode::PLUS,
        }
    }
    pub const fn opacity_down_default() -> Self {
        Self {
            meta: true,
            ctrl: true,
            alt: false,
            shift: false,
            key: KeyCode::MINUS,
        }
    }
    pub const fn ruler_default() -> Self {
        Self {
            meta: true,
            ctrl: true,
            alt: false,
            shift: true,
            key: KeyCode::M,
        }
    }
    /// Toggles AlwaysOn; PowerToys Awake has no shortcut to collide with.
    pub const fn always_on_default() -> Self {
        Self {
            meta: true,
            ctrl: true,
            alt: false,
            shift: true,
            key: KeyCode::A,
        }
    }
    /// A chord needs at least one modifier and a key that is not itself a modifier.
    pub fn new(meta: bool, ctrl: bool, alt: bool, shift: bool, key: KeyCode) -> Option<Self> {
        if !(meta || ctrl || alt || shift) || key.is_modifier() {
            return None;
        }
        Some(Self {
            meta,
            ctrl,
            alt,
            shift,
            key,
        })
    }
    /// The chord stored as a modifier mask and a key number.
    pub fn from_storage(mods: u8, key: u16) -> Option<Self> {
        Self::new(
            mods & MOD_META_BIT != 0,
            mods & MOD_CTRL_BIT != 0,
            mods & MOD_ALT_BIT != 0,
            mods & MOD_SHIFT_BIT != 0,
            KeyCode::from_stored(key)?,
        )
    }
    pub fn mods_mask(self) -> u8 {
        (u8::from(self.meta) * MOD_META_BIT)
            | (u8::from(self.ctrl) * MOD_CTRL_BIT)
            | (u8::from(self.alt) * MOD_ALT_BIT)
            | (u8::from(self.shift) * MOD_SHIFT_BIT)
    }
    /// The chord as text. `meta` is what the platform calls the meta key
    /// (Win, Command, Super); only a platform knows that.
    pub fn label(self, meta: &str) -> String {
        let mut parts = Vec::new();
        if self.meta {
            parts.push(meta);
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
        parts.push(self.key.label());
        parts.join("+")
    }
    /// The same chord with Alt instead of Ctrl, tried when a default Win+Ctrl chord is taken.
    fn alt_fallback(self) -> Option<Self> {
        if self.ctrl && !self.alt {
            Self::new(self.meta, false, true, self.shift, self.key)
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
/// Splits a [`Hotkey::label`] back into its keys for keycap display, keeping the
/// plus key of "Win+Ctrl++" as its own key.
pub fn label_keys(label: &str) -> Vec<&str> {
    match label.strip_suffix("++") {
        Some(rest) => rest.split('+').chain(["+"]).collect(),
        None => label.split('+').collect(),
    }
}
/// The first slot other than `slot` that already holds `key`. A chord can be
/// registered only once system-wide, so two PowerKit slots must never share one.
pub fn holder<T: Copy + PartialEq>(slots: &[(T, Hotkey)], slot: T, key: Hotkey) -> Option<T> {
    slots
        .iter()
        .find(|&&(other, held)| other != slot && held == key)
        .map(|&(other, _)| other)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_reports_only_the_chord_that_is_active() {
        let default = Hotkey::pin_default();
        let alt = Hotkey::new(true, false, true, false, KeyCode::T).unwrap();
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
    fn labels_split_into_keycaps_including_the_plus_key() {
        assert_eq!(
            label_keys(&Hotkey::ruler_default().label("Win")),
            ["Win", "Ctrl", "Shift", "M"]
        );
        assert_eq!(
            label_keys(&Hotkey::opacity_up_default().label("Win")),
            ["Win", "Ctrl", "+"]
        );
        assert_eq!(
            label_keys(&Hotkey::opacity_down_default().label("Win")),
            ["Win", "Ctrl", "-"]
        );
    }
    #[test]
    fn holder_finds_another_slot_with_the_same_chord() {
        let slots = [
            (1, Hotkey::pin_default()),
            (2, Hotkey::ruler_default()),
            (3, Hotkey::always_on_default()),
        ];
        assert_eq!(holder(&slots, 3, Hotkey::ruler_default()), Some(2));
        // Re-recording a slot's own chord is not a conflict.
        assert_eq!(holder(&slots, 2, Hotkey::ruler_default()), None);
        assert_eq!(holder(&slots, 1, Hotkey::opacity_up_default()), None);
    }
    #[test]
    fn hotkeys_require_a_modifier_and_roundtrip_storage() {
        assert_eq!(Hotkey::pin_default().label("Win"), "Win+Ctrl+T");
        assert_eq!(Hotkey::opacity_up_default().label("Win"), "Win+Ctrl++");
        assert_eq!(Hotkey::opacity_down_default().label("Win"), "Win+Ctrl+-");
        assert_eq!(Hotkey::new(false, false, false, false, KeyCode::T), None);
        let ctrl = KeyCode::from_stored(0x11).unwrap();
        assert_eq!(Hotkey::new(true, false, false, false, ctrl), None);
        let pin = Hotkey::pin_default();
        let key = Hotkey::from_storage(pin.mods_mask(), pin.key.stored()).unwrap();
        assert_eq!(key, pin);
        // Stored numbers keep their meaning: 0x54 is still the T key.
        assert_eq!(Hotkey::from_storage(3, 0x54), Some(pin));
        assert_eq!(Hotkey::from_storage(3, 0), None);
        assert_eq!(Hotkey::from_storage(0, 0x54), None);
    }
}
