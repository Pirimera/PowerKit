//! Converts the neutral [`Rgb`] to and from `COLORREF` (`0x00BBGGRR`), the
//! packing GDI, DWM and the registry accent value use. Preferences and the
//! settings shell carry `Rgb`; drawing code converts here, once, at its edge.
use powerkit_core::primitives::Rgb;

pub fn colorref(color: Rgb) -> u32 {
    u32::from_le_bytes([color.r, color.g, color.b, 0])
}

/// The high byte of a `COLORREF` is not a channel and is dropped.
pub fn from_colorref(colorref: u32) -> Rgb {
    let [r, g, b, _] = colorref.to_le_bytes();
    Rgb::new(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_packs_blue_high_and_round_trips() {
        // #FF4500 is COLORREF 0x000045FF.
        let orange = Rgb::from_hex(0xFF4500);
        assert_eq!(colorref(orange), 0x0000_45FF);
        assert_eq!(from_colorref(0x0000_45FF), orange);
        assert_eq!(from_colorref(0xFF00_45FF), orange);
        // The accent blue stored as `0078D4` is COLORREF 0x00D47800.
        assert_eq!(colorref(Rgb::from_hex(0x0078D4)), 0x00D4_7800);
    }
}
