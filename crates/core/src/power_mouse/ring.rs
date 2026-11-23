//! Pure geometry and pixels of the Actions Ring: eight wedges around a
//! centre hole, slot 0 at the top and the rest clockwise. The platform layer
//! puts the pixels in a layered window and feeds the cursor position back.

pub const SLOTS: usize = 8;
/// Ring diameter at 96 DPI.
pub const DIAMETER_96: i32 = 340;
/// Inner radius as a fraction of the outer one; inside it is the centre
/// hole, which cancels.
const HOLE: f32 = 0.40;
/// Half of the gap between two wedges, in pixels.
const GAP: f32 = 2.0;

const WEDGE: f32 = 360.0 / SLOTS as f32;

/// Radii in pixels for a ring drawn in a square of `size` pixels.
fn radii(size: i32) -> (f32, f32) {
    let outer = size as f32 / 2.0 - 2.0;
    (outer * HOLE, outer)
}

/// Polar position of an offset from the centre: distance, and the angle in
/// degrees clockwise from straight up.
fn polar(dx: f32, dy: f32) -> (f32, f32) {
    let distance = dx.hypot(dy);
    let angle = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
    (distance, angle)
}

/// The slot whose wedge holds the angle, and how far the angle is from that
/// wedge's centre line, in degrees.
fn wedge_at(angle: f32) -> (usize, f32) {
    let turned = (angle + WEDGE / 2.0).rem_euclid(360.0);
    let slot = (turned / WEDGE) as usize % SLOTS;
    (slot, (turned - slot as f32 * WEDGE - WEDGE / 2.0).abs())
}

/// The slot under a point `dx`, `dy` pixels from the ring's centre, for a
/// ring `size` pixels wide; `None` in the centre hole, in the gaps between
/// wedges and outside the ring.
pub fn slot_at(dx: i32, dy: i32, size: i32) -> Option<usize> {
    let (inner, outer) = radii(size);
    let (distance, angle) = polar(dx as f32, dy as f32);
    if distance < inner || distance > outer {
        return None;
    }
    let (slot, off_centre) = wedge_at(angle);
    let to_edge = distance * (WEDGE / 2.0 - off_centre).to_radians().sin();
    (to_edge >= GAP).then_some(slot)
}

/// Where a slot's label is centred, as an offset from the ring's centre.
pub fn label_centre(slot: usize, size: i32) -> (i32, i32) {
    let (inner, outer) = radii(size);
    // A little outside the middle of the band, where a wedge is wider.
    let radius = inner + (outer - inner) * 0.56;
    let angle = (slot as f32 * WEDGE).to_radians();
    (
        (radius * angle.sin()).round() as i32,
        (-radius * angle.cos()).round() as i32,
    )
}

/// How a slot is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    /// Nothing bound: dimmed and not selectable.
    Empty,
    Bound,
    /// Bound and under the cursor.
    Hover,
}

/// Colours as `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bound: u32,
    pub hover: u32,
}

/// Opacity of each shade, 0–255.
fn opacity(shade: Shade) -> f32 {
    match shade {
        Shade::Empty => 110.0,
        Shade::Bound => 232.0,
        Shade::Hover => 248.0,
    }
}

/// Renders the ring as `size * size` premultiplied BGRA pixels, top row
/// first, as `UpdateLayeredWindow` expects. Edges are anti-aliased by the
/// distance to the nearest edge; everything off the ring is transparent.
pub fn render(size: i32, shades: &[Shade; SLOTS], palette: Palette) -> Vec<u32> {
    let side = usize::try_from(size).unwrap_or(0);
    let (inner, outer) = radii(size);
    let centre = size as f32 / 2.0;
    let mut pixels = vec![0u32; side * side];
    for (index, pixel) in pixels.iter_mut().enumerate() {
        let x = (index % side) as f32 + 0.5 - centre;
        let y = (index / side) as f32 + 0.5 - centre;
        let (distance, angle) = polar(x, y);
        let (slot, off_centre) = wedge_at(angle);
        let to_edge = distance * (WEDGE / 2.0 - off_centre).to_radians().sin();
        // Coverage falls from 1 to 0 across one pixel at each edge.
        let coverage = (outer - distance + 0.5).clamp(0.0, 1.0)
            * (distance - inner + 0.5).clamp(0.0, 1.0)
            * (to_edge - GAP + 0.5).clamp(0.0, 1.0);
        if coverage <= 0.0 {
            continue;
        }
        let shade = shades[slot];
        let colour = if shade == Shade::Hover {
            palette.hover
        } else {
            palette.bound
        };
        let alpha = opacity(shade) * coverage;
        let channel = |shift: u32| {
            let value = (colour >> shift & 0xFF) as f32;
            (value * alpha / 255.0).round() as u32
        };
        *pixel = (alpha.round() as u32) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0);
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: i32 = 340;

    #[test]
    fn slots_run_clockwise_from_the_top() {
        // Offsets are screen coordinates: y grows downwards.
        assert_eq!(slot_at(0, -120, SIZE), Some(0));
        assert_eq!(slot_at(85, -85, SIZE), Some(1));
        assert_eq!(slot_at(120, 0, SIZE), Some(2));
        assert_eq!(slot_at(85, 85, SIZE), Some(3));
        assert_eq!(slot_at(0, 120, SIZE), Some(4));
        assert_eq!(slot_at(-85, 85, SIZE), Some(5));
        assert_eq!(slot_at(-120, 0, SIZE), Some(6));
        assert_eq!(slot_at(-85, -85, SIZE), Some(7));
    }

    #[test]
    fn hole_gaps_and_outside_select_nothing() {
        assert_eq!(slot_at(0, 0, SIZE), None);
        assert_eq!(slot_at(0, -40, SIZE), None, "inside the centre hole");
        assert_eq!(slot_at(0, -175, SIZE), None, "beyond the outer edge");
        // The boundary between slots 0 and 1 lies 22.5 degrees from the top.
        let (sin, cos) = 22.5f32.to_radians().sin_cos();
        let on_gap = ((120.0 * sin).round() as i32, (-120.0 * cos).round() as i32);
        assert_eq!(slot_at(on_gap.0, on_gap.1, SIZE), None);
        assert_eq!(slot_at(on_gap.0 - 8, on_gap.1, SIZE), Some(0));
        assert_eq!(slot_at(on_gap.0 + 8, on_gap.1, SIZE), Some(1));
    }

    #[test]
    fn labels_sit_inside_their_own_wedge() {
        for slot in 0..SLOTS {
            let (dx, dy) = label_centre(slot, SIZE);
            assert_eq!(slot_at(dx, dy, SIZE), Some(slot));
        }
    }

    #[test]
    fn pixels_are_premultiplied_and_follow_the_shades() {
        let mut shades = [Shade::Bound; SLOTS];
        shades[2] = Shade::Hover;
        shades[4] = Shade::Empty;
        let palette = Palette {
            bound: 0x20_20_20,
            hover: 0x00_78_D4,
        };
        let pixels = render(SIZE, &shades, palette);
        assert_eq!(pixels.len(), (SIZE * SIZE) as usize);
        let at = |dx: i32, dy: i32| pixels[((SIZE / 2 + dy) * SIZE + SIZE / 2 + dx) as usize];
        // Centre hole and corners are fully transparent.
        assert_eq!(at(0, 0), 0);
        assert_eq!(pixels[0], 0);
        // A bound wedge: its colour scaled by its opacity.
        let bound = at(0, -120);
        assert_eq!(bound >> 24, 232);
        assert_eq!(bound & 0xFF, (0x20 * 232 + 127) / 255);
        // The hovered wedge takes the hover colour; the empty one is fainter.
        let hover = at(120, 0);
        assert_eq!(hover >> 24, 248);
        assert_eq!(hover >> 8 & 0xFF, (0x78 * 248 + 127) / 255);
        assert_eq!(at(0, 120) >> 24, 110);
        // No channel ever exceeds its alpha, as premultiplied alpha requires.
        assert!(pixels.iter().all(|pixel| {
            let alpha = pixel >> 24;
            [16, 8, 0]
                .iter()
                .all(|shift| (pixel >> shift & 0xFF) <= alpha)
        }));
    }
}
