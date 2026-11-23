//! An opaque sRGB color.
//!
//! Layers above a platform crate pass colors as [`Rgb`]. A platform crate
//! converts to its native packing (`COLORREF` on Windows) where it draws.

/// Red, green and blue, eight bits each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// From `0xRRGGBB`; higher bits are ignored.
    pub const fn from_hex(rgb: u32) -> Self {
        Self {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
        }
    }

    /// As `0xRRGGBB`.
    pub const fn hex(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_keeps_channel_order_and_drops_high_bits() {
        let orange = Rgb::from_hex(0xFF4500);
        assert_eq!(orange, Rgb::new(0xFF, 0x45, 0x00));
        assert_eq!(orange.hex(), 0x00FF_4500);
        assert_eq!(Rgb::from_hex(0xAB12_3456), Rgb::new(0x12, 0x34, 0x56));
    }
}
