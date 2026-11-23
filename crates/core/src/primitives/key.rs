//! PowerKit's key numbering.
//!
//! A [`KeyCode`] names a logical key (the key that types `T`, not a position on
//! the board), which is what a shortcut shown as "Win+Ctrl+T" means. The
//! numbering is one fixed table used on every platform. Its values follow the
//! Windows virtual-key table, as Chromium's `KeyboardCode` does, because those
//! are the numbers settings files already store: adopting them as PowerKit's own
//! table keeps every stored shortcut valid in both directions. Only a platform
//! crate translates a `KeyCode` to or from a native key code.

/// A logical, non-zero key in PowerKit's numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyCode(u16);

impl KeyCode {
    pub const TAB: Self = Self(0x09);
    pub const ESCAPE: Self = Self(0x1B);
    pub const SPACE: Self = Self(0x20);
    pub const A: Self = Self(0x41);
    pub const M: Self = Self(0x4D);
    pub const T: Self = Self(0x54);
    pub const NUMPAD_ADD: Self = Self(0x6B);
    pub const NUMPAD_SUBTRACT: Self = Self(0x6D);
    pub const F1: Self = Self(0x70);
    pub const F13: Self = Self(0x7C);
    pub const F24: Self = Self(0x87);
    /// The key that types `+` (or `=`) on the main block.
    pub const PLUS: Self = Self(0xBB);
    /// The key that types `-` on the main block.
    pub const MINUS: Self = Self(0xBD);

    /// The key stored as `code`; zero is not a key.
    pub const fn from_stored(code: u16) -> Option<Self> {
        if code == 0 { None } else { Some(Self(code)) }
    }

    /// The number written to settings files.
    pub const fn stored(self) -> u16 {
        self.0
    }

    /// Function key `number` (1 to 24).
    pub const fn function(number: u8) -> Option<Self> {
        if number >= 1 && number <= 24 {
            Some(Self(Self::F1.0 + number as u16 - 1))
        } else {
            None
        }
    }

    /// The function key number (1 to 24) when this is a function key.
    pub const fn function_number(self) -> Option<u8> {
        if self.0 >= Self::F1.0 && self.0 <= Self::F24.0 {
            Some((self.0 - Self::F1.0) as u8 + 1)
        } else {
            None
        }
    }

    /// Shift, Ctrl, Alt and the two Win/Meta keys cannot end a chord.
    pub const fn is_modifier(self) -> bool {
        matches!(self.0, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C)
    }

    /// The keycap text for the keys a shortcut label can name; `"Key"` otherwise.
    pub fn label(self) -> &'static str {
        const LETTERS: [&str; 26] = [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
            "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
        ];
        const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
        const FUNCTIONS: [&str; 12] = [
            "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
        ];
        match self {
            Self::PLUS | Self::NUMPAD_ADD => "+",
            Self::MINUS | Self::NUMPAD_SUBTRACT => "-",
            Self::SPACE => "Space",
            Self::ESCAPE => "Esc",
            Self::TAB => "Tab",
            Self(code @ 0x41..=0x5A) => LETTERS[usize::from(code - 0x41)],
            Self(code @ 0x30..=0x39) => DIGITS[usize::from(code - 0x30)],
            // F13 to F24 keep the generic label, as shortcut labels always have.
            Self(code @ 0x70..=0x7B) => FUNCTIONS[usize::from(code - 0x70)],
            _ => "Key",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_numbers_round_trip_and_zero_is_not_a_key() {
        assert_eq!(KeyCode::from_stored(0), None);
        for code in [1, 0x41, 0x54, 0x7C, 0xBB, u16::MAX] {
            assert_eq!(KeyCode::from_stored(code).unwrap().stored(), code);
        }
        // The numbers settings files already hold keep their meaning.
        assert_eq!(KeyCode::from_stored(0x54), Some(KeyCode::T));
        assert_eq!(KeyCode::from_stored(0xBB), Some(KeyCode::PLUS));
    }

    #[test]
    fn function_keys_map_both_ways_within_one_to_twenty_four() {
        assert_eq!(KeyCode::function(0), None);
        assert_eq!(KeyCode::function(25), None);
        assert_eq!(KeyCode::function(1), Some(KeyCode::F1));
        assert_eq!(KeyCode::function(13), Some(KeyCode::F13));
        assert_eq!(KeyCode::function(24), Some(KeyCode::F24));
        for number in 1..=24 {
            assert_eq!(
                KeyCode::function(number).unwrap().function_number(),
                Some(number)
            );
        }
        assert_eq!(KeyCode::T.function_number(), None);
    }

    #[test]
    fn labels_name_keycaps_and_modifiers_are_recognised() {
        assert_eq!(KeyCode::T.label(), "T");
        assert_eq!(KeyCode::from_stored(0x39).unwrap().label(), "9");
        assert_eq!(KeyCode::PLUS.label(), "+");
        assert_eq!(KeyCode::NUMPAD_SUBTRACT.label(), "-");
        assert_eq!(KeyCode::function(12).unwrap().label(), "F12");
        assert_eq!(KeyCode::F13.label(), "Key");
        for code in [0x10, 0x11, 0x12, 0x5B, 0x5C] {
            assert!(KeyCode::from_stored(code).unwrap().is_modifier());
        }
        assert!(!KeyCode::T.is_modifier());
    }
}
