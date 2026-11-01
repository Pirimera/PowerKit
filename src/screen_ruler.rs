//! PowerToys Screen Ruler measurement math. Capture and the overlay stay in the Windows adapter.
pub const DEFAULT_TOLERANCE: u8 = 30;
pub const WHEEL_STEP: i32 = 15;
pub const FEET_HALF: i32 = 2;
/// COLORREF for #FF4500, the PowerToys cross default.
pub const DEFAULT_LINE_COLOR: u32 = 0x0000_45FF;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    None = 0,
    Bounds = 1,
    Spacing = 2,
    Horizontal = 3,
    Vertical = 4,
}
impl Mode {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::None,
            1 => Self::Bounds,
            2 => Self::Spacing,
            3 => Self::Horizontal,
            4 => Self::Vertical,
            _ => return None,
        })
    }
    pub fn index(self) -> i32 {
        self as i32
    }
    pub fn edges(self) -> bool {
        matches!(self, Self::Spacing | Self::Horizontal | Self::Vertical)
    }
    pub fn axes(self) -> (bool, bool) {
        match self {
            Self::None => (false, false),
            Self::Bounds | Self::Spacing => (true, true),
            Self::Horizontal => (true, false),
            Self::Vertical => (false, true),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Pixel = 0,
    Inch = 1,
    Centimetre = 2,
    Millimetre = 3,
    Dip = 4,
}
impl Unit {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Pixel,
            1 => Self::Inch,
            2 => Self::Centimetre,
            3 => Self::Millimetre,
            4 => Self::Dip,
            _ => return None,
        })
    }
    pub fn index(self) -> i32 {
        self as i32
    }
    pub fn abbreviation(self) -> &'static str {
        match self {
            Self::Pixel => "px",
            Self::Inch => "in",
            Self::Centimetre => "cm",
            Self::Millimetre => "mm",
            Self::Dip => "dip",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Span {
    pub fn bounds(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self {
            left: x0.min(x1),
            top: y0.min(y1),
            right: x0.max(x1),
            bottom: y0.max(y1),
        }
    }
    pub fn width_px(self) -> f32 {
        (self.right - self.left + 1) as f32
    }
    pub fn height_px(self) -> f32 {
        (self.bottom - self.top + 1) as f32
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterClick {
    Close,
    Keep,
}
pub fn spacing_after_click(shift: bool) -> AfterClick {
    if shift {
        AfterClick::Keep
    } else {
        AfterClick::Close
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundsRight {
    CancelDrag,
    ClearKept,
    Close,
}
pub fn bounds_right(dragging: bool, has_kept: bool) -> BoundsRight {
    if dragging {
        BoundsRight::CancelDrag
    } else if has_kept {
        BoundsRight::ClearKept
    } else {
        BoundsRight::Close
    }
}
pub fn escape_copies(mode: Mode) -> bool {
    mode == Mode::Bounds
}
pub fn wheel_tolerance(current: u8, delta: i16) -> u8 {
    if delta == 0 {
        return current;
    }
    let step = if delta > 0 { WHEEL_STEP } else { -WHEEL_STEP };
    (i32::from(current) + step).clamp(0, 255) as u8
}
pub fn pixels_close(start: u32, next: u32, tolerance: u8, per_channel: bool) -> bool {
    let diffs = [0, 8, 16, 24].map(|shift| {
        let left = ((start >> shift) & 0xff) as i32;
        let right = ((next >> shift) & 0xff) as i32;
        (left - right).abs()
    });
    let tolerance = i32::from(tolerance);
    if per_channel {
        diffs.iter().all(|diff| *diff <= tolerance)
    } else {
        diffs.iter().sum::<i32>() <= tolerance
    }
}
pub fn detect_edges(
    pixels: &[u32],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    per_channel: bool,
    tolerance: u8,
) -> Option<Span> {
    if width < 1 || height < 1 || pixels.len() < (width as usize) * (height as usize) {
        return None;
    }
    let walk = |axis_x: bool, increment: bool| {
        find_edge(
            pixels,
            width,
            height,
            x,
            y,
            EdgeWalk {
                per_channel,
                tolerance,
                axis_x,
                increment,
            },
        )
    };
    Some(Span {
        left: walk(true, false),
        top: walk(false, false),
        right: walk(true, true),
        bottom: walk(false, true),
    })
}
fn find_edge(pixels: &[u32], width: i32, height: i32, cx: i32, cy: i32, sample: EdgeWalk) -> i32 {
    let max_dim = if sample.axis_x { width } else { height };
    if width < 3 || height < 3 {
        return if sample.increment { max_dim - 1 } else { 0 };
    }
    let mut x = cx.clamp(1, width - 2);
    let mut y = cy.clamp(1, height - 2);
    let start = pixels[(y * width + x) as usize];
    loop {
        let (old_x, old_y) = (x, y);
        if sample.axis_x {
            if sample.increment {
                x += 1;
                if x == max_dim {
                    break;
                }
            } else {
                x -= 1;
                if x < 0 {
                    break;
                }
            }
        } else if sample.increment {
            y += 1;
            if y == max_dim {
                break;
            }
        } else {
            y -= 1;
            if y < 0 {
                break;
            }
        }
        let next = pixels[(y * width + x) as usize];
        if !pixels_close(start, next, sample.tolerance, sample.per_channel) {
            return if sample.axis_x { old_x } else { old_y };
        }
    }
    if sample.increment { max_dim - 1 } else { 0 }
}
struct EdgeWalk {
    per_channel: bool,
    tolerance: u8,
    axis_x: bool,
    increment: bool,
}
pub fn convert(pixels: f32, unit: Unit, px_to_mm: f32, dpi: f32) -> f32 {
    match unit {
        Unit::Pixel => pixels,
        Unit::Dip => {
            if dpi > 0.0 {
                pixels * 96.0 / dpi
            } else {
                pixels
            }
        }
        Unit::Inch if px_to_mm > 0.0 => pixels * px_to_mm / 25.4,
        Unit::Centimetre if px_to_mm > 0.0 => pixels * px_to_mm / 10.0,
        Unit::Millimetre if px_to_mm > 0.0 => pixels * px_to_mm,
        Unit::Inch => pixels / 96.0,
        Unit::Centimetre => pixels / 96.0 * 2.54,
        Unit::Millimetre => pixels / 96.0 * 25.4,
    }
}
pub fn overlay_label(span: Span, mode: Mode, extra: Unit, px_to_mm: f32, dpi: f32) -> String {
    let (width, height) = mode.axes();
    let mut text = format_pair(span, width, height, Unit::Pixel, px_to_mm, dpi, 4);
    text.push(' ');
    text.push_str(Unit::Pixel.abbreviation());
    if extra != Unit::Pixel {
        let extra_text = format_pair(span, width, height, extra, px_to_mm, dpi, 4);
        if width && height {
            text.push('\n');
            text.push('(');
        } else {
            text.push_str(" (");
        }
        text.push_str(&extra_text);
        text.push(' ');
        text.push_str(extra.abbreviation());
        text.push(')');
    }
    text
}
pub fn clipboard_text(spans: &[Span], mode: Mode, unit: Unit, px_to_mm: f32, dpi: f32) -> String {
    let (width, height) = mode.axes();
    spans
        .iter()
        .map(|span| {
            let mut line = format_pair(*span, width, height, unit, px_to_mm, dpi, 6);
            if unit != Unit::Pixel {
                line.push(' ');
                line.push_str(unit.abbreviation());
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn format_pair(
    span: Span,
    width: bool,
    height: bool,
    unit: Unit,
    px_to_mm: f32,
    dpi: f32,
    digits: i32,
) -> String {
    let mut text = String::new();
    if width {
        text.push_str(&format_g(
            convert(span.width_px(), unit, px_to_mm, dpi),
            digits,
        ));
        if height {
            text.push_str(" \u{00d7} ");
        }
    }
    if height {
        text.push_str(&format_g(
            convert(span.height_px(), unit, px_to_mm, dpi),
            digits,
        ));
    }
    text
}
fn format_g(value: f32, digits: i32) -> String {
    if !value.is_finite() || digits < 1 {
        return "0".into();
    }
    let negative = value < 0.0;
    let value = value.abs();
    if value == 0.0 {
        return "0".into();
    }
    let mut exp = value.log10().floor() as i32;
    let mut scale = 10f32.powi(digits - 1 - exp);
    let mut mantissa = (value * scale).round();
    if mantissa >= 10f32.powi(digits) {
        mantissa /= 10.0;
        exp += 1;
        scale = 10f32.powi(digits - 1 - exp);
        let _ = scale;
    }
    let mut body = format!("{}", mantissa as i64);
    if body.len() < digits as usize {
        body = format!("{:0width$}", mantissa as i64, width = digits as usize);
    }
    let sign = if negative { "-" } else { "" };
    if exp < -4 || exp >= digits {
        let mut scientific = String::new();
        scientific.push(body.as_bytes()[0] as char);
        let rest = body[1..].trim_end_matches('0');
        if !rest.is_empty() {
            scientific.push('.');
            scientific.push_str(rest);
        }
        return format!("{sign}{scientific}e{exp:+03}");
    }
    if exp >= 0 {
        let split = (exp as usize + 1).min(body.len());
        let (whole, fraction) = body.split_at(split);
        let fraction = fraction.trim_end_matches('0');
        if fraction.is_empty() {
            format!("{sign}{whole}")
        } else {
            format!("{sign}{whole}.{fraction}")
        }
    } else {
        let zeros = (-exp - 1) as usize;
        let fraction = body.trim_end_matches('0');
        format!("{sign}0.{}{fraction}", "0".repeat(zeros))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const BLUE: u32 = 0x0000_00FF;
    const GREEN: u32 = 0x0000_FF00;
    fn square() -> Vec<u32> {
        let mut pixels = vec![BLUE; 40 * 120];
        for y in 100..=104 {
            for x in 20..=24 {
                pixels[y * 40 + x] = GREEN;
            }
        }
        pixels
    }
    #[test]
    fn edge_walk_matches_the_powertoys_inclusive_square() {
        let span = detect_edges(&square(), 40, 120, 22, 102, false, 30).unwrap();
        assert_eq!(
            span,
            Span {
                left: 20,
                top: 100,
                right: 24,
                bottom: 104
            }
        );
        assert_eq!(span.width_px(), 5.0);
        assert_eq!(span.height_px(), 5.0);
        assert_eq!(
            clipboard_text(&[span], Mode::Spacing, Unit::Pixel, 0.0, 96.0),
            "5 \u{00d7} 5"
        );
        assert_eq!(
            clipboard_text(&[span], Mode::Horizontal, Unit::Pixel, 0.0, 96.0),
            "5"
        );
        assert_eq!(
            overlay_label(span, Mode::Spacing, Unit::Pixel, 0.0, 96.0),
            "5 \u{00d7} 5 px"
        );
    }
    #[test]
    fn per_channel_and_sum_disagree_on_even_channel_deltas() {
        let close = pixels_close(0, 20 | (20 << 8) | (20 << 16), 30, true);
        let sum = pixels_close(0, 20 | (20 << 8) | (20 << 16), 30, false);
        assert!(close);
        assert!(!sum);
    }
    #[test]
    fn bounds_are_inclusive_and_shift_keeps_them() {
        let span = Span::bounds(10, 4, 0, 8);
        assert_eq!(span.width_px(), 11.0);
        assert_eq!(span.height_px(), 5.0);
        assert_eq!(spacing_after_click(true), AfterClick::Keep);
        assert_eq!(spacing_after_click(false), AfterClick::Close);
        assert_eq!(bounds_right(true, true), BoundsRight::CancelDrag);
        assert_eq!(bounds_right(false, true), BoundsRight::ClearKept);
        assert_eq!(bounds_right(false, false), BoundsRight::Close);
        assert!(escape_copies(Mode::Bounds));
        assert!(!escape_copies(Mode::Spacing));
    }
    #[test]
    fn wheel_steps_fifteen_and_units_follow_the_fallback_dpi() {
        assert_eq!(wheel_tolerance(30, 120), 45);
        assert_eq!(wheel_tolerance(30, -120), 15);
        assert_eq!(wheel_tolerance(250, 120), 255);
        assert_eq!(wheel_tolerance(10, -120), 0);
        let span = Span::bounds(0, 0, 95, 47);
        assert_eq!(
            overlay_label(span, Mode::Bounds, Unit::Inch, 0.0, 96.0),
            "96 \u{00d7} 48 px\n(1 \u{00d7} 0.5 in)"
        );
        assert_eq!(
            clipboard_text(&[span], Mode::Horizontal, Unit::Dip, 0.0, 192.0),
            "48 dip"
        );
        let inches = convert(254.0, Unit::Inch, 25.4 / 100.0, 96.0);
        assert!((inches - 2.54).abs() < 0.001);
    }
    #[test]
    fn significant_digits_match_printf_g() {
        assert_eq!(format_g(100.0, 4), "100");
        assert_eq!(format_g(10.5, 4), "10.5");
        assert_eq!(format_g(1920.0, 4), "1920");
        assert_eq!(format_g(10000.0, 4), "1e+04");
        assert_eq!(format_g(0.5, 6), "0.5");
    }
}
