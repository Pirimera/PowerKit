//! Pure layout geometry for the PowerMouse interactive mouse diagram:
//! supported device artwork metadata, PNG dimension parsing, physical button
//! anchor projection, non-overlapping callout placement and routed leader lines.
use super::Trigger;

/// Logical DIP size of the interactive mouse canvas on the Buttons tab.
pub const CANVAS_W: f64 = 540.0;
pub const CANVAS_H: f64 = 368.0;
/// Width and height of one floating callout card beside the mouse.
pub const CARD_W: f64 = 160.0;
pub const CARD_H: f64 = 52.0;
/// Minimum vertical space kept between two callout cards on the same side.
pub const CARD_GAP: f64 = 10.0;
/// Horizontal gap between the mouse render box and a side callout card.
pub const SIDE_GAP: f64 = 12.0;
/// Margin kept at the top and bottom of the canvas for callout cards.
const EDGE_MARGIN: f64 = 8.0;
/// Maximum height and width allocated to the center mouse render.
const MAX_MOUSE_W: f64 = CANVAS_W - 2.0 * (CARD_W + SIDE_GAP);
const MAX_MOUSE_H: f64 = 324.0;
/// How far past the mouse render box a leader line's 45° elbow may reach.
const LEADER_STUB: f64 = 6.0;

/// Which side of the mouse a callout card sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// One interactive callout on the mouse canvas. Most map 1:1 to a [`Trigger`];
/// `ThumbWheel` groups [`Trigger::ThumbLeft`] and [`Trigger::ThumbRight`] on a
/// single physical anchor point so the wheel gets one clean callout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalloutId {
    Trigger(Trigger),
    ThumbWheel,
}

impl CalloutId {
    /// Whether this callout corresponds to the currently selected trigger.
    pub fn matches(self, trigger: Trigger) -> bool {
        match self {
            Self::Trigger(id) => id == trigger,
            Self::ThumbWheel => trigger.is_thumb_wheel(),
        }
    }

    /// Primary trigger selected when the user clicks this callout.
    pub fn primary_trigger(self, current: Trigger) -> Trigger {
        match self {
            Self::Trigger(id) => id,
            Self::ThumbWheel if current.is_thumb_wheel() => current,
            Self::ThumbWheel => Trigger::ThumbLeft,
        }
    }
}

/// Artwork and physical button markers for a recognized Logitech mouse model.
/// Markers are percentages `(0..100)` inside the device silhouette's `origin`
/// bounding box, matching OpenLogi's device asset metadata.
#[derive(Clone, Copy, Debug)]
pub struct DeviceArt {
    /// OpenLogi asset depot name (e.g. `"mx_master_4"`).
    pub depot: &'static str,
    /// Silhouette bounding box inside `side_core.png`, in pixels.
    pub origin_w: u32,
    pub origin_h: u32,
    /// Physical button anchor points in `side_core.png`.
    pub markers: &'static [(CalloutId, Side, f64, f64)],
}

/// MX Master 4 (`side_core.png`: 656×1024, origin 636×1024).
const MX_MASTER_4_MARKERS: &[(CalloutId, Side, f64, f64)] = &[
    (
        CalloutId::Trigger(Trigger::GestureButton),
        Side::Left,
        38.6,
        36.1,
    ),
    (CalloutId::Trigger(Trigger::Forward), Side::Left, 42.5, 47.0),
    (
        CalloutId::Trigger(Trigger::HapticPanel),
        Side::Left,
        30.4,
        48.9,
    ),
    (CalloutId::Trigger(Trigger::Back), Side::Left, 47.3, 60.0),
    (CalloutId::Trigger(Trigger::Middle), Side::Right, 75.5, 19.0),
    (
        CalloutId::Trigger(Trigger::ModeShift),
        Side::Right,
        80.5,
        39.5,
    ),
    (CalloutId::ThumbWheel, Side::Right, 56.5, 56.4),
];

/// MX Master 3S / 3 (`side_core.png`, approximate silhouette markers).
const MX_MASTER_3_MARKERS: &[(CalloutId, Side, f64, f64)] = &[
    (CalloutId::Trigger(Trigger::Forward), Side::Left, 36.0, 46.0),
    (CalloutId::Trigger(Trigger::Back), Side::Left, 39.0, 58.0),
    (
        CalloutId::Trigger(Trigger::GestureButton),
        Side::Left,
        24.0,
        64.0,
    ),
    (CalloutId::Trigger(Trigger::Middle), Side::Right, 66.0, 22.0),
    (
        CalloutId::Trigger(Trigger::ModeShift),
        Side::Right,
        68.0,
        40.0,
    ),
    (CalloutId::ThumbWheel, Side::Right, 46.0, 40.0),
];

/// MX Anywhere 3S (compact mouse without a thumb wheel).
const MX_ANYWHERE_3S_MARKERS: &[(CalloutId, Side, f64, f64)] = &[
    (CalloutId::Trigger(Trigger::Forward), Side::Left, 26.0, 44.0),
    (CalloutId::Trigger(Trigger::Back), Side::Left, 28.0, 56.0),
    (CalloutId::Trigger(Trigger::Middle), Side::Right, 50.0, 22.0),
    (
        CalloutId::Trigger(Trigger::ModeShift),
        Side::Right,
        50.0,
        38.0,
    ),
];

const MX_MASTER_4_ART: DeviceArt = DeviceArt {
    depot: "mx_master_4",
    origin_w: 636,
    origin_h: 1024,
    markers: MX_MASTER_4_MARKERS,
};

const MX_MASTER_3S_ART: DeviceArt = DeviceArt {
    depot: "mx_master_3s",
    origin_w: 636,
    origin_h: 1024,
    markers: MX_MASTER_3_MARKERS,
};

const MX_MASTER_3_ART: DeviceArt = DeviceArt {
    depot: "mx_master_3",
    origin_w: 636,
    origin_h: 1024,
    markers: MX_MASTER_3_MARKERS,
};

const MX_ANYWHERE_3S_ART: DeviceArt = DeviceArt {
    depot: "mx_anywhere_3s",
    origin_w: 636,
    origin_h: 1024,
    markers: MX_ANYWHERE_3S_MARKERS,
};

/// Resolves real-device artwork for a detected Logitech mouse by USB/BLE
/// product ID or reported model name. Returns `None` for unrecognized devices.
pub fn device_art(product_id: u16, name: &str) -> Option<&'static DeviceArt> {
    match product_id {
        0xB042 | 0xB048 => return Some(&MX_MASTER_4_ART),
        0xB034 | 0xB043 | 0xB035 | 0xB044 => return Some(&MX_MASTER_3S_ART),
        0xB023 | 0xB028 => return Some(&MX_MASTER_3_ART),
        0xB037 | 0xB038 => return Some(&MX_ANYWHERE_3S_ART),
        _ => {}
    }
    let lower = name.to_ascii_lowercase();
    if lower.contains("mx master 4") {
        Some(&MX_MASTER_4_ART)
    } else if lower.contains("mx master 3s") {
        Some(&MX_MASTER_3S_ART)
    } else if lower.contains("mx master 3") {
        Some(&MX_MASTER_3_ART)
    } else if lower.contains("mx anywhere 3s") {
        Some(&MX_ANYWHERE_3S_ART)
    } else {
        None
    }
}

/// Reads `(width, height)` from a PNG file's IHDR header without decoding pixels.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != PNG_SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width > 0 && height > 0).then_some((width, height))
}

/// Placed rectangle of the center mouse image or vector silhouette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One placed callout card with its anchor point on the mouse and its routed
/// leader-line polyline vertices.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedCallout {
    pub id: CalloutId,
    pub side: Side,
    /// Physical anchor point on the mouse image, in canvas DIPs.
    pub anchor: (f64, f64),
    /// Top-left corner of the callout card, in canvas DIPs.
    pub card_x: f64,
    pub card_y: f64,
    /// Polyline from `anchor` to the card's inner edge (`2` or `3` vertices).
    pub leader: Vec<(f64, f64)>,
}

/// Complete geometry for one frame of the interactive mouse canvas.
#[derive(Clone, Debug)]
pub struct CanvasLayout {
    pub mouse: MouseRect,
    pub callouts: Vec<PlacedCallout>,
}

/// Computes the centered mouse rectangle that fits `(MAX_MOUSE_W, MAX_MOUSE_H)`
/// while preserving `aspect = width / height`.
fn fit_mouse_rect(png_w: f64, png_h: f64) -> MouseRect {
    let aspect = (png_w / png_h.max(1.0)).clamp(0.4, 1.0);
    let mut h = MAX_MOUSE_H;
    let mut w = h * aspect;
    if w > MAX_MOUSE_W {
        w = MAX_MOUSE_W;
        h = w / aspect;
    }
    MouseRect {
        x: (CANVAS_W - w) / 2.0,
        y: (CANVAS_H - h) / 2.0,
        w,
        h,
    }
}

/// Fallback anchor points (percent of the generic sculpted mouse silhouette)
/// used when no real device photo is shown.
fn generic_anchor(id: CalloutId) -> Option<(Side, f64, f64)> {
    Some(match id {
        CalloutId::Trigger(Trigger::GestureButton) => (Side::Left, 15.0, 42.0),
        CalloutId::Trigger(Trigger::Forward) => (Side::Left, 17.0, 38.0),
        CalloutId::Trigger(Trigger::Back) => (Side::Left, 17.0, 54.0),
        CalloutId::Trigger(Trigger::HapticPanel) => (Side::Left, 13.0, 66.0),
        CalloutId::Trigger(Trigger::TiltLeft) => (Side::Left, 42.0, 24.0),
        CalloutId::Trigger(Trigger::Middle) => (Side::Right, 50.0, 22.0),
        CalloutId::Trigger(Trigger::TiltRight) => (Side::Right, 58.0, 29.0),
        CalloutId::Trigger(Trigger::ModeShift) => (Side::Right, 50.0, 42.0),
        CalloutId::ThumbWheel => (Side::Right, 22.0, 32.0),
        CalloutId::Trigger(_) => return None,
    })
}

/// Builds the full canvas layout for the active set of callouts.
///
/// When `art` and `png_size` are present, anchors are projected onto the real
/// device PNG using the asset's `origin` bounding box; otherwise they sit on
/// the generic sculpted mouse silhouette.
pub fn layout(
    active: &[CalloutId],
    art: Option<&DeviceArt>,
    png_size: Option<(u32, u32)>,
) -> CanvasLayout {
    let mouse = match (art, png_size) {
        (Some(_), Some((w, h))) => fit_mouse_rect(f64::from(w), f64::from(h)),
        (Some(art), None) => fit_mouse_rect(f64::from(art.origin_w), f64::from(art.origin_h)),
        (None, _) => fit_mouse_rect(176.0, 296.0),
    };

    // Translate silhouette percentage coordinates into canvas DIPs.
    let (bbox_x, bbox_w) = match (art, png_size) {
        (Some(art), Some((w, h))) => {
            let pw = f64::from(w.max(1));
            let ph = f64::from(h.max(1));
            let oh = f64::from(art.origin_h.max(1));
            let ow = if (oh - ph).abs() <= ph * 0.05 {
                f64::from(art.origin_w).min(pw)
            } else {
                pw
            };
            let rendered_w = mouse.w * ow / pw;
            (mouse.x + (mouse.w - rendered_w) / 2.0, rendered_w)
        }
        _ => (mouse.x, mouse.w),
    };

    let mut left_items: Vec<(CalloutId, (f64, f64))> = Vec::new();
    let mut right_items: Vec<(CalloutId, (f64, f64))> = Vec::new();

    for &id in active {
        let marker = art
            .and_then(|spec| {
                spec.markers
                    .iter()
                    .find(|(m_id, _, _, _)| *m_id == id)
                    .map(|(_, side, mx, my)| (*side, *mx, *my))
            })
            .or_else(|| generic_anchor(id));
        let Some((side, mx, my)) = marker else {
            continue;
        };
        let anchor = (
            bbox_x + (mx / 100.0) * bbox_w,
            mouse.y + (my / 100.0) * mouse.h,
        );
        match side {
            Side::Left => left_items.push((id, anchor)),
            Side::Right => right_items.push((id, anchor)),
        }
    }

    // Order each side top-to-bottom by physical anchor height so routed leader
    // lines never cross.
    left_items.sort_by(|a, b| a.1.1.total_cmp(&b.1.1));
    right_items.sort_by(|a, b| a.1.1.total_cmp(&b.1.1));

    let mut callouts = Vec::with_capacity(left_items.len() + right_items.len());
    place_side(&left_items, Side::Left, mouse, &mut callouts);
    place_side(&right_items, Side::Right, mouse, &mut callouts);

    CanvasLayout { mouse, callouts }
}

/// Spreads one side's cards vertically around their anchor heights without
/// overlap and routes a clean 45°-elbow leader line to each card.
fn place_side(
    items: &[(CalloutId, (f64, f64))],
    side: Side,
    mouse: MouseRect,
    out: &mut Vec<PlacedCallout>,
) {
    if items.is_empty() {
        return;
    }
    let min_cy = EDGE_MARGIN + CARD_H / 2.0;
    let max_cy = CANVAS_H - EDGE_MARGIN - CARD_H / 2.0;
    let step = CARD_H + CARD_GAP;

    let mut centers: Vec<f64> = items
        .iter()
        .map(|(_, (_, ay))| ay.clamp(min_cy, max_cy))
        .collect();

    // Forward pass: enforce minimum vertical separation downward.
    for i in 1..centers.len() {
        let floor = centers[i - 1] + step;
        if centers[i] < floor {
            centers[i] = floor;
        }
    }
    // Backward pass: if the stack pushed past the bottom margin, pull upward.
    if let Some(&last) = centers.last()
        && last > max_cy
    {
        let shift = last - max_cy;
        for cy in &mut centers {
            *cy -= shift;
        }
        for i in (0..centers.len().saturating_sub(1)).rev() {
            let ceiling = centers[i + 1] - step;
            if centers[i] > ceiling {
                centers[i] = ceiling;
            }
        }
    }
    // Final clamp in case there are many cards on a short canvas.
    for cy in &mut centers {
        *cy = cy.clamp(min_cy, max_cy);
    }

    let card_x = match side {
        Side::Left => 0.0,
        Side::Right => CANVAS_W - CARD_W,
    };

    for (&(id, anchor), cy) in items.iter().zip(centers) {
        let leader = route_leader(anchor, side, cy, mouse);
        out.push(PlacedCallout {
            id,
            side,
            anchor,
            card_x,
            card_y: cy - CARD_H / 2.0,
            leader,
        });
    }
}

/// Routes a clean 2-segment leader polyline (anchor → 45° elbow → horizontal
/// stub into the card's inner edge), or a single horizontal line when level.
fn route_leader(anchor: (f64, f64), side: Side, card_cy: f64, mouse: MouseRect) -> Vec<(f64, f64)> {
    let (hx, hy) = anchor;
    let edge_x = match side {
        Side::Left => CARD_W,
        Side::Right => CANVAS_W - CARD_W,
    };
    let rise = (card_cy - hy).abs();
    if rise < 1.0 {
        return vec![(hx, hy), (edge_x, card_cy)];
    }
    let elbow_x = match side {
        Side::Left => {
            let limit = (mouse.x - LEADER_STUB).max(edge_x + 4.0);
            (hx - rise).max(limit.min(hx))
        }
        Side::Right => {
            let limit = (mouse.x + mouse.w + LEADER_STUB).min(edge_x - 4.0);
            (hx + rise).min(limit.max(hx))
        }
    };
    vec![(hx, hy), (elbow_x, card_cy), (edge_x, card_cy)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_mx_master_4_by_pid_and_name() {
        let by_pid = device_art(0xB042, "").expect("MX Master 4 by PID");
        assert_eq!(by_pid.depot, "mx_master_4");
        let by_name = device_art(0x0000, "Logitech MX Master 4").expect("MX Master 4 by name");
        assert_eq!(by_name.depot, "mx_master_4");
        assert!(device_art(0x1234, "Generic Office Mouse").is_none());
    }

    #[test]
    fn parses_png_ihdr_dimensions_and_rejects_invalid_headers() {
        let mut fake_png = Vec::new();
        fake_png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        fake_png.extend_from_slice(&13u32.to_be_bytes());
        fake_png.extend_from_slice(b"IHDR");
        fake_png.extend_from_slice(&656u32.to_be_bytes());
        fake_png.extend_from_slice(&1024u32.to_be_bytes());
        assert_eq!(png_dimensions(&fake_png), Some((656, 1024)));
        assert_eq!(png_dimensions(b"not a png"), None);
        assert_eq!(png_dimensions(&fake_png[..16]), None);
    }

    #[test]
    fn mx_master_4_layout_has_non_overlapping_cards_inside_canvas() {
        let art = device_art(0xB042, "MX Master 4").unwrap();
        let active = [
            CalloutId::Trigger(Trigger::GestureButton),
            CalloutId::Trigger(Trigger::Forward),
            CalloutId::Trigger(Trigger::Back),
            CalloutId::Trigger(Trigger::HapticPanel),
            CalloutId::Trigger(Trigger::Middle),
            CalloutId::Trigger(Trigger::ModeShift),
            CalloutId::ThumbWheel,
        ];
        let laid_out = layout(&active, Some(art), Some((656, 1024)));
        assert_eq!(laid_out.callouts.len(), 7);
        for side in [Side::Left, Side::Right] {
            let cards: Vec<&PlacedCallout> = laid_out
                .callouts
                .iter()
                .filter(|c| c.side == side)
                .collect();
            for c in &cards {
                assert!(c.card_y >= EDGE_MARGIN - 0.1);
                assert!(c.card_y + CARD_H <= CANVAS_H - EDGE_MARGIN + 0.1);
                assert!(c.leader.len() >= 2);
                assert_eq!(c.leader[0], c.anchor);
            }
            for pair in cards.windows(2) {
                assert!(
                    pair[1].card_y - pair[0].card_y >= CARD_H + CARD_GAP - 0.1,
                    "overlap on {side:?}: {:?} vs {:?}",
                    pair[0].card_y,
                    pair[1].card_y
                );
            }
        }
    }

    #[test]
    fn generic_mouse_layout_places_all_standard_triggers() {
        let active = [
            CalloutId::Trigger(Trigger::Forward),
            CalloutId::Trigger(Trigger::Back),
            CalloutId::Trigger(Trigger::TiltLeft),
            CalloutId::Trigger(Trigger::Middle),
            CalloutId::Trigger(Trigger::TiltRight),
        ];
        let laid_out = layout(&active, None, None);
        assert_eq!(laid_out.callouts.len(), 5);
        assert!(laid_out.callouts.iter().any(|c| c.side == Side::Left));
        assert!(laid_out.callouts.iter().any(|c| c.side == Side::Right));
    }
}
