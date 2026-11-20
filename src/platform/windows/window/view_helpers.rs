//! Shared Fluent building blocks for the PowerStore and MediaRenamer workspaces and drawers.
use super::super::settings_ui;
use windows_reactor::*;

pub(super) const DRAWER_WIDTH: f64 = 460.0;
pub(super) const DRAWER_ICON_BOX: f64 = 36.0;

const GLYPH_CLOSE: &str = "\u{E711}";

/// At a 1 px font this is several thousand pixels of text, wider than any window.
const FILLER_GLYPHS: usize = 8192;

/// windows-reactor hosts each realized `ItemsRepeater` item in a `ContentControl` whose content
/// stays left-aligned, so an item is only as wide as its own content. A hidden, zero-height
/// text that wraps at the available width makes the item report the full list width instead.
pub(super) fn fill_item_width(item: View) -> View {
    let filler = TextBlock::new()
        .text("m".repeat(FILLER_GLYPHS))
        .font_size(1.0)
        .text_wrapping(TextWrapping::Wrap)
        .height(0.0)
        .opacity(0.0);
    Grid::new().children((item, filler))
}

/// Horizontal padding a Fluent button keeps on both sides of its content together; reactor
/// cannot change it, so an icon-only button has to be wide enough for glyph plus padding.
const BUTTON_PADDING_BOTH_SIDES: f64 = 24.0;

/// Width of an icon-only button: `min_width`, or wider when the glyph would be clipped
/// (a 14 px glyph in a 32 px button loses its edges).
pub(super) fn icon_button_width(glyph_size: f64, min_width: f64) -> f64 {
    min_width.max(glyph_size + BUTTON_PADDING_BOTH_SIDES)
}

/// Square icon-only subtle button with the glyph centered in it. The square is never
/// smaller than the glyph plus the button's own padding, otherwise the glyph is clipped.
pub(super) fn icon_button(
    glyph: &str,
    glyph_size: f64,
    size: f64,
    on_click: Callback<()>,
    tooltip: &str,
) -> View {
    let side = icon_button_width(glyph_size, size);
    Button::new()
        .width(side)
        .height(side)
        .style(ButtonStyle::Subtle)
        .horizontal_content_alignment(HorizontalAlignment::Center)
        .vertical_content_alignment(VerticalAlignment::Center)
        .on_click(on_click)
        .content(sized_icon(glyph, glyph_size))
        .tooltip(tooltip)
}

pub(super) fn sized_icon(glyph: &str, size: f64) -> View {
    Viewbox::new()
        .width(size)
        .height(size)
        .stretch(Stretch::Uniform)
        .vertical_alignment(VerticalAlignment::Center)
        .horizontal_alignment(HorizontalAlignment::Center)
        .slot(ViewboxSlot::Child, FontIcon::new().glyph(glyph))
}

pub(super) fn icon_label(glyph: &str, label: &str) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(6.0)
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            sized_icon(glyph, 13.0),
            TextBlock::new()
                .text(label)
                .vertical_alignment(VerticalAlignment::Center),
        ))
}

pub(super) fn button_row(buttons: Vec<View>) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .keyed_children(
            buttons
                .into_iter()
                .enumerate()
                .map(|(index, button)| KeyedView::new(index, button)),
        )
}

/// Equal-width horizontal action row so 1 or 2 buttons span the card cleanly.
pub(super) fn equal_button_row(buttons: Vec<View>) -> View {
    let count = buttons.len();
    let columns: Vec<GridLength> = (0..count).map(|_| GridLength::STAR).collect();
    let placed: Vec<KeyedView> = buttons
        .into_iter()
        .enumerate()
        .map(|(index, button)| {
            let col = i32::try_from(index).unwrap_or(0);
            KeyedView::new(
                index,
                StackPanel::new().grid_column(col).children((button,)),
            )
        })
        .collect();
    Grid::new()
        .columns(columns)
        .column_spacing(8.0)
        .keyed_children(placed)
}

#[derive(Clone, Copy)]
pub(super) enum PillTone {
    Success,
    Muted,
    Warning,
    Critical,
    Neutral,
}

pub(super) fn status_pill(glyph: Option<&str>, text: &str, tone: PillTone) -> View {
    let label_for_white = |glyph: Option<&str>| -> View {
        match glyph {
            Some(glyph) => StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(4.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    sized_icon(glyph, 10.0),
                    TextBlock::new()
                        .text(text)
                        .font_size(10.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .foreground(Color::rgb(0xFF, 0xFF, 0xFF))
                        .vertical_alignment(VerticalAlignment::Center),
                )),
            None => TextBlock::new()
                .text(text)
                .font_size(10.5)
                .font_weight(FontWeight::SEMI_BOLD)
                .foreground(Color::rgb(0xFF, 0xFF, 0xFF))
                .into(),
        }
    };

    match tone {
        PillTone::Success => Border::new()
            .corner_radius(CornerRadius::uniform(10.0))
            .padding(Thickness::xy(8.0, 2.0))
            .background(Color::rgb(0x1E, 0x7B, 0x3C))
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Center)
            .content(label_for_white(glyph)),
        PillTone::Muted => Border::new()
            .corner_radius(CornerRadius::uniform(10.0))
            .padding(Thickness::xy(8.0, 2.0))
            .background(Color::rgb(0x5E, 0x62, 0x68))
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Center)
            .content(label_for_white(glyph)),
        PillTone::Warning => Border::new()
            .corner_radius(CornerRadius::uniform(10.0))
            .padding(Thickness::xy(8.0, 2.0))
            .background(Color::rgb(0x9A, 0x67, 0x00))
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Center)
            .content(label_for_white(glyph)),
        PillTone::Critical => {
            let content: View = match glyph {
                Some(glyph) => StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(4.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        sized_icon(glyph, 10.0),
                        TextBlock::new()
                            .text(text)
                            .font_size(10.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .foreground(ThemeBrush::SystemCritical)
                            .vertical_alignment(VerticalAlignment::Center),
                    )),
                None => TextBlock::new()
                    .text(text)
                    .font_size(10.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(ThemeBrush::SystemCritical)
                    .into(),
            };
            Border::new()
                .corner_radius(CornerRadius::uniform(10.0))
                .padding(Thickness::xy(8.0, 2.0))
                .background(ThemeBrush::SystemCriticalBackground)
                .border_brush(ThemeBrush::SystemCritical)
                .border_thickness(Thickness::uniform(1.0))
                .horizontal_alignment(HorizontalAlignment::Left)
                .vertical_alignment(VerticalAlignment::Center)
                .content(content)
        }
        PillTone::Neutral => {
            let content: View = match glyph {
                Some(glyph) => StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(4.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        StackPanel::new()
                            .opacity(0.75)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((sized_icon(glyph, 10.0),)),
                        TextBlock::new()
                            .text(text)
                            .font_size(10.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .opacity(0.84)
                            .vertical_alignment(VerticalAlignment::Center),
                    )),
                None => TextBlock::new()
                    .text(text)
                    .font_size(10.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .opacity(0.84)
                    .into(),
            };
            Border::new()
                .corner_radius(CornerRadius::uniform(10.0))
                .padding(Thickness::xy(8.0, 2.0))
                .background(ThemeBrush::SolidBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .horizontal_alignment(HorizontalAlignment::Left)
                .vertical_alignment(VerticalAlignment::Center)
                .content(content)
        }
    }
}

pub(super) fn drawer_icon_badge(glyph: &str) -> View {
    Border::new()
        .width(DRAWER_ICON_BOX)
        .height(DRAWER_ICON_BOX)
        .corner_radius(CornerRadius::uniform(8.0))
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .content(sized_icon(glyph, 16.0))
}

pub(super) fn drawer_section(glyph: &str, text: &str, count: Option<usize>, first: bool) -> View {
    let mut parts: Vec<View> = vec![
        StackPanel::new()
            .opacity(0.68)
            .vertical_alignment(VerticalAlignment::Center)
            .children((sized_icon(glyph, 12.0),)),
        TextBlock::new()
            .text(text)
            .font_size(13.0)
            .font_weight(FontWeight::SEMI_BOLD)
            .opacity(0.90)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
    ];
    if let Some(count) = count {
        parts.push(status_pill(None, &count.to_string(), PillTone::Neutral));
    }
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(7.0)
        .vertical_alignment(VerticalAlignment::Center)
        .margin(Thickness::new(
            0.0,
            if first { 0.0 } else { 14.0 },
            0.0,
            2.0,
        ))
        .keyed_children(
            parts
                .into_iter()
                .enumerate()
                .map(|(index, item)| KeyedView::new(index, item)),
        )
}

pub(super) fn hairline() -> View {
    Border::new()
        .height(1.0)
        .background(ThemeBrush::CardStroke)
        .opacity(0.62)
        .into()
}

pub(super) fn accent_bar(height: f64, brush: Brush) -> Border {
    Border::new()
        .width(3.0)
        .height(height)
        .corner_radius(CornerRadius::uniform(2.0))
        .background(brush)
        .vertical_alignment(VerticalAlignment::Center)
}

pub(super) fn inset_box(content: View) -> View {
    Border::new()
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(6.0))
        .padding(Thickness::xy(10.0, 7.0))
        .content(content)
}

pub(super) fn inset_notice(glyph: &str, text: &str) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .padding(Thickness::xy(14.0, 11.0))
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR])
                .column_spacing(10.0)
                .children((
                    StackPanel::new()
                        .grid_column(0)
                        .opacity(0.68)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((sized_icon(glyph, 14.0),)),
                    TextBlock::new()
                        .grid_column(1)
                        .text(text)
                        .font_size(12.5)
                        .opacity(0.78)
                        .text_wrapping(TextWrapping::Wrap)
                        .vertical_alignment(VerticalAlignment::Center),
                )),
        )
}

pub(super) fn empty_card(glyph: &str, text: &str) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .padding(Thickness::xy(20.0, 42.0))
        .content(
            StackPanel::new()
                .spacing(12.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .children((
                    Border::new()
                        .width(48.0)
                        .height(48.0)
                        .corner_radius(CornerRadius::uniform(24.0))
                        .background(ThemeBrush::SolidBackground)
                        .border_brush(ThemeBrush::CardStroke)
                        .border_thickness(Thickness::uniform(1.0))
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .content(
                            StackPanel::new()
                                .opacity(0.55)
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((sized_icon(glyph, 22.0),)),
                        ),
                    TextBlock::new()
                        .text(text)
                        .font_size(13.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .opacity(0.76)
                        .horizontal_alignment(HorizontalAlignment::Center),
                )),
        )
}

pub(super) fn compact_empty_row(text: &str) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .padding(Thickness::xy(14.0, 10.0))
        .content(TextBlock::new().text(text).font_size(12.5).opacity(0.62))
}

/// Unified Windows 11 Settings-style card container with 1 px hairlines between rows.
pub(super) fn grouped_surface(items: Vec<View>) -> View {
    let mut children: Vec<View> = Vec::with_capacity(items.len() * 2);
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            children.push(hairline());
        }
        children.push(item);
    }
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .content(settings_ui::stack(children, 0.0))
}

pub(super) fn grouped_item(content: View) -> View {
    Border::new()
        .padding(Thickness::xy(14.0, 12.0))
        .content(content)
}

pub(super) fn grouped_setting_row(
    title: &str,
    description: Option<&str>,
    control: impl Into<View>,
) -> View {
    let label: View = match description {
        Some(desc) => settings_ui::stack(
            vec![
                TextBlock::new()
                    .text(title)
                    .font_size(13.5)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
                TextBlock::new()
                    .text(desc)
                    .font_size(12.0)
                    .opacity(0.68)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            ],
            2.0,
        ),
        None => TextBlock::new()
            .text(title)
            .font_size(13.5)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
    };
    grouped_item(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(14.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((label,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((control.into(),)),
            )),
    )
}

pub(super) fn fact_row(label: &str, value: View) -> View {
    Grid::new()
        .columns([GridLength::Pixel(116.0), GridLength::STAR])
        .column_spacing(12.0)
        .children((
            TextBlock::new()
                .grid_column(0)
                .text(label)
                .font_size(12.0)
                .opacity(0.66)
                .vertical_alignment(VerticalAlignment::Center),
            StackPanel::new()
                .grid_column(1)
                .vertical_alignment(VerticalAlignment::Center)
                .children((value,)),
        ))
}

/// Shared chrome for right-side workspace drawers (PowerStore and MediaRenamer).
pub(super) struct DrawerChrome<'a> {
    pub leading: Option<View>,
    pub title: String,
    pub title_badge: Option<String>,
    pub subtitle: String,
    pub sticky_bar: Option<View>,
    pub body: Vec<View>,
    pub close_tooltip: &'a str,
}

/// Renders a 460 px trailing-edge drawer over a semi-transparent dimming scrim.
pub(super) fn drawer_shell(
    chrome: DrawerChrome<'_>,
    make_close: impl Fn() -> Callback<()>,
) -> View {
    let scrim = Grid::new().grid_column(0).children((
        Border::new()
            .background(Color::argb(104, 0, 0, 0))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch),
        Button::new()
            .style(ButtonStyle::Subtle)
            .opacity(0.01)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .on_click(make_close())
            .content(""),
    ));

    let title_text = TextBlock::new()
        .text(chrome.title)
        .font_size(18.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .text_wrapping(TextWrapping::Wrap);
    let title_view: View = match chrome.title_badge {
        Some(badge) => StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((title_text, status_pill(None, &badge, PillTone::Neutral))),
        None => title_text.into(),
    };

    let mut header_labels: Vec<View> = vec![title_view];
    if !chrome.subtitle.is_empty() {
        header_labels.push(
            TextBlock::new()
                .text(chrome.subtitle)
                .font_size(12.0)
                .opacity(0.68)
                .is_text_selection_enabled(true)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        );
    }

    let header_left: View = match chrome.leading {
        Some(lead) => Grid::new()
            .grid_column(0)
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((lead,)),
                StackPanel::new()
                    .grid_column(1)
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .keyed_children(
                        header_labels
                            .into_iter()
                            .enumerate()
                            .map(|(index, view)| KeyedView::new(index, view)),
                    ),
            )),
        None => StackPanel::new()
            .grid_column(0)
            .spacing(2.0)
            .vertical_alignment(VerticalAlignment::Center)
            .keyed_children(
                header_labels
                    .into_iter()
                    .enumerate()
                    .map(|(index, view)| KeyedView::new(index, view)),
            ),
    };

    let header = Grid::new()
        .grid_row(0)
        .columns([GridLength::STAR, GridLength::Auto])
        .column_spacing(12.0)
        .margin(Thickness::new(20.0, 16.0, 14.0, 14.0))
        .children((
            header_left,
            StackPanel::new()
                .grid_column(1)
                .vertical_alignment(VerticalAlignment::Top)
                .children((icon_button(
                    GLYPH_CLOSE,
                    12.0,
                    32.0,
                    make_close(),
                    chrome.close_tooltip,
                ),)),
        ));

    let divider = Border::new()
        .grid_row(1)
        .height(1.0)
        .background(ThemeBrush::CardStroke);

    let sticky_slot: View = match chrome.sticky_bar {
        Some(bar) => StackPanel::new().grid_row(2).children((
            Border::new()
                .background(ThemeBrush::CardBackground)
                .padding(Thickness::xy(20.0, 10.0))
                .content(bar),
            Border::new().height(1.0).background(ThemeBrush::CardStroke),
        )),
        None => StackPanel::new().grid_row(2).into(),
    };

    let scroll = ScrollViewer::new()
        .grid_row(3)
        .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
        .content(
            StackPanel::new()
                .margin(Thickness::new(20.0, 16.0, 20.0, 24.0))
                .children((settings_ui::stack(chrome.body, 10.0),)),
        );

    let panel = Border::new()
        .grid_column(1)
        .width(DRAWER_WIDTH)
        .vertical_alignment(VerticalAlignment::Stretch)
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::new(1.0, 0.0, 0.0, 0.0))
        .corner_radius(CornerRadius::new(12.0, 0.0, 0.0, 12.0))
        .content(
            Grid::new()
                .rows([
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::STAR,
                ])
                .children((header, divider, sticky_slot, scroll)),
        );

    Grid::new()
        .columns([GridLength::STAR, GridLength::Auto])
        .children((scrim, panel))
}
