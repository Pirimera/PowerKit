//! MediaRenamer workspace: sticky header, template bar, filter/action bar and the
//! preview table. Files and folders can be dropped anywhere on the pane.
use super::super::media_renamer::{
    Action, Busy, DrawerView, MAX_VISIBLE_ROWS, Notice, RowView, StateFilter,
};
use super::super::settings_ui;
use super::store_page::{button_row, icon_label, sized_icon};
use super::{Message, Shell};
use crate::locale::RenameStrings;
use crate::media_renamer::{plan::State, template};
use std::path::PathBuf;
use windows_reactor::*;

const COL_CHECK_WIDTH: f64 = 34.0;
const COL_TIME_WIDTH: f64 = 170.0;
const COL_STATE_WIDTH: f64 = 150.0;
const FILTER_STATE_WIDTH: f64 = 200.0;
const APPLY_WIDTH: f64 = 176.0;

const GLYPH_ADD: &str = "\u{E710}";
const GLYPH_FOLDER: &str = "\u{E8B7}";
const GLYPH_HISTORY: &str = "\u{E81C}";
const GLYPH_SETTINGS: &str = "\u{E713}";
const GLYPH_CLEAR: &str = "\u{E711}";
const GLYPH_DELETE: &str = "\u{E74D}";
const GLYPH_RENAME: &str = "\u{E8AC}";
const GLYPH_DROP: &str = "\u{E896}";

fn table_columns() -> [GridLength; 5] {
    [
        GridLength::Pixel(COL_CHECK_WIDTH),
        GridLength::STAR,
        GridLength::STAR,
        GridLength::Pixel(COL_TIME_WIDTH),
        GridLength::Pixel(COL_STATE_WIDTH),
    ]
}

impl Shell {
    fn rename_strings(&self) -> &'static RenameStrings {
        self.t().rename
    }
    pub(super) fn media_renamer_pane(&self, context: &ViewContext<Self>) -> View {
        let mut top = vec![self.media_renamer_header(context)];
        if let Some(strip) = self.media_renamer_strip(context) {
            top.push(strip);
        }
        top.push(self.media_renamer_template_bar(context));
        top.push(self.media_renamer_toolbar(context));
        if !self.snapshot.media_renamer.rows.is_empty() {
            top.push(self.media_renamer_table_header(context));
        }
        let sticky_top = StackPanel::new()
            .grid_row(0)
            .margin(Thickness::new(24.0, 14.0, 24.0, 0.0))
            .children((settings_ui::stack(top, 8.0),));
        let table_scroll = ScrollViewer::new()
            .grid_row(1)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .margin(Thickness::new(24.0, 0.0, 24.0, 20.0))
                    .children((self.media_renamer_table_body(context),)),
            );
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
        let message = |action: Action| context.message(Message::MediaRenamer(action));
        let accent_when = |open: bool| {
            if open {
                ButtonStyle::Accent
            } else {
                ButtonStyle::Default
            }
        };
        let summary = if s.summary.groups == 0 {
            r.lead.to_string()
        } else {
            format!(
                "{} {} · {} {} · {}: {}",
                s.summary.groups,
                r.summary_items,
                s.summary.files,
                r.summary_files,
                r.filter_ready,
                s.summary.ready
            )
        };
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        settings_ui::heading(r.title, 24.0),
                        settings_ui::caption(&summary),
                    )),
                StackPanel::new()
                    .grid_column(1)
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
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
                        Button::new()
                            .style(accent_when(matches!(s.drawer, DrawerView::History(_))))
                            .on_click(message(Action::ToggleHistory))
                            .content(icon_label(GLYPH_HISTORY, r.history)),
                        Button::new()
                            .style(accent_when(matches!(s.drawer, DrawerView::Settings)))
                            .on_click(message(Action::ToggleSettings))
                            .content(icon_label(GLYPH_SETTINGS, r.settings)),
                    )),
            ))
    }

    /// Progress of a scan, rename or undo, the last outcome, and list-wide notes.
    fn media_renamer_strip(&self, context: &ViewContext<Self>) -> Option<View> {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let message = |action: Action| context.message(Message::MediaRenamer(action));
        let mut rows: Vec<View> = Vec::new();
        let mut controls = Vec::new();
        let progress = |label: &str, done: usize, total: usize| -> Vec<View> {
            vec![
                TextBlock::new()
                    .text(RenameStrings::progress(label, done, total))
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                ProgressBar::new()
                    .maximum(total.max(1) as f64)
                    .value(done as f64)
                    .is_indeterminate(total == 0)
                    .into(),
            ]
        };
        match s.busy {
            Busy::Idle => {}
            Busy::Loading => rows.push(settings_ui::caption(r.loading)),
            Busy::Scanning { done, total } => {
                rows.extend(progress(r.scanning, done, total));
                if s.queued > 0 {
                    rows.push(settings_ui::caption(&RenameStrings::count(
                        r.queued, s.queued,
                    )));
                }
                controls.push(
                    Button::new()
                        .on_click(message(Action::CancelScan))
                        .content(r.cancel),
                );
            }
            Busy::Applying { done, total } | Busy::Undoing { done, total } => {
                let label = if matches!(s.busy, Busy::Applying { .. }) {
                    r.applying
                } else {
                    r.undoing
                };
                rows.extend(progress(label, done, total));
                controls.push(
                    Button::new()
                        .on_click(message(Action::CancelWork))
                        .content(r.cancel),
                );
            }
        }
        if let Some(notice) = &s.notice {
            rows.extend(notice_lines(r, notice));
            if s.busy == Busy::Idle {
                controls.push(
                    Button::new()
                        .style(ButtonStyle::Subtle)
                        .on_click(message(Action::DismissNotice))
                        .content(r.close),
                );
            }
        }
        if s.skipped > 0 {
            rows.push(settings_ui::caption(&RenameStrings::count(
                r.skipped_files,
                s.skipped,
            )));
        }
        if s.batch_capped {
            rows.push(settings_ui::caption(r.batch_capped));
        }
        if rows.is_empty() {
            return None;
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
                        .columns([GridLength::STAR, GridLength::Auto])
                        .column_spacing(12.0)
                        .children((
                            StackPanel::new()
                                .grid_column(0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((settings_ui::stack(rows, 4.0),)),
                            StackPanel::new()
                                .grid_column(1)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((button_row(controls),)),
                        )),
                ),
        )
    }

    fn media_renamer_template_bar(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let current = s.template.clone();
        let presets = Button::new()
            .grid_column(1)
            .content(r.presets)
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
            .content(r.tokens)
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
        let reset = Button::new()
            .grid_column(3)
            .is_enabled(s.template != s.default_template)
            .on_click(context.message(Message::MediaRenamer(Action::SetTemplate(
                s.default_template.clone(),
            ))))
            .content(r.reset_template);
        let input = Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::Auto,
                GridLength::Auto,
                GridLength::Auto,
            ])
            .column_spacing(8.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(s.template.clone())
                    .placeholder_text(template::DEFAULT)
                    .on_text_changed(context.callback(Message::MediaRenamerTemplate)),
                presets,
                tokens,
                reset,
            ));
        let feedback: View = match &s.template_error {
            Some(error) => TextBlock::new()
                .text(r.template_error(error))
                .foreground(ThemeBrush::SystemCritical)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            None => match s.rows.iter().find(|row| !row.new_name.is_empty()) {
                Some(row) => settings_ui::caption(&format!("{} → {}", row.current, row.new_name)),
                None => settings_ui::caption(r.drop_hint),
            },
        };
        let toggle =
            |label: &'static str, on: bool, action: fn(bool) -> Action| {
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        TextBlock::new()
                            .text(label)
                            .vertical_alignment(VerticalAlignment::Center),
                        settings_ui::toggle_switch(ToggleSwitch::new().is_on(on).on_toggled(
                            context.callback(move |on| Message::MediaRenamer(action(on))),
                        )),
                    ))
            };
        let options = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(24.0)
            .children((
                toggle(r.recurse, s.recurse, Action::SetRecurse),
                toggle(r.mtime_fallback, s.mtime_fallback, Action::SetMtimeFallback),
            ));
        let mut rows = vec![
            TextBlock::new()
                .text(r.template)
                .font_weight(FontWeight::SEMI_BOLD)
                .into(),
            input,
            feedback,
            options,
        ];
        if s.mtime_fallback {
            rows.push(settings_ui::caption(r.mtime_fallback_hint));
        }
        settings_ui::surface(settings_ui::stack(rows, 8.0))
    }

    fn media_renamer_toolbar(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let message = |action: Action| context.message(Message::MediaRenamer(action));
        let labels: Vec<String> = StateFilter::ALL
            .iter()
            .map(|filter| filter_label(r, *filter).to_string())
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
            StackPanel::new().grid_column(1).into()
        } else {
            Button::new()
                .grid_column(1)
                .width(32.0)
                .style(ButtonStyle::Subtle)
                .on_click(message(Action::SetFilterText(String::new())))
                .content(sized_icon(GLYPH_CLEAR, 11.0))
        };
        let search = Grid::new()
            .grid_column(0)
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(4.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(s.filter_text.clone())
                    .placeholder_text(r.filter_hint)
                    .on_text_changed(context.callback(Message::MediaRenamerFilter)),
                clear_filter,
            ));
        let filters = Grid::new()
            .columns([GridLength::STAR, GridLength::Pixel(FILTER_STATE_WIDTH)])
            .column_spacing(8.0)
            .children((search, state_combo));
        let idle = s.busy == Busy::Idle;
        let apply_label = if s.summary.ready > 0 {
            format!("{} ({})", r.apply, s.summary.ready)
        } else {
            r.apply.to_string()
        };
        let actions = button_row(vec![
            Button::new()
                .min_width(APPLY_WIDTH)
                .style(ButtonStyle::Accent)
                .is_enabled(s.can_apply)
                .on_click(message(Action::Apply))
                .content(icon_label(GLYPH_RENAME, &apply_label)),
            Button::new()
                .is_enabled(idle && s.summary.groups > 0)
                .on_click(message(Action::Clear))
                .content(icon_label(GLYPH_DELETE, r.clear)),
        ]);
        let shown = if s.visible == s.summary.groups {
            format!("{} {}", s.visible, r.summary_items)
        } else {
            format!("{} / {} {}", s.visible, s.summary.groups, r.summary_items)
        };
        let actions_row = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((settings_ui::caption(&shown),)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((actions,)),
            ));
        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .padding(Thickness::xy(12.0, 10.0))
            .content(settings_ui::stack(vec![filters, actions_row], 8.0))
    }

    fn media_renamer_table_header(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let header = |label: &str| {
            TextBlock::new()
                .text(label)
                .font_size(12.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .opacity(0.75)
                .vertical_alignment(VerticalAlignment::Center)
        };
        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::new(8.0, 8.0, 0.0, 0.0))
            .padding(Thickness::xy(12.0, 6.0))
            .content(
                Grid::new()
                    .columns(table_columns())
                    .column_spacing(10.0)
                    .children((
                        CheckBox::new()
                            .grid_column(0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .is_enabled(s.any_selectable && s.busy == Busy::Idle)
                            .is_checked(s.all_visible_selected)
                            .on_is_checked_changed(context.callback(|checked| {
                                Message::MediaRenamer(Action::SetSelectAllVisible(checked))
                            })),
                        header(r.col_current).grid_column(1),
                        header(r.col_new).grid_column(2),
                        header(r.col_time).grid_column(3),
                        header(r.col_state).grid_column(4),
                    )),
            )
    }

    fn media_renamer_table_body(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        if s.rows.is_empty() && s.summary.groups > 0 {
            return Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(16.0, 16.0))
                .content(settings_ui::caption(&format!(
                    "0 / {} {}",
                    s.summary.groups, r.summary_items
                )));
        }
        if s.rows.is_empty() {
            return Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(16.0, 32.0))
                .content(
                    StackPanel::new()
                        .spacing(10.0)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .children((
                            sized_icon(GLYPH_DROP, 32.0),
                            TextBlock::new()
                                .text(r.empty)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .horizontal_alignment(HorizontalAlignment::Center),
                            TextBlock::new()
                                .text(r.drop_hint)
                                .opacity(0.8)
                                .text_wrapping(TextWrapping::Wrap)
                                .horizontal_alignment(HorizontalAlignment::Center),
                        )),
                );
        }
        let last = s.rows.len() - 1;
        let mut items: Vec<KeyedView> = s
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                KeyedView::new(
                    format!("{}|{}", row.group, row.current),
                    self.media_renamer_row(context, row, index == last && !s.truncated),
                )
            })
            .collect();
        if s.truncated {
            items.push(KeyedView::new(
                "truncated",
                Border::new()
                    .background(ThemeBrush::CardBackground)
                    .border_brush(ThemeBrush::CardStroke)
                    .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
                    .corner_radius(CornerRadius::new(0.0, 0.0, 8.0, 8.0))
                    .padding(Thickness::xy(14.0, 10.0))
                    .content(settings_ui::caption(&format!(
                        "{} ({MAX_VISIBLE_ROWS} / {})",
                        r.truncated, s.visible
                    ))),
            ));
        }
        StackPanel::new().spacing(0.0).keyed_children(items)
    }

    fn media_renamer_row(&self, context: &ViewContext<Self>, row: &RowView, last: bool) -> View {
        let s = &self.snapshot.media_renamer;
        let r = self.rename_strings();
        let group = row.group;
        let mut current_lines = vec![
            TextBlock::new()
                .text(row.current.as_str())
                .text_trimming(TextTrimming::CharacterEllipsis)
                .into(),
        ];
        if row.members > 1 {
            current_lines.push(
                TextBlock::new()
                    .text(RenameStrings::count(r.companions, row.members - 1))
                    .font_size(11.5)
                    .opacity(0.65)
                    .into(),
            );
        }
        let current = Button::new()
            .grid_column(1)
            .style(ButtonStyle::Subtle)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Left)
            .on_click(context.message(Message::MediaRenamer(Action::OpenDetails(group))))
            .content(settings_ui::stack(current_lines, 1.0))
            .tooltip(r.details);
        let changes = row.state == State::Ready;
        let new_name = TextBlock::new()
            .grid_column(2)
            .text(if row.new_name.is_empty() {
                "—"
            } else {
                row.new_name.as_str()
            })
            .font_weight(if changes && row.selected {
                FontWeight::SEMI_BOLD
            } else {
                FontWeight::NORMAL
            })
            .opacity(if changes { 1.0 } else { 0.65 })
            .text_trimming(TextTrimming::CharacterEllipsis)
            .vertical_alignment(VerticalAlignment::Center);
        let time = StackPanel::new()
            .grid_column(3)
            .spacing(1.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                TextBlock::new().text(if row.time.is_empty() {
                    "—"
                } else {
                    row.time.as_str()
                }),
                TextBlock::new()
                    .text(
                        row.source
                            .map(|source| r.source(source))
                            .unwrap_or_default(),
                    )
                    .font_size(11.0)
                    .opacity(0.6)
                    .text_trimming(TextTrimming::CharacterEllipsis),
            ));
        let mut state_text = r.state(row.state);
        if row.collided {
            state_text = format!("{state_text} · {}", r.collided);
        }
        let problem = matches!(
            row.state,
            State::Unreadable | State::CodecMissing(_) | State::Conflict | State::OnlineOnly
        );
        let state = TextBlock::new()
            .grid_column(4)
            .text(state_text)
            .text_wrapping(TextWrapping::Wrap)
            .opacity(if problem || changes { 1.0 } else { 0.75 })
            .vertical_alignment(VerticalAlignment::Center);
        let state: View = if problem {
            state.foreground(ThemeBrush::SystemCritical).into()
        } else {
            state.into()
        };
        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
            .corner_radius(if last {
                CornerRadius::new(0.0, 0.0, 8.0, 8.0)
            } else {
                CornerRadius::uniform(0.0)
            })
            .padding(Thickness::xy(12.0, 6.0))
            .content(
                Grid::new()
                    .columns(table_columns())
                    .column_spacing(10.0)
                    .children((
                        CheckBox::new()
                            .grid_column(0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .is_enabled(row.selectable && s.busy == Busy::Idle)
                            .is_checked(row.selected && row.selectable)
                            .on_is_checked_changed(context.callback(move |selected| {
                                Message::MediaRenamer(Action::SetSelected(group, selected))
                            })),
                        current,
                        new_name,
                        time,
                        state,
                    )),
            )
    }
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

fn notice_lines(r: &RenameStrings, notice: &Notice) -> Vec<View> {
    const MAX_FAILURES: usize = 5;
    let bold = |text: String| -> View {
        TextBlock::new()
            .text(text)
            .font_weight(FontWeight::SEMI_BOLD)
            .text_wrapping(TextWrapping::Wrap)
            .into()
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
    }
}
