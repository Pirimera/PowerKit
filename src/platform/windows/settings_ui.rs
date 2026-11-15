//! Shared WinUI settings surfaces for the shell and dialogs.
//!
//! Layout-only helpers: no command IDs, snapshots or Win32 dialogs. Callers wire
//! controls and messages; this module keeps Fluent card chrome consistent.
use std::time::Duration;
use windows_reactor::*;

pub(super) const STEPPER_DOWN: &str = "−";
pub(super) const STEPPER_UP: &str = "+";

pub(super) fn heading(text: &str, size: f64) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(size)
        .font_weight(FontWeight::SEMI_BOLD)
        .text_wrapping(TextWrapping::Wrap)
}

pub(super) fn caption(text: &str) -> View {
    TextBlock::new()
        .text(text)
        .text_wrapping(TextWrapping::Wrap)
        .opacity(0.8)
        .into()
}

pub(super) fn section(text: &str) -> View {
    TextBlock::new()
        .text(text)
        .font_weight(FontWeight::SEMI_BOLD)
        .margin(Thickness::new(0.0, 16.0, 0.0, 4.0))
        .into()
}

pub(super) fn stack(rows: Vec<View>, spacing: f64) -> View {
    StackPanel::new().spacing(spacing).keyed_children(
        rows.into_iter()
            .enumerate()
            .map(|(index, row)| KeyedView::new(index, row)),
    )
}

pub(super) fn surface(content: View) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .padding(Thickness::xy(16.0, 14.0))
        .content(content)
}

pub(super) fn row(labels: View, control: View) -> View {
    surface(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((labels,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((control,)),
            )),
    )
}

pub(super) fn card(title: &str, control: impl Into<View>) -> View {
    row(
        TextBlock::new()
            .text(title)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
        control.into(),
    )
}

pub(super) fn described_card(title: &str, description: View, control: impl Into<View>) -> View {
    let labels = stack(
        vec![
            TextBlock::new()
                .text(title)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            description,
        ],
        2.0,
    );
    row(labels, control.into())
}

pub(super) fn wide_card(title: &str, control: View) -> View {
    surface(stack(
        vec![TextBlock::new().text(title).into(), control],
        8.0,
    ))
}

pub(super) fn stepper_card(
    title: &str,
    value: String,
    down: impl Into<View>,
    up: impl Into<View>,
) -> View {
    let control = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children((
            down.into(),
            TextBlock::new()
                .text(value)
                .min_width(64.0)
                .vertical_alignment(VerticalAlignment::Center)
                .horizontal_alignment(HorizontalAlignment::Center),
            up.into(),
        ));
    card(title, control)
}

pub(super) fn color_swatch(colorref: u32) -> View {
    let [r, g, b, _] = colorref.to_le_bytes();
    Border::new()
        .width(48.0)
        .height(20.0)
        .corner_radius(CornerRadius::uniform(4.0))
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .background(Color::rgb(r, g, b))
        .into()
}

/// Accent while a capture session is active; callers add `on_click` then `content`.
pub(super) fn hotkey_button(capturing: bool) -> Button {
    Button::new()
        .min_width(168.0)
        .style(capturing.then_some(ButtonStyle::Accent))
}

pub(super) fn hotkey_card(title: &str, button: impl Into<View>) -> View {
    card(title, button)
}

/// Near-square rail target (~128×116 CSS px at 200% DPI → 64×58 DIP).
pub(super) const RAIL_ITEM_WIDTH: f64 = 64.0;
const RAIL_ITEM_HEIGHT: f64 = 58.0;
const RAIL_CORNER: f64 = 8.0;
const RAIL_FADE: Duration = Duration::from_millis(167);
/// Store content pane: large top-left radius against the acrylic chrome.
pub(super) const CONTENT_TOP_LEFT_RADIUS: f64 = 12.0;
/// Extra SolidBackground opacity on the main pane only. Stacked over the shared
/// shell tint, this makes the pane 50% less transparent than the rail/chrome.
pub(super) const CONTENT_PANE_EXTRA_OPACITY: f64 = 0.50;

/// One Microsoft Store-style rail item.
pub(super) struct RailItem<'a> {
    pub glyph: &'a str,
    /// Filled glyph shown while selected (Store swaps outline for solid).
    pub selected_glyph: &'a str,
    pub label: &'a str,
    pub selected: bool,
    /// Reserved for Accent-tinted selected icons once a safe brush path exists.
    pub accent: Option<u32>,
}

fn rail_icon(glyph: &str) -> View {
    Viewbox::new()
        .width(16.0)
        .height(16.0)
        .stretch(Stretch::Uniform)
        .horizontal_alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Center)
        .slot(ViewboxSlot::Child, FontIcon::new().glyph(glyph))
}

/// Centered rail caption. windows-reactor TextBlock has no TextAlignment, so
/// multi-word labels become one centered line per word (max two lines).
fn rail_label(label: &str) -> View {
    let mut words = label.split_whitespace();
    let Some(first) = words.next() else {
        return TextBlock::new().text(label).font_size(10.0).into();
    };
    let rest: Vec<&str> = words.collect();
    let line = |text: String| {
        TextBlock::new()
            .text(text)
            .font_size(10.0)
            .horizontal_alignment(HorizontalAlignment::Center)
    };
    if rest.is_empty() {
        return line(first.into()).into();
    }
    StackPanel::new()
        .spacing(0.0)
        .horizontal_alignment(HorizontalAlignment::Center)
        .children((line(first.into()), line(rest.join(" "))))
}

/// Store rail item: icon over label; selected hides the label, shows a filled icon,
/// a muted chip, and a left Accent indicator. Chip/indicator stay mounted so the
/// opacity fade can run. Avoids Scale / negative margins / Subtle foreground
/// overrides — those crash WinUI (`0xc000027b`) in this reactor build.
pub(super) fn nav_rail_item(item: RailItem<'_>, button: Button) -> View {
    let _ = item.accent;
    let selected = item.selected;
    let shown = if selected { 1.0 } else { 0.0 };
    let chip = Border::new()
        .width(RAIL_ITEM_WIDTH)
        .height(RAIL_ITEM_HEIGHT)
        .background(ThemeBrush::CardBackground)
        .corner_radius(CornerRadius::uniform(RAIL_CORNER))
        .opacity(shown)
        .opacity_transition(RAIL_FADE);
    let indicator = Border::new()
        .width(3.0)
        .height(16.0)
        .corner_radius(CornerRadius::uniform(2.0))
        .background(ThemeBrush::Accent)
        .horizontal_alignment(HorizontalAlignment::Left)
        .vertical_alignment(VerticalAlignment::Center)
        .margin(Thickness::new(2.0, 0.0, 0.0, 0.0))
        .opacity(shown)
        .opacity_transition(RAIL_FADE);
    let content: View = if selected {
        rail_icon(item.selected_glyph)
    } else {
        StackPanel::new()
            .spacing(2.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .children((rail_icon(item.glyph), rail_label(item.label)))
    };
    let button = button
        .width(RAIL_ITEM_WIDTH)
        .height(RAIL_ITEM_HEIGHT)
        .style(ButtonStyle::Subtle)
        .horizontal_content_alignment(HorizontalAlignment::Center)
        .vertical_content_alignment(VerticalAlignment::Center)
        .content(content);
    Grid::new()
        .width(RAIL_ITEM_WIDTH)
        .height(RAIL_ITEM_HEIGHT)
        .keyed_children([
            KeyedView::new("chip", chip),
            KeyedView::new("indicator", indicator),
            KeyedView::new(if selected { "on" } else { "off" }, button),
        ])
}

/// Maps persisted shell opacity (50–100) to a SolidBackground tint over Acrylic.
/// Higher percent → stronger tint (less desktop show-through); acrylic always stays on.
pub(super) fn acrylic_tint_opacity(percent: i32) -> f64 {
    let percent = f64::from(percent.clamp(50, 100));
    0.22 + (percent - 50.0) / 50.0 * 0.68
}

#[cfg(test)]
mod tests {
    use super::acrylic_tint_opacity;

    #[test]
    fn acrylic_tint_scales_with_shell_opacity() {
        assert!((acrylic_tint_opacity(50) - 0.22).abs() < f64::EPSILON);
        assert!((acrylic_tint_opacity(100) - 0.90).abs() < f64::EPSILON);
        assert!(acrylic_tint_opacity(75) > acrylic_tint_opacity(50));
        assert_eq!(acrylic_tint_opacity(10), acrylic_tint_opacity(50));
        assert_eq!(acrylic_tint_opacity(200), acrylic_tint_opacity(100));
    }
}
