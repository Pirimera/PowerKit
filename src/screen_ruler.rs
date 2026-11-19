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
    /// PowerToys toggles the active toolbar button off instead of leaving it selected.
    pub fn toggled(self, chosen: Self) -> Self {
        if self == chosen { Self::None } else { chosen }
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
/// A Bounds drag in monitor pixels: where it started and where the pointer is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drag {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}
impl Drag {
    pub fn span(self) -> Span {
        Span::bounds(self.x0, self.y0, self.x1, self.y1)
    }
}
/// A measurement left on screen with Shift, drawn again at `cursor_x`/`cursor_y`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kept {
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub span: Span,
}
/// Result of a primary-button release that measured something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release {
    /// Span to copy in addition to the kept marks; `None` when the measurement was
    /// kept, because it is then already one of them.
    pub current: Option<Span>,
    /// True when the session ends (a spacing tool clicked without Shift).
    pub close: bool,
}
/// One monitor's marks: the Bounds drag in progress and the kept measurements.
/// The Windows overlay forwards pointer events here and only carries out what
/// these methods decide (capture, clipboard, closing).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub drag: Option<Drag>,
    pub kept: Vec<Kept>,
}
impl Marks {
    /// Primary button down. Only Bounds starts a drag; returns whether it did, so
    /// the caller captures the pointer.
    pub fn press(&mut self, mode: Mode, x: i32, y: i32) -> bool {
        if mode != Mode::Bounds {
            return false;
        }
        self.drag = Some(Drag {
            x0: x,
            y0: y,
            x1: x,
            y1: y,
        });
        true
    }
    /// Pointer moved while a drag may be in progress.
    pub fn drag_to(&mut self, x: i32, y: i32) {
        if let Some(drag) = self.drag.as_mut() {
            drag.x1 = x;
            drag.y1 = y;
        }
    }
    /// Primary button up. Bounds ends its drag; the spacing tools use `measured`,
    /// the cursor and the edge span found under it (`None` when the cursor is off
    /// this monitor or no span was found). Shift keeps the measurement on screen.
    /// Returns `None` when nothing was measured.
    pub fn release(
        &mut self,
        mode: Mode,
        shift: bool,
        measured: Option<(i32, i32, Span)>,
    ) -> Option<Release> {
        let (cursor_x, cursor_y, span, keep, close) = match mode {
            Mode::Bounds => {
                let drag = self.drag.take()?;
                (drag.x1, drag.y1, drag.span(), shift, false)
            }
            Mode::Spacing | Mode::Horizontal | Mode::Vertical => {
                let (x, y, span) = measured?;
                let keep = spacing_after_click(shift) == AfterClick::Keep;
                (x, y, span, keep, !keep)
            }
            Mode::None => return None,
        };
        if keep {
            self.kept.push(Kept {
                cursor_x,
                cursor_y,
                span,
            });
        }
        Some(Release {
            current: (!keep).then_some(span),
            close,
        })
    }
    /// Secondary button up. In Bounds it cancels a drag, else clears kept marks,
    /// else closes; every other tool closes. The returned action is already
    /// applied to the marks.
    pub fn right_click(&mut self, mode: Mode) -> BoundsRight {
        let action = if mode == Mode::Bounds {
            bounds_right(self.drag.is_some(), !self.kept.is_empty())
        } else {
            BoundsRight::Close
        };
        match action {
            BoundsRight::CancelDrag => self.drag = None,
            BoundsRight::ClearKept => self.kept.clear(),
            BoundsRight::Close => {}
        }
        action
    }
    /// A tool change starts with no marks.
    pub fn clear(&mut self) {
        self.drag = None;
        self.kept.clear();
    }
    /// Spans to copy from this monitor, in order: kept marks, the drag in
    /// progress, then `current`.
    pub fn spans(&self, current: Option<Span>) -> Vec<Span> {
        self.kept
            .iter()
            .map(|kept| kept.span)
            .chain(self.drag.map(Drag::span))
            .chain(current)
            .collect()
    }
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
/// Clipboard text for spans kept on several displays. Each group is
/// `(spans, px_to_mm, dpi)` of one display and converts with that display's own
/// physical size and scale; empty groups add no line.
pub fn clipboard_text_by_display(
    groups: &[(Vec<Span>, f32, f32)],
    mode: Mode,
    unit: Unit,
) -> String {
    groups
        .iter()
        .filter(|(spans, _, _)| !spans.is_empty())
        .map(|(spans, px_to_mm, dpi)| clipboard_text(spans, mode, unit, *px_to_mm, *dpi))
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
        assert_eq!(Mode::None.toggled(Mode::Bounds), Mode::Bounds);
        assert_eq!(Mode::Bounds.toggled(Mode::Spacing), Mode::Spacing);
        assert_eq!(Mode::Bounds.toggled(Mode::Bounds), Mode::None);
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
    fn span(left: i32, top: i32, right: i32, bottom: i32) -> Span {
        Span {
            left,
            top,
            right,
            bottom,
        }
    }
    #[test]
    fn bounds_drag_copies_on_release_and_shift_keeps_it_once() {
        let mut marks = Marks::default();
        assert!(marks.press(Mode::Bounds, 30, 20));
        marks.drag_to(10, 5);
        assert_eq!(marks.spans(None), vec![span(10, 5, 30, 20)]);
        let first = marks.release(Mode::Bounds, false, None).unwrap();
        assert_eq!(
            first,
            Release {
                current: Some(span(10, 5, 30, 20)),
                close: false
            }
        );
        assert_eq!(marks, Marks::default());
        assert!(marks.press(Mode::Bounds, 0, 0));
        marks.drag_to(4, 4);
        let kept = marks.release(Mode::Bounds, true, None).unwrap();
        // Kept spans are copied from `kept`, so the release adds no second copy.
        assert_eq!(kept.current, None);
        assert_eq!(marks.spans(kept.current), vec![span(0, 0, 4, 4)]);
        assert_eq!(marks.kept[0].cursor_x, 4);
        assert_eq!(marks.release(Mode::Bounds, false, None), None);
    }
    #[test]
    fn right_click_cancels_drag_then_clears_kept_then_closes() {
        let mut marks = Marks::default();
        marks.press(Mode::Bounds, 0, 0);
        marks.drag_to(9, 9);
        marks.release(Mode::Bounds, true, None);
        marks.press(Mode::Bounds, 20, 20);
        assert_eq!(marks.right_click(Mode::Bounds), BoundsRight::CancelDrag);
        assert_eq!(marks.drag, None);
        assert_eq!(marks.kept.len(), 1);
        assert_eq!(marks.right_click(Mode::Bounds), BoundsRight::ClearKept);
        assert!(marks.kept.is_empty());
        assert_eq!(marks.right_click(Mode::Bounds), BoundsRight::Close);
        let mut spacing = Marks::default();
        spacing.release(Mode::Spacing, true, Some((1, 1, span(0, 0, 2, 2))));
        assert_eq!(spacing.right_click(Mode::Spacing), BoundsRight::Close);
        assert_eq!(spacing.kept.len(), 1);
    }
    #[test]
    fn spacing_click_closes_unless_shift_keeps_the_mark() {
        let mut marks = Marks::default();
        assert!(!marks.press(Mode::Spacing, 5, 5));
        assert_eq!(marks.drag, None);
        let measured = Some((5, 6, span(0, 0, 10, 12)));
        assert_eq!(
            marks.release(Mode::Spacing, false, measured),
            Some(Release {
                current: Some(span(0, 0, 10, 12)),
                close: true
            })
        );
        assert!(marks.kept.is_empty());
        let kept = marks.release(Mode::Vertical, true, measured).unwrap();
        assert!(!kept.close);
        assert_eq!(
            marks.kept,
            vec![Kept {
                cursor_x: 5,
                cursor_y: 6,
                span: span(0, 0, 10, 12)
            }]
        );
        // Off this monitor or no edge found: nothing is measured or closed.
        assert_eq!(marks.release(Mode::Spacing, false, None), None);
        assert_eq!(marks.release(Mode::None, false, measured), None);
        marks.clear();
        assert_eq!(marks, Marks::default());
    }
    #[test]
    fn copied_spans_list_kept_then_drag_then_current() {
        let mut marks = Marks::default();
        marks.press(Mode::Bounds, 0, 0);
        marks.drag_to(1, 1);
        marks.release(Mode::Bounds, true, None);
        marks.press(Mode::Bounds, 5, 5);
        marks.drag_to(7, 8);
        assert_eq!(
            marks.spans(Some(span(9, 9, 9, 9))),
            vec![span(0, 0, 1, 1), span(5, 5, 7, 8), span(9, 9, 9, 9)]
        );
    }
    #[test]
    fn each_display_converts_its_own_spans() {
        let at_100 = (vec![Span::bounds(0, 0, 47, 9)], 0.0, 96.0);
        let at_200 = (vec![Span::bounds(0, 0, 95, 19)], 0.0, 192.0);
        let empty = (Vec::new(), 0.0, 144.0);
        assert_eq!(
            clipboard_text_by_display(&[at_100, empty, at_200], Mode::Horizontal, Unit::Dip),
            "48 dip\n48 dip"
        );
        let physical = (vec![Span::bounds(0, 0, 99, 0)], 0.25, 96.0);
        assert_eq!(
            clipboard_text_by_display(&[physical], Mode::Horizontal, Unit::Millimetre),
            "25 mm"
        );
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
