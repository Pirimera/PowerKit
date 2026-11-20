//! MediaRenamer workspace: sticky header, template card, two-row filter/action toolbar
//! and the virtualized preview table. Files and folders can be dropped anywhere on the pane.
use super::super::media_renamer::{Action, Busy, DrawerView, Notice, RowView, StateFilter};
use super::super::settings_ui;
use super::view_helpers::{
    PillTone, accent_bar, button_row, fill_item_width, hairline, icon_button, icon_label,
    inset_box, sized_icon, status_pill,
};
use super::{Message, Shell};
use crate::locale::RenameStrings;
use crate::media_renamer::{
    Kind,
    plan::State,
    template::{self, Fields, Template},
    time::WallClock,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::rc::Rc;
use windows_reactor::*;

const COL_CHECK_WIDTH: f64 = 34.0;
const COL_TIME_WIDTH: f64 = 166.0;
const COL_STATE_WIDTH: f64 = 148.0;
const FILTER_STATE_WIDTH: f64 = 204.0;
const ROW_TILE_SIZE: f64 = 32.0;

const GLYPH_ADD: &str = "\u{E710}";
const GLYPH_FOLDER: &str = "\u{E8B7}";
const GLYPH_HISTORY: &str = "\u{E81C}";
const GLYPH_SETTINGS: &str = "\u{E713}";
const GLYPH_CLEAR: &str = "\u{E711}";
const GLYPH_DELETE: &str = "\u{E74D}";
const GLYPH_RENAME: &str = "\u{E8AC}";
const GLYPH_DROP: &str = "\u{E896}";
const GLYPH_SEARCH: &str = "\u{E721}";
const GLYPH_PRESETS: &str = "\u{E8FD}";
const GLYPH_RESET: &str = "\u{E7A7}";
const GLYPH_SYNC: &str = "\u{E72C}";
const GLYPH_CHECK: &str = "\u{E73E}";
const GLYPH_ERROR: &str = "\u{EA39}";
const GLYPH_WARNING: &str = "\u{E7BA}";
const GLYPH_INFO: &str = "\u{E946}";
const GLYPH_PHOTO: &str = "\u{EB9F}";
const GLYPH_CAMERA: &str = "\u{E722}";
const GLYPH_VIDEO: &str = "\u{E714}";
const GLYPH_SIDECAR: &str = "\u{E8EC}";

fn table_columns() -> [GridLength; 5] {
    [
        GridLength::Pixel(COL_CHECK_WIDTH),
        GridLength::STAR,
        GridLength::STAR,
        GridLength::Pixel(COL_TIME_WIDTH),
        GridLength::Pixel(COL_STATE_WIDTH),
    ]
}

fn media_kind_glyph(kind: Kind) -> &'static str {
    match kind {
        Kind::Jpeg | Kind::Heif | Kind::Png | Kind::WebP => GLYPH_PHOTO,
        Kind::Raw => GLYPH_CAMERA,
        Kind::Video | Kind::Avi => GLYPH_VIDEO,
        Kind::Sidecar => GLYPH_SIDECAR,
    }
}

/// Framed media-kind tile shared by preview rows and the Details drawer.
pub(super) fn media_kind_tile(kind: Kind, tile_size: f64) -> View {
    let inner = (tile_size - 16.0).max(14.0);
    let corner = if tile_size >= 36.0 { 8.0 } else { 7.0 };
    Border::new()
        .width(tile_size)
        .height(tile_size)
        .corner_radius(CornerRadius::uniform(corner))
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .content(sized_icon(media_kind_glyph(kind), inner))
}

/// Semantic pill badge for a row's or group's [`State`].
pub(super) fn state_pill_view(r: &RenameStrings, state: State, selected: bool) -> View {
    let label = r.state(state);
    match state {
        State::Ready if selected => status_pill(Some(GLYPH_CHECK), &label, PillTone::Success),
        State::Ready | State::Pending | State::Unchanged => {
            status_pill(None, &label, PillTone::Neutral)
        }
        State::NoDate => status_pill(None, &label, PillTone::Warning),
        State::OnlineOnly | State::Unreadable | State::CodecMissing(_) | State::Conflict => {
            status_pill(Some(GLYPH_ERROR), &label, PillTone::Critical)
        }
    }
}

/// What a preview row needs from the page. It owns its sender and copies, so the virtualized
/// list can build rows after `view` returns.
#[derive(Clone)]
struct RowScope {
    sender: LocalSender<Message>,
    r: &'static RenameStrings,
    /// Rows take input only while no scan, apply or undo runs.
    idle: bool,
}

impl RowScope {
    fn message(&self, message: Message) -> Callback<()> {
        self.sender.message(message)
    }

    fn callback<T>(&self, map: impl Fn(T) -> Message + 'static) -> Callback<T> {
        self.sender.callback(map)
    }
}

impl Shell {
    fn rename_strings(&self) -> &'static RenameStrings {
        self.t().rename
    }

    pub(super) fn media_renamer_pane(&self, context: &ViewContext<Self>) -> View {
        let mut top = vec![self.media_renamer_header(context)];
        if let Some(progress) = self.media_renamer_progress_card(context) {
            top.push(progress);
        }
        if let Some(notice) = self.media_renamer_notice_strip(context) {
            top.push(notice);
        }
        if let Some(notes) = self.media_renamer_notes_strip() {
            top.push(notes);
        }
        top.push(self.media_renamer_template_bar(context));
        top.push(self.media_renamer_toolbar(context));
        if self.snapshot.media_renamer.summary.groups > 0 {
            top.push(self.media_renamer_table_header(context));
        }
        let sticky_top = StackPanel::new()
            .grid_row(0)
            .margin(Thickness::new(24.0, 14.0, 24.0, 0.0))
            .children((settings_ui::stack(top, 8.0),));
        let table_scroll = ScrollViewer::new()
            .grid_row(1)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            // The virtualized list is the scroller's direct content so it realizes only the
            // rows in view.
            .content(self.media_renamer_table_body(context));
        let main = Border::new()
            .background(Color::argb(0, 0, 0, 0))
            .drop_policy(
                DragDropPolicy::new().storage_items(DragDropAction::new(DragDropOperation::Copy)),
            )
            .on_drop(context.callback(|data: DroppedData| {
                match data {
                    DroppedData::StorageItems(items) => Message::MediaRenamer(Action::Add(
                        items
                            .into_iter()
                            .map(|item| PathBuf::from(item.path))
                            .collect(),
                    )),
                    _ => Message::Noop,
                }
            }))
            .content(
                Grid::new()
                    .rows([GridLength::Auto, GridLength::STAR])
                    .children((sticky_top, table_scroll)),
            );
        let mut layers = vec![KeyedView::new("main", main)];
        if let Some(drawer) = self.media_renamer_drawer_overlay(context) {
            layers.push(KeyedView::new("drawer", drawer));
        }
        Grid::new().keyed_children(layers)
    }

    fn media_renamer_header(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let working = matches!(s.busy, Busy::Applying { .. } | Busy::Undoing { .. });
        let history_open = matches!(s.drawer, DrawerView::History(_));
        let settings_open = matches!(s.drawer, DrawerView::Settings);
        let message = |action: Action| context.message(Message::MediaRenamer(action));

        let mut title_items: Vec<View> = vec![settings_ui::heading(r.title, 26.0).into()];
        if let Some(pill) = self.media_renamer_status_pill() {
            title_items.push(pill);
        }
        let title_row = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(10.0)
            .vertical_alignment(VerticalAlignment::Center)
            .keyed_children(
                title_items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| KeyedView::new(index, item)),
            );

        let subtitle = self.media_renamer_header_subtitle();

        let separator = Border::new()
            .width(1.0)
            .height(20.0)
            .background(ThemeBrush::CardStroke)
            .vertical_alignment(VerticalAlignment::Center)
            .margin(Thickness::xy(4.0, 0.0));

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .spacing(3.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((title_row, subtitle)),
                StackPanel::new()
                    .grid_column(1)
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        Button::new()
                            .is_enabled(!working)
                            .on_click(message(Action::AddFiles))
                            .content(icon_label(GLYPH_ADD, r.add_files)),
                        Button::new()
                            .is_enabled(!working)
                            .on_click(message(Action::AddFolder))
                            .content(icon_label(GLYPH_FOLDER, r.add_folder)),
                        separator,
                        Button::new()
                            .style(if history_open {
                                ButtonStyle::Accent
                            } else {
                                ButtonStyle::Subtle
                            })
                            .on_click(message(Action::ToggleHistory))
                            .content(icon_label(GLYPH_HISTORY, r.history)),
                        Button::new()
                            .style(if settings_open {
                                ButtonStyle::Accent
                            } else {
                                ButtonStyle::Subtle
                            })
                            .on_click(message(Action::ToggleSettings))
                            .content(icon_label(GLYPH_SETTINGS, r.settings)),
                    )),
            ))
    }

    fn media_renamer_status_pill(&self) -> Option<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let spinner_pill = |text: String| -> View {
            Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(12.0))
                .padding(Thickness::xy(10.0, 3.0))
                .vertical_alignment(VerticalAlignment::Center)
                .content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(6.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            ProgressRing::new()
                                .width(12.0)
                                .height(12.0)
                                .is_active(true)
                                .is_indeterminate(true),
                            TextBlock::new()
                                .text(text)
                                .font_size(11.5)
                                .opacity(0.85)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                )
        };
        match s.busy {
            Busy::Loading => Some(spinner_pill(r.loading.to_string())),
            Busy::Scanning { done, total } => Some(spinner_pill(RenameStrings::progress(
                r.scanning, done, total,
            ))),
            Busy::Applying { done, total } => Some(spinner_pill(RenameStrings::progress(
                r.applying, done, total,
            ))),
            Busy::Undoing { done, total } => Some(spinner_pill(RenameStrings::progress(
                r.undoing, done, total,
            ))),
            Busy::Idle if s.summary.ready > 0 => Some(
                Border::new()
                    .background(ThemeBrush::CardBackground)
                    .border_brush(ThemeBrush::CardStroke)
                    .border_thickness(Thickness::uniform(1.0))
                    .corner_radius(CornerRadius::uniform(12.0))
                    .padding(Thickness::xy(10.0, 3.0))
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(5.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                sized_icon(GLYPH_CHECK, 11.0),
                                TextBlock::new()
                                    .text(format!("{}: {}", r.filter_ready, s.summary.ready))
                                    .font_size(11.5)
                                    .opacity(0.85)
                                    .vertical_alignment(VerticalAlignment::Center),
                            )),
                    ),
            ),
            Busy::Idle => None,
        }
    }

    fn media_renamer_header_subtitle(&self) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        if s.summary.groups == 0 {
            return TextBlock::new()
                .text(r.lead)
                .font_size(12.5)
                .opacity(0.72)
                .text_wrapping(TextWrapping::Wrap)
                .into();
        }
        let mut parts = vec![
            format!("{} {}", s.summary.groups, r.summary_items),
            format!("{} {}", s.summary.files, r.summary_files),
            format!("{}: {}", r.filter_ready, s.summary.ready),
        ];
        if s.summary.collided > 0 {
            parts.push(format!("{}: {}", r.filter_collision, s.summary.collided));
        }
        let base: View = TextBlock::new()
            .text(parts.join("  ·  "))
            .font_size(12.5)
            .opacity(0.72)
            .vertical_alignment(VerticalAlignment::Center)
            .into();
        if s.summary.failed == 0 {
            return base;
        }
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                base,
                TextBlock::new()
                    .text("·")
                    .font_size(12.5)
                    .opacity(0.45)
                    .vertical_alignment(VerticalAlignment::Center),
                TextBlock::new()
                    .text(format!("{}: {}", r.filter_problem, s.summary.failed))
                    .font_size(12.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(ThemeBrush::SystemCritical)
                    .vertical_alignment(VerticalAlignment::Center),
            ))
    }

    /// Cancellable progress card while a scan, rename or undo runs.
    fn media_renamer_progress_card(&self, context: &ViewContext<Self>) -> Option<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let (label, done, total, cancel_action, extra) = match s.busy {
            Busy::Idle | Busy::Loading => return None,
            Busy::Scanning { done, total } => (
                r.scanning,
                done,
                total,
                Action::CancelScan,
                (s.queued > 0).then(|| RenameStrings::count(r.queued, s.queued)),
            ),
            Busy::Applying { done, total } => (r.applying, done, total, Action::CancelWork, None),
            Busy::Undoing { done, total } => (r.undoing, done, total, Action::CancelWork, None),
        };

        let mut text_rows: Vec<View> = vec![
            TextBlock::new()
                .text(RenameStrings::progress(label, done, total))
                .font_size(13.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .into(),
            ProgressBar::new()
                .maximum(total.max(1) as f64)
                .value(done as f64)
                .is_indeterminate(total == 0)
                .into(),
        ];
        if let Some(extra) = extra {
            text_rows.push(
                TextBlock::new()
                    .text(extra)
                    .font_size(11.5)
                    .opacity(0.68)
                    .into(),
            );
        }

        Some(
            Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(14.0, 10.0))
                .content(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                        .column_spacing(12.0)
                        .children((
                            accent_bar(24.0, ThemeBrush::Accent.into()).grid_column(0),
                            StackPanel::new()
                                .grid_column(1)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((settings_ui::stack(text_rows, 5.0),)),
                            StackPanel::new()
                                .grid_column(2)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((Button::new()
                                    .on_click(context.message(Message::MediaRenamer(cancel_action)))
                                    .content(r.cancel),)),
                        )),
                ),
        )
    }

    /// Semantic outcome or warning banner above the template card.
    fn media_renamer_notice_strip(&self, context: &ViewContext<Self>) -> Option<View> {
        let s = &self.snapshot.media_renamer;
        let notice = s.notice.as_ref()?;
        let r = self.rename_strings();
        let message = |action: Action| context.message(Message::MediaRenamer(action));

        let critical = notice_is_critical(notice);
        let warning = notice_is_warning(notice);
        let (bg, border, leading): (Brush, Brush, View) = if critical {
            (
                ThemeBrush::SystemCriticalBackground.into(),
                ThemeBrush::SystemCritical.into(),
                sized_icon(GLYPH_ERROR, 15.0),
            )
        } else if warning {
            (
                ThemeBrush::CardBackground.into(),
                ThemeBrush::CardStroke.into(),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        accent_bar(20.0, Color::rgb(0x9A, 0x67, 0x00).into()),
                        sized_icon(GLYPH_WARNING, 15.0),
                    )),
            )
        } else {
            (
                ThemeBrush::CardBackground.into(),
                ThemeBrush::CardStroke.into(),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        accent_bar(18.0, Color::rgb(0x2D, 0x8A, 0x4E).into()),
                        sized_icon(GLYPH_CHECK, 14.0),
                    )),
            )
        };

        let mut controls: Vec<View> = Vec::new();
        if s.busy == Busy::Idle {
            if matches!(notice, Notice::Stale) {
                controls.push(
                    Button::new()
                        .style(ButtonStyle::Accent)
                        .on_click(message(Action::Refresh))
                        .content(icon_label(GLYPH_SYNC, r.refresh)),
                );
            }
            controls.push(icon_button(
                GLYPH_CLEAR,
                11.0,
                30.0,
                message(Action::DismissNotice),
                r.close,
            ));
        }

        Some(
            Border::new()
                .background(bg)
                .border_brush(border)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(14.0, 9.0))
                .content(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                        .column_spacing(12.0)
                        .children((
                            StackPanel::new()
                                .grid_column(0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((leading,)),
                            StackPanel::new()
                                .grid_column(1)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((settings_ui::stack(
                                    notice_lines(r, notice, critical),
                                    3.0,
                                ),)),
                            StackPanel::new()
                                .grid_column(2)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((button_row(controls),)),
                        )),
                ),
        )
    }

    /// Compact informational strip when unsupported/hidden files were skipped or the batch hit
    /// the 20,000-file ceiling.
    fn media_renamer_notes_strip(&self) -> Option<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let mut notes: Vec<String> = Vec::new();
        if s.skipped > 0 {
            notes.push(RenameStrings::count(r.skipped_files, s.skipped));
        }
        if s.batch_capped {
            notes.push(r.batch_capped.to_string());
        }
        if notes.is_empty() {
            return None;
        }
        Some(
            Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(12.0, 7.0))
                .content(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR])
                        .column_spacing(10.0)
                        .children((
                            StackPanel::new()
                                .grid_column(0)
                                .opacity(0.68)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((sized_icon(GLYPH_INFO, 13.0),)),
                            TextBlock::new()
                                .grid_column(1)
                                .text(notes.join("  ·  "))
                                .font_size(12.0)
                                .opacity(0.80)
                                .text_wrapping(TextWrapping::Wrap)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                ),
        )
    }

    fn media_renamer_template_bar(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let current = s.template.clone();

        let header = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .opacity(0.72)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((sized_icon(GLYPH_RENAME, 13.0),)),
                TextBlock::new()
                    .grid_column(1)
                    .text(r.template)
                    .font_size(13.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center),
                Button::new()
                    .grid_column(2)
                    .style(ButtonStyle::Subtle)
                    .is_enabled(s.template != s.default_template)
                    .on_click(context.message(Message::MediaRenamer(Action::SetTemplate(
                        s.default_template.clone(),
                    ))))
                    .content(icon_label(GLYPH_RESET, r.reset_template)),
            ));

        let presets = Button::new()
            .grid_column(1)
            .content(icon_label(GLYPH_PRESETS, r.presets))
            .menu(Menu::new(
                template::PRESETS
                    .iter()
                    .map(|preset| MenuItem::item(*preset, *preset))
                    .collect::<Vec<_>>(),
                context
                    .callback(|picked: String| Message::MediaRenamer(Action::SetTemplate(picked))),
            ));
        let tokens = Button::new()
            .grid_column(2)
            .content(icon_label(GLYPH_ADD, r.tokens))
            .tooltip(r.tokens_hint)
            .menu(Menu::new(
                template::CHIPS
                    .iter()
                    .map(|chip| MenuItem::item(*chip, *chip))
                    .collect::<Vec<_>>(),
                context.callback(move |picked: String| {
                    Message::MediaRenamer(Action::SetTemplate(format!("{current}{picked}")))
                }),
            ));
        let input = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(s.template.clone())
                    .placeholder_text(template::DEFAULT)
                    .on_text_changed(context.callback(Message::MediaRenamerTemplate)),
                presets,
                tokens,
            ));

        let feedback: View = match &s.template_error {
            Some(error) => Border::new()
                .background(ThemeBrush::SystemCriticalBackground)
                .border_brush(ThemeBrush::SystemCritical)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(6.0))
                .padding(Thickness::xy(10.0, 6.0))
                .content(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR])
                        .column_spacing(8.0)
                        .children((
                            StackPanel::new()
                                .grid_column(0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((sized_icon(GLYPH_ERROR, 13.0),)),
                            TextBlock::new()
                                .grid_column(1)
                                .text(r.template_error(error))
                                .font_size(12.5)
                                .foreground(ThemeBrush::SystemCritical)
                                .text_wrapping(TextWrapping::Wrap)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                ),
            None => {
                let (before, after) = match s.rows.iter().find(|row| !row.new_name.is_empty()) {
                    Some(row) => (row.current.clone(), row.new_name.clone()),
                    None => sample_preview(&s.template, s.lowercase_ext),
                };
                inset_box(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR])
                        .column_spacing(8.0)
                        .children((
                            TextBlock::new()
                                .grid_column(0)
                                .text(format!("{}:", r.preview_example))
                                .font_size(12.0)
                                .opacity(0.62)
                                .vertical_alignment(VerticalAlignment::Center),
                            StackPanel::new()
                                .grid_column(1)
                                .orientation(Orientation::Horizontal)
                                .spacing(6.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((
                                    TextBlock::new()
                                        .text(before)
                                        .font_size(12.5)
                                        .opacity(0.75)
                                        .text_trimming(TextTrimming::CharacterEllipsis)
                                        .max_lines(1),
                                    TextBlock::new().text("→").font_size(12.5).opacity(0.55),
                                    TextBlock::new()
                                        .text(after)
                                        .font_size(12.5)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .foreground(ThemeBrush::AccentText)
                                        .text_trimming(TextTrimming::CharacterEllipsis)
                                        .max_lines(1),
                                )),
                        )),
                )
            }
        };

        let toggle_cell =
            |column: i32, label: &'static str, on: bool, action: fn(bool) -> Action| {
                Grid::new()
                    .grid_column(column)
                    .columns([GridLength::STAR, GridLength::Auto])
                    .column_spacing(10.0)
                    .children((
                        TextBlock::new()
                            .grid_column(0)
                            .text(label)
                            .font_size(12.5)
                            .text_wrapping(TextWrapping::Wrap)
                            .vertical_alignment(VerticalAlignment::Center),
                        StackPanel::new()
                            .grid_column(1)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((settings_ui::toggle_switch(
                                ToggleSwitch::new().is_on(on).min_width(0.0).on_toggled(
                                    context.callback(move |on| Message::MediaRenamer(action(on))),
                                ),
                            ),)),
                    ))
            };
        let options = Grid::new()
            .columns([GridLength::STAR, GridLength::STAR])
            .column_spacing(18.0)
            .children((
                toggle_cell(0, r.recurse, s.recurse, Action::SetRecurse),
                toggle_cell(
                    1,
                    r.mtime_fallback,
                    s.mtime_fallback,
                    Action::SetMtimeFallback,
                ),
            ));

        let mut rows = vec![header, input, feedback, hairline(), options];
        if s.mtime_fallback {
            rows.push(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        StackPanel::new()
                            .opacity(0.65)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((sized_icon(GLYPH_INFO, 12.0),)),
                        TextBlock::new()
                            .text(r.mtime_fallback_hint)
                            .font_size(11.5)
                            .opacity(0.72)
                            .text_wrapping(TextWrapping::Wrap),
                    )),
            );
        }

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .padding(Thickness::xy(14.0, 11.0))
            .content(settings_ui::stack(rows, 8.0))
    }

    fn media_renamer_toolbar(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let message = |action: Action| context.message(Message::MediaRenamer(action));
        let has_items = s.summary.groups > 0;

        let labels: Vec<String> = StateFilter::ALL
            .iter()
            .enumerate()
            .map(|(index, filter)| {
                let base = filter_label(r, *filter);
                if has_items {
                    format!("{base} ({})", s.filter_counts[index])
                } else {
                    base.to_string()
                }
            })
            .collect();
        let current = s.state_filter;
        let state_combo = ComboBox::new()
            .grid_column(1)
            .width(FILTER_STATE_WIDTH)
            .min_width(FILTER_STATE_WIDTH)
            .items_source(labels)
            .selected_index(current.index())
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                match index.and_then(StateFilter::from_index) {
                    Some(next) if next != current => {
                        Message::MediaRenamer(Action::SetStateFilter(next))
                    }
                    _ => Message::Noop,
                }
            }));

        let clear_filter: View = if s.filter_text.is_empty() {
            StackPanel::new().grid_column(2).into()
        } else {
            StackPanel::new()
                .grid_column(2)
                .vertical_alignment(VerticalAlignment::Center)
                .children((icon_button(
                    GLYPH_CLEAR,
                    11.0,
                    32.0,
                    message(Action::SetFilterText(String::new())),
                    r.close,
                ),))
        };
        let search_box = Grid::new()
            .grid_column(0)
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(6.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::new(4.0, 0.0, 2.0, 0.0))
                    .opacity(0.6)
                    .children((sized_icon(GLYPH_SEARCH, 13.0),)),
                TextBox::new()
                    .grid_column(1)
                    .text(s.filter_text.clone())
                    .placeholder_text(r.filter_hint)
                    .on_text_changed(context.callback(Message::MediaRenamerFilter)),
                clear_filter,
            ));
        let filters_row = Grid::new()
            .columns([GridLength::STAR, GridLength::Pixel(FILTER_STATE_WIDTH)])
            .column_spacing(8.0)
            .children((search_box, state_combo));

        let pill = |filter: StateFilter, count: usize, always_count: bool| -> View {
            let active = s.state_filter == filter;
            let base = filter_label(r, filter);
            let label = if (always_count || count > 0) && has_items {
                format!("{base} ({count})")
            } else {
                base.to_string()
            };
            Button::new()
                .style(if active {
                    ButtonStyle::Accent
                } else {
                    ButtonStyle::Subtle
                })
                .on_click(message(Action::SetStateFilter(filter)))
                .content(
                    TextBlock::new()
                        .text(label)
                        .font_size(12.5)
                        .font_weight(if active {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::NORMAL
                        }),
                )
        };
        let mut pill_items = vec![
            pill(StateFilter::All, s.filter_counts[0], false),
            pill(StateFilter::Ready, s.filter_counts[1], false),
            pill(StateFilter::Problem, s.filter_counts[5], false),
        ];
        // When a secondary state is chosen in the dropdown, surface it in the pill bar too.
        if matches!(
            s.state_filter,
            StateFilter::Unchanged | StateFilter::NoDate | StateFilter::Collision
        ) {
            let index = s.state_filter.index();
            pill_items.push(pill(s.state_filter, s.filter_counts[index], true));
        }
        let status_pills = Border::new()
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .padding(Thickness::uniform(3.0))
            .horizontal_alignment(HorizontalAlignment::Left)
            .content(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(2.0)
                    .keyed_children(
                        pill_items
                            .into_iter()
                            .enumerate()
                            .map(|(index, item)| KeyedView::new(index, item)),
                    ),
            );

        let idle = s.busy == Busy::Idle;
        let apply_label = if s.summary.ready > 0 {
            format!("{} ({})", r.apply, s.summary.ready)
        } else {
            r.apply.to_string()
        };
        let actions = button_row(vec![
            Button::new()
                .style(ButtonStyle::Accent)
                .is_enabled(s.can_apply)
                .on_click(message(Action::Apply))
                .content(icon_label(GLYPH_RENAME, &apply_label)),
            Button::new()
                .is_enabled(idle && has_items)
                .on_click(message(Action::Clear))
                .content(icon_label(GLYPH_DELETE, r.clear)),
        ]);

        let actions_row = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((status_pills,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((actions,)),
            ));

        let row_divider = Border::new()
            .height(1.0)
            .background(ThemeBrush::CardStroke)
            .opacity(0.55);

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .padding(Thickness::xy(12.0, 9.0))
            .content(settings_ui::stack(
                vec![filters_row, row_divider.into(), actions_row],
                7.0,
            ))
    }

    fn media_renamer_table_header(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let header = |label: &str| {
            TextBlock::new()
                .text(label)
                .font_size(12.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .opacity(0.72)
                .vertical_alignment(VerticalAlignment::Center)
        };
        let current_label = if s.visible == s.summary.groups {
            format!("{} ({})", r.col_current, s.visible)
        } else {
            format!("{} ({}/{})", r.col_current, s.visible, s.summary.groups)
        };
        Border::new()
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::new(8.0, 8.0, 0.0, 0.0))
            .padding(Thickness::xy(14.0, 7.0))
            .content(
                Grid::new()
                    .columns(table_columns())
                    .column_spacing(12.0)
                    .children((
                        CheckBox::new()
                            .grid_column(0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .is_enabled(s.any_selectable && s.busy == Busy::Idle)
                            .is_checked(s.all_visible_selected)
                            .on_is_checked_changed(context.callback(|checked| {
                                Message::MediaRenamer(Action::SetSelectAllVisible(checked))
                            })),
                        header(&current_label).grid_column(1),
                        header(r.col_new).grid_column(2),
                        header(r.col_time).grid_column(3),
                        header(r.col_state).grid_column(4),
                    )),
            )
    }

    /// The preview list is the scroller's direct content, so the virtualized list realizes
    /// only the rows in view; a list of any length stays responsive.
    fn media_renamer_table_body(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let working = matches!(s.busy, Busy::Applying { .. } | Busy::Undoing { .. });
        let margin = Thickness::new(24.0, 0.0, 24.0, 20.0);

        if s.rows.is_empty() && s.summary.groups > 0 {
            return Border::new()
                .margin(margin)
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
                .corner_radius(CornerRadius::new(0.0, 0.0, 8.0, 8.0))
                .padding(Thickness::xy(24.0, 38.0))
                .content(
                    StackPanel::new()
                        .spacing(12.0)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .children((
                            StackPanel::new()
                                .opacity(0.45)
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .children((sized_icon(GLYPH_SEARCH, 30.0),)),
                            TextBlock::new()
                                .text(r.filter_empty)
                                .font_size(14.0)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .horizontal_alignment(HorizontalAlignment::Center),
                            Button::new()
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .on_click(
                                    context.message(Message::MediaRenamer(Action::ClearFilter)),
                                )
                                .content(icon_label(GLYPH_CLEAR, r.clear_filter)),
                        )),
                );
        }

        if s.rows.is_empty() {
            return Border::new()
                .margin(Thickness::new(24.0, 8.0, 24.0, 20.0))
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(24.0, 44.0))
                .content(
                    StackPanel::new()
                        .spacing(14.0)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .children((
                            Border::new()
                                .width(52.0)
                                .height(52.0)
                                .corner_radius(CornerRadius::uniform(26.0))
                                .background(ThemeBrush::SolidBackground)
                                .border_brush(ThemeBrush::CardStroke)
                                .border_thickness(Thickness::uniform(1.0))
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .content(
                                    StackPanel::new()
                                        .opacity(0.62)
                                        .horizontal_alignment(HorizontalAlignment::Center)
                                        .vertical_alignment(VerticalAlignment::Center)
                                        .children((sized_icon(GLYPH_DROP, 24.0),)),
                                ),
                            StackPanel::new()
                                .spacing(4.0)
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .children((
                                    TextBlock::new()
                                        .text(r.empty)
                                        .font_size(15.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .horizontal_alignment(HorizontalAlignment::Center),
                                    TextBlock::new()
                                        .text(r.drop_hint)
                                        .font_size(12.5)
                                        .opacity(0.72)
                                        .text_wrapping(TextWrapping::Wrap)
                                        .horizontal_alignment(HorizontalAlignment::Center),
                                )),
                            StackPanel::new()
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .children((button_row(vec![
                                    Button::new()
                                        .style(ButtonStyle::Accent)
                                        .is_enabled(!working)
                                        .on_click(
                                            context
                                                .message(Message::MediaRenamer(Action::AddFiles)),
                                        )
                                        .content(icon_label(GLYPH_ADD, r.add_files)),
                                    Button::new()
                                        .is_enabled(!working)
                                        .on_click(
                                            context
                                                .message(Message::MediaRenamer(Action::AddFolder)),
                                        )
                                        .content(icon_label(GLYPH_FOLDER, r.add_folder)),
                                ]),)),
                        )),
                );
        }

        // Rows are keyed by group and name; the revision covers every shown field, so a
        // selection or state change re-realizes the rows instead of reusing stale ones.
        let count = s.rows.len();
        let mut hasher = DefaultHasher::new();
        s.rows.hash(&mut hasher);
        let key_revision = hasher.finish();
        let scope = RowScope {
            sender: context.sender(),
            r,
            idle: s.busy == Busy::Idle,
        };
        let rows = Rc::clone(&s.rows);
        let key_rows = Rc::clone(&rows);
        let source = VirtualSource::new(
            key_revision,
            count,
            move |index: usize| format!("{}|{}", key_rows[index].group, key_rows[index].current),
            move |index: usize| {
                fill_item_width(Self::media_renamer_row(
                    &scope,
                    &rows[index],
                    index + 1 == count,
                ))
            },
        );
        ItemsRepeater::new()
            .virtual_source(source)
            .margin(margin)
            .into()
    }

    fn media_renamer_row(scope: &RowScope, row: &RowView, last: bool) -> View {
        let r = scope.r;
        let group = row.group;
        let active_rename = row.state == State::Ready && row.selected;

        // Column 1: 32px media-kind tile + subtle button with current name, companion pill and
        // folder caption. Keep the name TextBlock in a STAR column so ellipsis measures cleanly.
        let name_block = TextBlock::new()
            .text(row.current.as_str())
            .font_size(13.5)
            .font_weight(FontWeight::SEMI_BOLD)
            .text_trimming(TextTrimming::CharacterEllipsis)
            .max_lines(1)
            .vertical_alignment(VerticalAlignment::Center);
        let title_line: View = if row.members > 1 {
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    name_block.grid_column(0),
                    StackPanel::new()
                        .grid_column(1)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((status_pill(
                            None,
                            &RenameStrings::count(r.companions, row.members - 1),
                            PillTone::Neutral,
                        ),)),
                ))
        } else {
            name_block.into()
        };
        let name_stack = StackPanel::new().spacing(2.0).children((
            title_line,
            TextBlock::new()
                .text(row.folder.as_str())
                .font_size(11.5)
                .opacity(0.60)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1),
        ));
        let open_details = Button::new()
            .grid_column(1)
            .style(ButtonStyle::Subtle)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Left)
            .on_click(scope.message(Message::MediaRenamer(Action::OpenDetails(group))))
            .content(name_stack)
            .tooltip(r.details);
        let current_cell = Grid::new()
            .grid_column(1)
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((media_kind_tile(row.kind, ROW_TILE_SIZE),)),
                open_details,
            ));

        // Column 2: New name, accented when selected & ready so changes pop at a glance.
        let new_cell: View = if row.state == State::Ready && !row.new_name.is_empty() {
            let text = TextBlock::new()
                .grid_column(2)
                .text(format!("→ {}", row.new_name))
                .font_size(13.0)
                .font_weight(if active_rename {
                    FontWeight::SEMI_BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1)
                .vertical_alignment(VerticalAlignment::Center);
            if active_rename {
                text.foreground(ThemeBrush::AccentText).into()
            } else {
                text.opacity(0.68).into()
            }
        } else if row.state == State::Unchanged {
            TextBlock::new()
                .grid_column(2)
                .text(row.current.as_str())
                .font_size(12.5)
                .opacity(0.55)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1)
                .vertical_alignment(VerticalAlignment::Center)
                .into()
        } else {
            TextBlock::new()
                .grid_column(2)
                .text("—")
                .font_size(12.5)
                .opacity(0.50)
                .vertical_alignment(VerticalAlignment::Center)
                .into()
        };

        // Column 3: Capture time + subtle source caption.
        let time_cell = StackPanel::new()
            .grid_column(3)
            .spacing(1.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                TextBlock::new()
                    .text(if row.time.is_empty() {
                        "—"
                    } else {
                        row.time.as_str()
                    })
                    .font_size(12.5)
                    .opacity(if row.time.is_empty() { 0.50 } else { 0.90 })
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    .max_lines(1),
                TextBlock::new()
                    .text(
                        row.source
                            .map(|source| r.source(source))
                            .unwrap_or_default(),
                    )
                    .font_size(11.0)
                    .opacity(0.58)
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    .max_lines(1),
            ));

        // Column 4: Semantic status pill (+ collision badge when disambiguated).
        let state_cell: View = if row.state == State::Pending {
            StackPanel::new()
                .grid_column(4)
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    ProgressRing::new()
                        .width(12.0)
                        .height(12.0)
                        .is_active(true)
                        .is_indeterminate(true),
                    TextBlock::new()
                        .text(r.state_pending)
                        .font_size(12.0)
                        .opacity(0.75)
                        .vertical_alignment(VerticalAlignment::Center),
                ))
        } else if row.collided {
            StackPanel::new()
                .grid_column(4)
                .spacing(3.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    state_pill_view(r, row.state, row.selected),
                    status_pill(None, r.collided, PillTone::Warning),
                ))
        } else {
            StackPanel::new()
                .grid_column(4)
                .vertical_alignment(VerticalAlignment::Center)
                .children((state_pill_view(r, row.state, row.selected),))
        };

        let row_grid = Grid::new()
            .columns(table_columns())
            .column_spacing(12.0)
            .children((
                CheckBox::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .is_enabled(row.selectable && scope.idle)
                    .is_checked(active_rename)
                    .on_is_checked_changed(scope.callback(move |selected: bool| {
                        Message::MediaRenamer(Action::SetSelected(group, selected))
                    })),
                current_cell,
                new_cell,
                time_cell,
                state_cell,
            ));

        let mut layers: Vec<KeyedView> = vec![KeyedView::new(
            "row-content",
            Border::new()
                .padding(Thickness::xy(14.0, 8.0))
                .content(row_grid),
        )];
        if active_rename {
            layers.push(KeyedView::new(
                "selected-bar",
                Border::new()
                    .width(3.0)
                    .height(28.0)
                    .corner_radius(CornerRadius::uniform(2.0))
                    .background(ThemeBrush::Accent)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::new(3.0, 0.0, 0.0, 0.0)),
            ));
        }

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
            .corner_radius(if last {
                CornerRadius::new(0.0, 0.0, 8.0, 8.0)
            } else {
                CornerRadius::uniform(0.0)
            })
            .content(Grid::new().keyed_children(layers))
    }
}

/// Renders a synthetic example pair when the workspace has no files yet, so editing the
/// template always gives live feedback.
fn sample_preview(template_text: &str, lowercase_ext: bool) -> (String, String) {
    let ext = if lowercase_ext { ".jpg" } else { ".JPG" };
    let Ok(parsed) = Template::parse(template_text) else {
        return ("IMG_1024.JPG".into(), format!("2026-05-01_14-03-22{ext}"));
    };
    let clock = WallClock::new(2026, 5, 1, 14, 3, 22)
        .expect("sample wall clock is valid")
        .with_millis(Some(420));
    let raw = parsed.render(&Fields {
        time: Some(&clock),
        make: Some("Sony"),
        model: Some("ILCE-7M4"),
        original: "IMG_1024",
        counter: 1,
    });
    let stem = template::sanitize_stem(&raw, 180);
    ("IMG_1024.JPG".into(), format!("{stem}{ext}"))
}

fn filter_label(r: &RenameStrings, filter: StateFilter) -> &'static str {
    match filter {
        StateFilter::All => r.filter_all,
        StateFilter::Ready => r.filter_ready,
        StateFilter::Unchanged => r.filter_unchanged,
        StateFilter::NoDate => r.filter_no_date,
        StateFilter::Collision => r.filter_collision,
        StateFilter::Problem => r.filter_problem,
    }
}

fn notice_is_critical(notice: &Notice) -> bool {
    match notice {
        Notice::JournalFailed(_) | Notice::HistoryUnreadable => true,
        Notice::Applied { failures, .. } | Notice::Undone { failures, .. } => !failures.is_empty(),
        Notice::Stale | Notice::HistoryCleared => false,
    }
}

fn notice_is_warning(notice: &Notice) -> bool {
    match notice {
        Notice::Stale => true,
        Notice::Applied {
            cancelled: true,
            failures,
            ..
        } => failures.is_empty(),
        Notice::Undone {
            skipped, failures, ..
        } => *skipped > 0 && failures.is_empty(),
        _ => false,
    }
}

fn notice_lines(r: &RenameStrings, notice: &Notice, critical: bool) -> Vec<View> {
    const MAX_FAILURES: usize = 5;
    let bold = |text: String| -> View {
        let block = TextBlock::new()
            .text(text)
            .font_size(12.5)
            .font_weight(FontWeight::SEMI_BOLD)
            .text_wrapping(TextWrapping::Wrap);
        if critical {
            block.foreground(ThemeBrush::SystemCritical).into()
        } else {
            block.into()
        }
    };
    let failures_of = |failures: &[String]| -> Vec<View> {
        let mut lines = Vec::new();
        if !failures.is_empty() {
            lines.push(settings_ui::caption(&RenameStrings::count(
                r.group_failures,
                failures.len(),
            )));
            lines.extend(
                failures
                    .iter()
                    .take(MAX_FAILURES)
                    .map(|failure| settings_ui::caption(failure)),
            );
        }
        lines
    };
    match notice {
        Notice::Applied {
            renamed,
            failures,
            cancelled,
        } => {
            let mut lines = vec![bold(RenameStrings::count(r.applied, *renamed))];
            if *cancelled {
                lines.push(settings_ui::caption(r.cancelled));
            }
            lines.extend(failures_of(failures));
            lines
        }
        Notice::Undone {
            restored,
            skipped,
            failures,
        } => {
            let mut lines = vec![bold(RenameStrings::count(r.undone, *restored))];
            if *skipped > 0 {
                lines.push(settings_ui::caption(&RenameStrings::count(
                    r.undo_skipped,
                    *skipped,
                )));
            }
            lines.extend(failures_of(failures));
            lines
        }
        Notice::Stale => vec![bold(r.stale.to_string())],
        Notice::JournalFailed(error) => vec![bold(r.journal_failed.replace("{error}", error))],
        Notice::HistoryCleared => vec![settings_ui::caption(r.history_cleared)],
        Notice::HistoryUnreadable => vec![bold(r.history_unreadable.to_string())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_preview_renders_tokens_and_extension_case() {
        assert_eq!(
            sample_preview("{yyyy}-{MM}-{dd}_{model}", true),
            ("IMG_1024.JPG".into(), "2026-05-01_ILCE-7M4.jpg".into())
        );
        assert_eq!(
            sample_preview("{original}_{counter:2}", false),
            ("IMG_1024.JPG".into(), "IMG_1024_01.JPG".into())
        );
    }

    #[test]
    fn notice_severity_distinguishes_errors_warnings_and_clean_outcomes() {
        let clean = Notice::Applied {
            renamed: 4,
            failures: Vec::new(),
            cancelled: false,
        };
        assert!(!notice_is_critical(&clean) && !notice_is_warning(&clean));
        assert!(notice_is_warning(&Notice::Stale));
        assert!(notice_is_critical(&Notice::HistoryUnreadable));
        let partial = Notice::Applied {
            renamed: 2,
            failures: vec!["a.jpg: access denied".into()],
            cancelled: false,
        };
        assert!(notice_is_critical(&partial));
    }
}
