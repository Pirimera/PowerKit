//! Translates PowerKit's neutral keys and chords to Windows virtual-key codes
//! and `RegisterHotKey` modifiers. This is the only place that knows the two
//! numberings coincide on Windows.
use powerkit_core::hotkey::Hotkey;
use powerkit_core::primitives::KeyCode;

/// The Windows virtual-key code for `key`.
pub fn vk(key: KeyCode) -> u16 {
    key.stored()
}

/// The neutral key for a Windows virtual-key code; zero is not a key.
/// What Windows calls the meta key in a shortcut.
pub const META_KEY: &str = "Win";

pub fn from_vk(vk: u16) -> Option<KeyCode> {
    KeyCode::from_stored(vk)
}

/// `RegisterHotKey` modifier flags for `chord`, always without auto-repeat.
pub fn register_mods(chord: Hotkey) -> u32 {
    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_WIN: u32 = 0x0008;
    const MOD_NOREPEAT: u32 = 0x4000;
    let mut mods = MOD_NOREPEAT;
    if chord.alt {
        mods |= MOD_ALT;
    }
    if chord.ctrl {
        mods |= MOD_CONTROL;
    }
    if chord.shift {
        mods |= MOD_SHIFT;
    }
    if chord.meta {
        mods |= MOD_WIN;
    }
    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_register_without_repeat_and_keys_keep_their_virtual_key() {
        let pin = Hotkey::pin_default();
        // Win+Ctrl, no repeat.
        assert_eq!(register_mods(pin), 0x4000 | 0x0008 | 0x0002);
        assert_eq!(
            register_mods(Hotkey::ruler_default()),
            0x4000 | 0x0008 | 0x0002 | 0x0004
        );
        assert_eq!(vk(pin.key), 0x54);
        assert_eq!(from_vk(0x54), Some(KeyCode::T));
        assert_eq!(from_vk(0), None);
    }
}
