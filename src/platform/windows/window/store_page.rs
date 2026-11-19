//! PowerStore main workspace: sticky header, unified filter/action bar, sticky column
//! header, scrollable unified package table and the Discover search dialog.
use super::super::power_store::session::{PackageRow, RowOperation};
use super::super::power_store::{Action, DrawerView, Notice, OpResult, Status};
use super::super::settings_ui;
use super::manager_icon::manager_icon;
use super::{AppCommand, Message, Shell};
use crate::power_store::{
    LIST_WORKER_STOPPED, ListMode, NO_MANAGERS_AVAILABLE, NO_SEARCH_ANSWER, OperationKind,
    OperationProgress, ProgressStage, ScopeFilter, SortKey, StatusFilter,
    WINGET_UNAVAILABLE_MISSING, WINGET_UNAVAILABLE_TOO_OLD, sort_choice_from_index,
    sort_choice_index,
};
use windows_reactor::*;

const COL_CHECK_WIDTH: f64 = 34.0;
const COL_VERSION_WIDTH: f64 = 166.0;
const SOURCE_ICON_SIZE: f64 = 18.0;
/// Room for the manager badge, its gap and a name such as `powershell7`.
const COL_SOURCE_WIDTH: f64 = 140.0;
const COL_ACTIONS_WIDTH: f64 = 188.0;

/// Fixed filter control widths so selection changes do not reflow the toolbar.
const FILTER_SOURCE_WIDTH: f64 = 156.0;
const FILTER_STATUS_WIDTH: f64 = 200.0;
const FILTER_SCOPE_WIDTH: f64 = 136.0;
const FILTER_SORT_WIDTH: f64 = 204.0;
const FILTER_PRIMARY_ACTION_WIDTH: f64 = 188.0;

const GLYPH_SEARCH: &str = "\u{E721}";
const GLYPH_HISTORY: &str = "\u{E81C}";
const GLYPH_SETTINGS: &str = "\u{E713}";
const GLYPH_SYNC: &str = "\u{E72C}";
const GLYPH_DELETE: &str = "\u{E74D}";
const GLYPH_IGNORE: &str = "\u{E711}";
const GLYPH_CANCEL: &str = "\u{E711}";
const GLYPH_MORE: &str = "\u{E712}";
const GLYPH_FILTER: &str = "\u{E71C}";

fn table_columns() -> [GridLength; 5] {
    [
        GridLength::Pixel(COL_CHECK_WIDTH),
        GridLength::STAR,
        GridLength::Pixel(COL_VERSION_WIDTH),
        GridLength::Pixel(COL_SOURCE_WIDTH),
        GridLength::Pixel(COL_ACTIONS_WIDTH),
    ]
}

impl Shell {
    pub(super) fn power_store_pane(&self, context: &ViewContext<Self>) -> View {
        let mut top_chrome = vec![self.power_store_header_bar(context)];
        if let Some(banner) = self.power_store_discover_banner(context) {
            top_chrome.push(banner);
        }
        if let Some(strip) = self.power_store_live_strip(context) {
            top_chrome.push(strip);
        }
        if let Some(error_card) = self.power_store_error_card() {
            top_chrome.push(error_card);
        }
        top_chrome.push(self.power_store_toolbar(context));
        top_chrome.push(self.power_store_table_header(context));

        let sticky_top = StackPanel::new()
            .grid_row(0)
            .margin(Thickness::new(24.0, 14.0, 24.0, 0.0))
            .children((settings_ui::stack(top_chrome, 8.0),));

        let table_scroll = ScrollViewer::new()
            .grid_row(1)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .margin(Thickness::new(24.0, 0.0, 24.0, 20.0))
                    .children((self.power_store_table_body(context),)),
            );

        let main = Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((sticky_top, table_scroll));

        let mut layers = vec![KeyedView::new("main", main)];
        if let Some(drawer) = self.power_store_drawer_overlay(context) {
            layers.push(KeyedView::new("drawer", drawer));
        }
        layers.push(KeyedView::new(
            "discover-dialog",
            self.power_store_discover_dialog(context),
        ));
        Grid::new().keyed_children(layers)
    }

    fn power_store_header_bar(&self, context: &ViewContext<Self>) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let summary = self.power_store_header_summary();

        let history_open = matches!(store.drawer, DrawerView::History);
        let settings_open = matches!(store.drawer, DrawerView::Settings);

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        settings_ui::heading(t.power_store_title, 24.0),
                        settings_ui::caption(&summary),
                    )),
                StackPanel::new()
                    .grid_column(1)
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        Button::new()
                            .style(if store.mode == ListMode::Discover {
                                ButtonStyle::Accent
                            } else {
                                ButtonStyle::Default
                            })
                            .on_click(context.message(Message::OpenDiscoverDialog))
                            .content(icon_label(GLYPH_SEARCH, t.power_store_discover_button)),
                        Button::new()
                            .style(if history_open {
                                ButtonStyle::Accent
                            } else {
                                ButtonStyle::Default
                            })
                            .on_click(
                                context.message(Message::PowerStore(Action::ToggleHistoryDrawer)),
                            )
                            .content(icon_label(GLYPH_HISTORY, t.power_store_history)),
                        Button::new()
                            .style(if settings_open {
                                ButtonStyle::Accent
                            } else {
                                ButtonStyle::Default
                            })
                            .on_click(
                                context.message(Message::PowerStore(Action::ToggleSettingsDrawer)),
                            )
                            .content(icon_label(GLYPH_SETTINGS, t.power_store_settings)),
                    )),
            ))
    }

    fn power_store_header_summary(&self) -> String {
        let store = &self.snapshot.power_store;
        let t = self.t();
        if matches!(store.status, Status::Loading) && store.packages.is_empty() {
            return t.power_store_loading.to_string();
        }
        let mut parts = Vec::new();
        if store.total_installed > 0 {
            parts.push(format!(
                "{}: {}",
                t.power_store_installed, store.total_installed
            ));
        }
        if store.refreshing {
            parts.push(t.power_store_refreshing.to_string());
        }
        // Operations also show on their rows; the summary keeps them visible when the
        // package is filtered out of the current list.
        if let Some(running) = &store.op.running {
            parts.push(format!(
                "{}: {}",
                running.name,
                power_store_row_label(t, running.kind, store.op.progress, store.op.cancelling)
            ));
        }
        if store.op.pending > 0 {
            parts.push(format!(
                "{} {}",
                store.op.pending, t.power_store_queue_pending
            ));
        }
        if store.check.checking {
            parts.push(t.power_store_checking.to_string());
        } else if let Some(count) = store.check.count {
            if count > 0 {
                parts.push(format!("{}: {count}", t.power_store_updates_count));
            } else {
                parts.push(t.power_store_up_to_date.to_string());
            }
        }
        if let Some(stamp) = &store.check.last_checked {
            parts.push(format!("{}: {stamp}", t.power_store_last_checked));
        }
        if parts.is_empty() {
            t.power_store_lead.to_string()
        } else {
            parts.join(" · ")
        }
    }

    fn power_store_discover_banner(&self, context: &ViewContext<Self>) -> Option<View> {
        let store = &self.snapshot.power_store;
        if store.mode != ListMode::Discover {
            return None;
        }
        let t = self.t();
        let source = store
            .discover_source
            .as_deref()
            .unwrap_or(t.power_store_all_sources);
        let label = format!(
            "{}: \"{}\" · {source} · {} {}",
            t.power_store_discover_results,
            store.discover_query,
            store.visible_count,
            t.power_store_showing
        );
        Some(
            Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::Accent)
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
                                .orientation(Orientation::Horizontal)
                                .spacing(8.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((
                                    sized_icon(GLYPH_SEARCH, 14.0),
                                    TextBlock::new()
                                        .text(label)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .text_wrapping(TextWrapping::Wrap),
                                )),
                            StackPanel::new()
                                .grid_column(1)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((button_row(vec![
                                    Button::new()
                                        .on_click(context.message(Message::OpenDiscoverDialog))
                                        .content(t.power_store_change_search),
                                    Button::new()
                                        .style(ButtonStyle::Accent)
                                        .on_click(
                                            context
                                                .message(Message::PowerStore(Action::ExitDiscover)),
                                        )
                                        .content(t.power_store_back_installed),
                                ]),)),
                        )),
                ),
        )
    }

    /// Failure and one-shot notices only. Running and waiting operations appear on the
    /// package's own table row, never as a banner above the filters.
    fn power_store_live_strip(&self, context: &ViewContext<Self>) -> Option<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let headline = if let Some(notice) = &store.notice {
            power_store_notice(t, notice)
        } else if let Some((label, OpResult::Failed(error))) = &store.op.last {
            format!(
                "{}: {} · {} · {error}",
                power_store_operation_name(t, label.kind),
                label.name,
                t.power_store_operation_failed
            )
        } else {
            return None;
        };

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
                            TextBlock::new()
                                .grid_column(0)
                                .text(headline)
                                .text_wrapping(TextWrapping::Wrap)
                                .vertical_alignment(VerticalAlignment::Center),
                            Button::new()
                                .grid_column(1)
                                .style(ButtonStyle::Subtle)
                                .on_click(
                                    context.message(Message::PowerStore(Action::DismissNotice)),
                                )
                                .content(t.power_store_close),
                        )),
                ),
        )
    }

    fn power_store_error_card(&self) -> Option<View> {
        let t = self.t();
        match &self.snapshot.power_store.status {
            Status::Unavailable(message) => {
                let (title, detail) = match message.as_str() {
                    WINGET_UNAVAILABLE_MISSING => (
                        t.power_store_unavailable_missing,
                        t.power_store_unavailable_missing_detail,
                    ),
                    WINGET_UNAVAILABLE_TOO_OLD => (
                        t.power_store_unavailable_too_old,
                        t.power_store_unavailable_too_old_detail,
                    ),
                    NO_MANAGERS_AVAILABLE => {
                        (t.power_store_no_managers, t.power_store_no_managers_detail)
                    }
                    _ => (t.power_store_unavailable, message.as_str()),
                };
                Some(settings_ui::status_card(title, detail))
            }
            Status::Failed(message) => {
                let detail = match message.as_str() {
                    NO_SEARCH_ANSWER => t.power_store_no_search_answer,
                    LIST_WORKER_STOPPED => t.power_store_worker_stopped,
                    _ => message.as_str(),
                };
                Some(settings_ui::status_card(t.power_store_failed, detail))
            }
            _ => None,
        }
    }

    fn power_store_toolbar(&self, context: &ViewContext<Self>) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let installed_mode = store.mode == ListMode::Installed;
        let loading = matches!(store.status, Status::Loading) || store.refreshing;

        let source_filter = self.power_store_source_filter(context);

        // Status filter ComboBox
        let update_badge = store.check.count.unwrap_or(0);
        let status_labels = [
            t.power_store_status_all.to_string(),
            format!("{} ({update_badge})", t.power_store_status_updates),
            t.power_store_status_current.to_string(),
            format!("{} ({})", t.power_store_status_pinned, store.pinned_count),
        ];
        let current_status = store.query.status;
        let status_combo = ComboBox::new()
            .width(FILTER_STATUS_WIDTH)
            .min_width(FILTER_STATUS_WIDTH)
            .items_source(status_labels)
            .selected_index(current_status.index())
            .is_enabled(installed_mode)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                let next = index.and_then(StatusFilter::from_index).unwrap_or_default();
                if next != current_status {
                    Message::PowerStore(Action::SetStatusFilter(next))
                } else {
                    Message::Noop
                }
            }));

        // Scope filter ComboBox
        let scope_labels = [
            t.power_store_scope_all.to_string(),
            t.power_store_scope_user.to_string(),
            t.power_store_scope_machine.to_string(),
        ];
        let current_scope = store.query.scope;
        let scope_combo = ComboBox::new()
            .width(FILTER_SCOPE_WIDTH)
            .min_width(FILTER_SCOPE_WIDTH)
            .items_source(scope_labels)
            .selected_index(current_scope.index())
            .is_enabled(installed_mode)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                let next = index.and_then(ScopeFilter::from_index).unwrap_or_default();
                if next != current_scope {
                    Message::PowerStore(Action::SetScopeFilter(next))
                } else {
                    Message::Noop
                }
            }));

        // Every sort key in both directions; the arrow shows the direction.
        let sort_labels: Vec<String> = SortKey::ALL
            .iter()
            .flat_map(|key| {
                let label = power_store_sort_label(t, *key);
                [format!("{label} ↑"), format!("{label} ↓")]
            })
            .collect();
        let current_choice = sort_choice_index(store.query.sort_by, store.query.sort_ascending);
        let sort_combo = ComboBox::new()
            .width(FILTER_SORT_WIDTH)
            .min_width(FILTER_SORT_WIDTH)
            .items_source(sort_labels)
            .selected_index(current_choice)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                match index
                    .filter(|index| *index != current_choice)
                    .and_then(sort_choice_from_index)
                {
                    Some((key, ascending)) => Message::PowerStore(Action::SetSort(key, ascending)),
                    None => Message::Noop,
                }
            }));

        // Filter search input with optional clear button
        let has_filter_text = !store.query.filter_text.is_empty();
        let clear_btn: View = if has_filter_text {
            Button::new()
                .grid_column(1)
                .width(32.0)
                .style(ButtonStyle::Subtle)
                .on_click(
                    context.message(Message::PowerStore(Action::SetFilterText(String::new()))),
                )
                .content(sized_icon(GLYPH_IGNORE, 11.0))
        } else {
            StackPanel::new().grid_column(1).into()
        };
        let search_box = Grid::new()
            .grid_column(0)
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(4.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(store.query.filter_text.clone())
                    .placeholder_text(t.power_store_filter_hint)
                    .on_text_changed(context.callback(Message::PowerStoreFilterText)),
                clear_btn,
            ));

        let filters_row = Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::Pixel(FILTER_SOURCE_WIDTH),
                GridLength::Pixel(FILTER_STATUS_WIDTH),
                GridLength::Pixel(FILTER_SCOPE_WIDTH),
                GridLength::Pixel(FILTER_SORT_WIDTH),
            ])
            .column_spacing(8.0)
            .children((
                search_box,
                StackPanel::new()
                    .grid_column(1)
                    .width(FILTER_SOURCE_WIDTH)
                    .children((source_filter,)),
                status_combo.grid_column(2),
                scope_combo.grid_column(3),
                sort_combo.grid_column(4),
            ));

        // Action row (left count/selection + right bulk actions)
        let left_info: View = if store.selected_count > 0 {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(format!(
                            "{}: {}",
                            t.power_store_selected, store.selected_count
                        ))
                        .font_weight(FontWeight::SEMI_BOLD)
                        .vertical_alignment(VerticalAlignment::Center),
                    Button::new()
                        .style(ButtonStyle::Subtle)
                        .on_click(context.message(Message::PowerStore(Action::ClearSelection)))
                        .content(t.power_store_clear_selection),
                ))
        } else {
            settings_ui::caption(&format!(
                "{} {}",
                store.visible_count, t.power_store_showing
            ))
        };

        let primary_action = if !installed_mode {
            Button::new()
                .min_width(FILTER_PRIMARY_ACTION_WIDTH)
                .style(ButtonStyle::Accent)
                .is_enabled(store.selected_count > 0)
                .on_click(context.message(Message::PowerStore(Action::QueueSelected(
                    OperationKind::Install,
                ))))
                .content(if store.selected_count > 0 {
                    format!(
                        "{} ({})",
                        t.power_store_install_selected, store.selected_count
                    )
                } else {
                    t.power_store_install_selected.to_string()
                })
        } else if store.selected_count > 0 {
            Button::new()
                .min_width(FILTER_PRIMARY_ACTION_WIDTH)
                .style(ButtonStyle::Accent)
                .is_enabled(store.selected_updatable_count > 0)
                .on_click(context.message(Message::PowerStore(Action::QueueSelected(
                    OperationKind::Update,
                ))))
                .content(format!(
                    "{} ({})",
                    t.power_store_update_selected, store.selected_updatable_count
                ))
        } else {
            let visible_updates = store.visible_updatable_count;
            Button::new()
                .min_width(FILTER_PRIMARY_ACTION_WIDTH)
                .style(ButtonStyle::Accent)
                .is_enabled(visible_updates > 0)
                .on_click(context.message(Message::PowerStore(Action::QueueVisibleUpdates)))
                .content(if visible_updates > 0 {
                    format!("{} ({visible_updates})", t.power_store_update_all)
                } else {
                    t.power_store_update_all.to_string()
                })
        };

        let mut action_buttons = vec![
            primary_action,
            Button::new()
                .is_enabled(installed_mode && store.selected_removable_count > 0)
                .on_click(context.message(Message::PowerStore(Action::QueueSelected(
                    OperationKind::Uninstall,
                ))))
                .content(icon_label(GLYPH_DELETE, t.power_store_uninstall)),
            Button::new()
                .is_enabled(installed_mode && store.selected_removable_count > 0)
                .on_click(context.message(Message::PowerStore(Action::IgnoreSelected)))
                .content(icon_label(GLYPH_IGNORE, t.power_store_ignore)),
            Button::new()
                .is_enabled(!loading)
                .on_click(context.message(Message::Command(AppCommand::PowerStoreRefresh)))
                .content(icon_label(GLYPH_SYNC, t.power_store_refresh)),
        ];
        if store.op.pending > 0 {
            // Per-row buttons remove one waiting item; this clears the whole queue.
            action_buttons.push(
                Button::new()
                    .on_click(
                        context.message(Message::Command(AppCommand::PowerStoreCancelPending)),
                    )
                    .content(format!(
                        "{} ({})",
                        t.power_store_cancel_pending, store.op.pending
                    )),
            );
        }
        let actions_bar = button_row(action_buttons);

        let actions_row = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((left_info,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((actions_bar,)),
            ));

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .padding(Thickness::xy(12.0, 10.0))
            .content(settings_ui::stack(vec![filters_row, actions_row], 8.0))
    }

    /// Multi-select source filter: a button whose flyout lists one checkbox per source.
    fn power_store_source_filter(&self, context: &ViewContext<Self>) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let picked = &store.query.sources;
        // Keep a stable caption shape ("Sources" / "Sources (n)") so the button
        // width does not jump when a single source name is selected.
        let label = match picked.len() {
            0 => t.power_store_all_sources.to_string(),
            count => format!("{} ({count})", t.power_store_sources_picked),
        };

        let mut rows: Vec<View> = store
            .sources
            .iter()
            .map(|source| {
                let value = source.clone();
                CheckBox::new()
                    .is_checked(picked.contains(&source.to_ascii_lowercase()))
                    .on_is_checked_changed(context.callback(move |checked| {
                        Message::PowerStore(Action::SetSourceChecked(value.clone(), checked))
                    }))
                    .content(source.as_str())
            })
            .collect();
        rows.push(
            Button::new()
                .style(ButtonStyle::Subtle)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .is_enabled(!picked.is_empty())
                .on_click(context.message(Message::PowerStore(Action::ClearSourceFilter)))
                .content(t.power_store_clear_sources),
        );

        Button::new()
            .width(FILTER_SOURCE_WIDTH)
            .min_width(FILTER_SOURCE_WIDTH)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Left)
            .style(if picked.is_empty() {
                ButtonStyle::Default
            } else {
                ButtonStyle::Accent
            })
            .content(icon_label(GLYPH_FILTER, &label))
            .flyout_with(
                Flyout::rich(
                    StackPanel::new()
                        .min_width(180.0)
                        .children((settings_ui::stack(rows, 2.0),)),
                )
                .placement(FlyoutPlacement::BottomEdgeAlignedLeft),
            )
    }

    fn power_store_table_header(&self, context: &ViewContext<Self>) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let header_text = |label: &str, right: bool| {
            TextBlock::new()
                .text(label)
                .font_size(12.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .opacity(0.75)
                .vertical_alignment(VerticalAlignment::Center)
                .horizontal_alignment(if right {
                    HorizontalAlignment::Right
                } else {
                    HorizontalAlignment::Left
                })
        };

        let app_header_label = if store.query.sort_by == SortKey::Name {
            let arrow = if store.query.sort_ascending {
                " ↑"
            } else {
                " ↓"
            };
            format!("{}{arrow}", t.power_store_col_app)
        } else {
            t.power_store_col_app.to_string()
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
                            .is_enabled(!store.packages.is_empty())
                            .is_checked(store.all_visible_selected)
                            .on_is_checked_changed(context.callback(|checked| {
                                Message::PowerStore(Action::SetSelectAllVisible(checked))
                            })),
                        Button::new()
                            .grid_column(1)
                            .style(ButtonStyle::Subtle)
                            .horizontal_alignment(HorizontalAlignment::Left)
                            .on_click(
                                context.message(Message::PowerStore(Action::SetSortKey(
                                    SortKey::Name,
                                ))),
                            )
                            .content(header_text(&app_header_label, false)),
                        header_text(t.power_store_version, false).grid_column(2),
                        header_text(t.power_store_source, false).grid_column(3),
                        header_text(t.power_store_col_actions, true).grid_column(4),
                    )),
            )
    }

    fn power_store_table_body(&self, context: &ViewContext<Self>) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();

        if store.packages.is_empty() {
            let empty_text = if matches!(store.status, Status::Loading) {
                t.power_store_loading
            } else {
                t.power_store_empty
            };
            return Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
                .corner_radius(CornerRadius::new(0.0, 0.0, 8.0, 8.0))
                .padding(Thickness::xy(16.0, 24.0))
                .content(settings_ui::caption(empty_text));
        }

        let unified_groups =
            store.mode == ListMode::Installed && store.query.status == StatusFilter::All;
        let updatable_rows = store
            .packages
            .iter()
            .filter(|row| row.actionable_update)
            .count();
        let regular_rows = store.packages.len().saturating_sub(updatable_rows);

        let mut items: Vec<KeyedView> = Vec::with_capacity(store.packages.len() + 3);
        let mut shown_updates_header = false;
        let mut shown_installed_header = false;
        let last_index = store.packages.len().saturating_sub(1);

        for (index, package) in store.packages.iter().enumerate() {
            if unified_groups && updatable_rows > 0 {
                if package.actionable_update && !shown_updates_header {
                    shown_updates_header = true;
                    items.push(KeyedView::new(
                        "grp-updates",
                        group_divider(&format!(
                            "{} ({updatable_rows})",
                            t.power_store_group_updates
                        )),
                    ));
                } else if !package.actionable_update && !shown_installed_header {
                    shown_installed_header = true;
                    items.push(KeyedView::new(
                        "grp-installed",
                        group_divider(&format!(
                            "{} ({regular_rows})",
                            t.power_store_group_installed
                        )),
                    ));
                }
            }
            let is_last = index == last_index && !store.truncated;
            let row_key = format!("{}|{}", package.key.source, package.key.id);
            items.push(KeyedView::new(
                row_key,
                self.power_store_table_row(context, package, is_last),
            ));
        }

        if store.truncated {
            items.push(KeyedView::new(
                "truncated",
                Border::new()
                    .background(ThemeBrush::CardBackground)
                    .border_brush(ThemeBrush::CardStroke)
                    .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
                    .corner_radius(CornerRadius::new(0.0, 0.0, 8.0, 8.0))
                    .padding(Thickness::xy(14.0, 10.0))
                    .content(settings_ui::caption(t.power_store_truncated)),
            ));
        }

        StackPanel::new().spacing(0.0).keyed_children(items)
    }

    fn power_store_table_row(
        &self,
        context: &ViewContext<Self>,
        package: &PackageRow,
        is_last: bool,
    ) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let own = package.key.is_own_package();

        // Column 1: App Name + subtle metadata line
        let mut badges = Vec::new();
        if package.ignored {
            badges.push(t.power_store_badge_ignored);
        } else if package.skipped {
            badges.push(t.power_store_badge_skipped);
        }
        if package.configured {
            badges.push(t.power_store_configured);
        }
        let title_text = if badges.is_empty() {
            package.name.clone()
        } else {
            format!("{}  ·  {}", package.name, badges.join(" · "))
        };

        let mut subtitle = package.key.id.clone();
        match store.query.sort_by {
            SortKey::UpdatedDate if !package.updated_at.is_empty() => {
                subtitle.push_str(" · ");
                subtitle.push_str(&package.updated_at);
            }
            SortKey::InstalledDate if !package.install_date.is_empty() => {
                subtitle.push_str(" · ");
                subtitle.push_str(&package.install_date);
            }
            _ if !package.install_date.is_empty() => {
                subtitle.push_str(" · ");
                subtitle.push_str(&package.install_date);
            }
            _ => {}
        }

        let name_text = StackPanel::new().spacing(1.0).children((
            TextBlock::new()
                .text(title_text)
                .font_weight(FontWeight::SEMI_BOLD)
                .text_wrapping(TextWrapping::Wrap),
            TextBlock::new()
                .text(subtitle)
                .font_size(11.5)
                .opacity(0.65)
                .text_wrapping(TextWrapping::Wrap),
        ));
        let open_details = Button::new()
            .grid_column(1)
            .style(ButtonStyle::Subtle)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Left)
            .on_click(context.message(Message::PowerStore(Action::OpenInspector(
                package.key.clone(),
            ))))
            .content(name_text)
            .tooltip(t.power_store_show_details);
        let app_cell = Grid::new()
            .grid_column(1)
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(4.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((package_logo(package), open_details));

        // Column 2: Version (with arrow highlight when an update is available)
        fn dash(value: &str) -> &str {
            if value.is_empty() { "—" } else { value }
        }
        let version_cell: View = if package.installed && !package.available.is_empty() {
            TextBlock::new()
                .grid_column(2)
                .text(format!(
                    "{} → {}",
                    dash(&package.version),
                    package.available
                ))
                .font_weight(if package.actionable_update {
                    FontWeight::SEMI_BOLD
                } else {
                    FontWeight::NORMAL
                })
                .vertical_alignment(VerticalAlignment::Center)
                .text_wrapping(TextWrapping::Wrap)
                .into()
        } else {
            let shown = if package.installed {
                dash(&package.version)
            } else {
                dash(&package.available)
            };
            TextBlock::new()
                .grid_column(2)
                .text(shown)
                .opacity(0.85)
                .vertical_alignment(VerticalAlignment::Center)
                .text_wrapping(TextWrapping::Wrap)
                .into()
        };

        // Column 3: Source (+ subtle scope)
        let scope_caption = match package.scope.as_str() {
            "user" => Some(t.power_store_scope_user),
            "machine" => Some(t.power_store_scope_machine),
            _ => None,
        };
        let source_name = if package.store_managed {
            t.power_store_source_store
        } else {
            package.key.source.as_str()
        };
        let source_text: View = match scope_caption {
            Some(scope_label) => StackPanel::new()
                .spacing(1.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new().text(source_name),
                    TextBlock::new()
                        .text(scope_label)
                        .font_size(11.0)
                        .opacity(0.6),
                )),
            None => TextBlock::new()
                .text(source_name)
                .vertical_alignment(VerticalAlignment::Center)
                .into(),
        };
        let source_cell: View = StackPanel::new()
            .grid_column(3)
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                manager_icon(package.key.manager, SOURCE_ICON_SIZE),
                source_text,
            ));

        // Column 4: Primary CTA + Overflow Menu (`⋯`)
        let row_operation = package.operation;
        let primary_btn = if let Some(operation) = row_operation {
            // The queue state lives on the package's own row: stage/percent while running,
            // "waiting" while queued.
            let label = match operation {
                RowOperation::Running {
                    kind,
                    progress,
                    cancelling,
                } => power_store_row_label(t, kind, progress, cancelling),
                RowOperation::Pending { .. } => t.power_store_waiting.to_string(),
            };
            Button::new().is_enabled(false).content(label)
        } else if store.mode == ListMode::Discover {
            Button::new()
                .style(ButtonStyle::Accent)
                .on_click(context.message(Message::PowerStore(Action::Queue(
                    OperationKind::Install,
                    package.key.clone(),
                    package.name.clone(),
                ))))
                .content(t.power_store_install)
        } else if package.actionable_update && !own {
            Button::new()
                .style(ButtonStyle::Accent)
                .on_click(context.message(Message::PowerStore(Action::Queue(
                    OperationKind::Update,
                    package.key.clone(),
                    package.name.clone(),
                ))))
                .content(t.power_store_update)
        } else if package.store_managed {
            Button::new()
                .on_click(context.message(Message::PowerStore(Action::OpenStoreUpdates)))
                .content(t.power_store_update_in_store)
        } else if !own {
            Button::new()
                .on_click(context.message(Message::PowerStore(Action::Queue(
                    OperationKind::Uninstall,
                    package.key.clone(),
                    package.name.clone(),
                ))))
                .content(t.power_store_uninstall)
        } else {
            Button::new()
                .style(ButtonStyle::Subtle)
                .on_click(context.message(Message::PowerStore(Action::OpenInspector(
                    package.key.clone(),
                ))))
                .content(t.power_store_info)
        };

        // While an operation is queued or running the overflow menu gives way to a cancel
        // (running) or remove-from-queue (waiting) button for this row.
        let more_btn: View = match row_operation {
            Some(RowOperation::Running { cancelling, .. }) => Button::new()
                .width(34.0)
                .style(ButtonStyle::Subtle)
                .is_enabled(!cancelling)
                .on_click(context.message(Message::Command(AppCommand::PowerStoreCancelRunning)))
                .content(sized_icon(GLYPH_CANCEL, 14.0))
                .tooltip(t.power_store_cancel_running),
            Some(RowOperation::Pending { index, .. }) => Button::new()
                .width(34.0)
                .style(ButtonStyle::Subtle)
                .on_click(context.message(Message::PowerStore(Action::RemovePending(index))))
                .content(sized_icon(GLYPH_CANCEL, 14.0))
                .tooltip(t.power_store_remove_pending),
            None => self.row_overflow_menu(context, package),
        };

        let actions_cell = StackPanel::new()
            .grid_column(4)
            .orientation(Orientation::Horizontal)
            .spacing(6.0)
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Center)
            .children((primary_btn, more_btn));

        let check_key = package.key.clone();
        let radius = if is_last {
            CornerRadius::new(0.0, 0.0, 8.0, 8.0)
        } else {
            CornerRadius::uniform(0.0)
        };

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
            .corner_radius(radius)
            .padding(Thickness::xy(12.0, 8.0))
            .content(
                Grid::new()
                    .columns(table_columns())
                    .column_spacing(10.0)
                    .children((
                        CheckBox::new()
                            .grid_column(0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .is_checked(package.selected)
                            .on_is_checked_changed(context.callback(move |selected| {
                                Message::PowerStore(Action::SetSelected(
                                    check_key.clone(),
                                    selected,
                                ))
                            })),
                        app_cell,
                        version_cell,
                        source_cell,
                        actions_cell,
                    )),
            )
    }

    fn row_overflow_menu(&self, context: &ViewContext<Self>, package: &PackageRow) -> View {
        let t = self.t();
        let own = package.key.is_own_package();
        let inspect_label = format!("{} & {}", t.power_store_details, t.power_store_options);
        let skip_label = format!("{} ({})", t.power_store_skip_version, package.available);
        let ignore_label = if package.ignored {
            t.power_store_unignore.to_string()
        } else {
            t.power_store_ignore.to_string()
        };
        let uninstall_label = t.power_store_uninstall.to_string();
        let copy_label = t.power_store_copy_id.to_string();

        let mut items = vec![MenuItem::item("inspect", inspect_label.clone())];
        if package.installed && !own {
            if package.actionable_update && !package.available.is_empty() {
                items.push(MenuItem::item("skip", skip_label.clone()));
            }
            items.push(MenuItem::item("ignore", ignore_label.clone()));
            if package.actionable_update {
                items.push(MenuItem::separator("sep-uninstall"));
                items.push(MenuItem::item("uninstall", uninstall_label.clone()));
            }
        }
        items.push(MenuItem::separator("sep-copy"));
        items.push(MenuItem::item("copy", copy_label.clone()));

        let key = package.key.clone();
        let name = package.name.clone();
        let available = package.available.clone();
        let was_ignored = package.ignored;

        Button::new()
            .width(34.0)
            .style(ButtonStyle::Subtle)
            .content(sized_icon(GLYPH_MORE, 14.0))
            .menu(Menu::new(
                items,
                context.callback(move |picked: String| {
                    let action = if picked == inspect_label {
                        Action::OpenInspector(key.clone())
                    } else if picked == skip_label {
                        Action::SkipVersion(key.clone(), available.clone())
                    } else if picked == ignore_label {
                        if was_ignored {
                            Action::Unignore(key.clone())
                        } else {
                            Action::Ignore(key.clone())
                        }
                    } else if picked == uninstall_label {
                        Action::Queue(OperationKind::Uninstall, key.clone(), name.clone())
                    } else {
                        Action::CopyId(key.id.clone())
                    };
                    Message::PowerStore(action)
                }),
            ))
    }

    fn power_store_discover_sources(&self) -> Vec<String> {
        use super::super::power_store::{ManagerKind, ManagerPresence};
        let t = self.t();
        let store = &self.snapshot.power_store;
        // Managers are installed or not independent of their switch; Discover offers
        // only the ones that are both installed and switched on.
        let enabled = |kind: ManagerKind| {
            store
                .manager_settings
                .get(&kind)
                .is_none_or(|settings| settings.enabled)
        };
        let mut sources = vec![t.power_store_all_sources.to_string()];
        if enabled(ManagerKind::WinGet) {
            sources.extend(["winget".to_string(), "msstore".to_string()]);
        }
        for manager in &store.managers {
            if manager.presence != ManagerPresence::Available || !enabled(manager.kind) {
                continue;
            }
            match manager.kind {
                ManagerKind::Scoop => sources.push("scoop".to_string()),
                ManagerKind::Chocolatey => sources.push("chocolatey".to_string()),
                ManagerKind::Npm => sources.push("npm".to_string()),
                ManagerKind::Pip => sources.push("pip".to_string()),
                ManagerKind::Cargo => sources.push("cargo".to_string()),
                ManagerKind::DotnetTool => sources.push("dotnet".to_string()),
                ManagerKind::PowerShell => sources.push("powershell".to_string()),
                ManagerKind::PowerShell7 => sources.push("powershell7".to_string()),
                ManagerKind::Vcpkg => sources.push("vcpkg".to_string()),
                ManagerKind::WinGet => {}
            }
        }
        sources
    }

    pub(super) fn power_store_discover_source_value(&self, index: usize) -> Option<String> {
        self.power_store_discover_sources()
            .get(index)
            .filter(|label| *label != self.t().power_store_all_sources)
            .cloned()
    }

    pub(super) fn power_store_discover_source_index(&self, source: Option<&str>) -> usize {
        let sources = self.power_store_discover_sources();
        let t = self.t();
        match source {
            Some(value) => sources
                .iter()
                .position(|label| label.eq_ignore_ascii_case(value))
                .unwrap_or(0),
            None => sources
                .iter()
                .position(|label| label == t.power_store_all_sources)
                .unwrap_or(0),
        }
    }

    fn power_store_discover_dialog(&self, context: &ViewContext<Self>) -> View {
        let t = self.t();
        let can_submit = !self.discover_query_draft.trim().is_empty();
        let source_options = self.power_store_discover_sources();

        let dialog_body = Grid::new()
            .min_width(420.0)
            .key_accelerators(KeyAccelerators::new([KeyAccelerator::new(
                AcceleratorKey::Enter,
                AcceleratorModifiers::None,
                context.message(Message::SubmitDiscover),
            )]))
            .children((StackPanel::new().spacing(12.0).children((
                settings_ui::caption(t.power_store_discover_dialog_lead),
                TextBox::new()
                    .text(self.discover_query_draft.clone())
                    .placeholder_text(t.power_store_search_hint)
                    .on_text_changed(context.callback(Message::EditDiscoverQuery)),
                ComboBox::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .items_source(source_options)
                    .selected_index(self.discover_source_index)
                    .on_selection_changed(context.callback(Message::PickDiscoverSource)),
            )),));

        ContentDialog::new()
            .is_open(self.discover_dialog_open)
            .title(t.power_store_discover_dialog_title)
            .primary_button_text(t.power_store_search)
            .close_button_text(t.power_store_cancel)
            .is_primary_button_enabled(can_submit)
            .on_closed(context.callback(Message::DiscoverDialogClosed))
            .content(dialog_body)
    }
}

const LOGO_SIZE: f64 = 24.0;

/// Package logo, or the first letter of its name when no local icon was found.
fn package_logo(package: &PackageRow) -> View {
    let logo: View = match &package.icon {
        Some(png) => Image::new()
            .width(LOGO_SIZE)
            .height(LOGO_SIZE)
            .stretch(Stretch::Uniform)
            .source_data(EncodedImage::new(png.clone()))
            .into(),
        None => {
            let initial: String = package
                .name
                .chars()
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().collect())
                .unwrap_or_else(|| "?".into());
            Border::new()
                .width(LOGO_SIZE)
                .height(LOGO_SIZE)
                .corner_radius(CornerRadius::uniform(6.0))
                .background(ThemeBrush::SolidBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .content(
                    TextBlock::new()
                        .text(initial)
                        .font_size(12.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .opacity(0.75)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .vertical_alignment(VerticalAlignment::Center),
                )
        }
    };
    StackPanel::new()
        .grid_column(0)
        .vertical_alignment(VerticalAlignment::Center)
        .children((logo,))
}

fn power_store_sort_label(t: &crate::locale::Strings, key: SortKey) -> &'static str {
    match key {
        SortKey::Name => t.power_store_sort_name,
        SortKey::Id => t.power_store_sort_id,
        SortKey::InstalledDate => t.power_store_sort_installed,
        SortKey::UpdatedDate => t.power_store_sort_updated,
    }
}

fn group_divider(label: &str) -> View {
    Border::new()
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
        .padding(Thickness::xy(12.0, 6.0))
        .content(
            TextBlock::new()
                .text(label)
                .font_size(11.5)
                .font_weight(FontWeight::SEMI_BOLD)
                .opacity(0.75),
        )
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

fn power_store_notice(t: &crate::locale::Strings, notice: &Notice) -> String {
    match notice {
        Notice::Exported(count, path) => format!("{}: {count} · {path}", t.power_store_exported),
        Notice::ExportFailed(error) => format!("{}: {error}", t.power_store_export_failed),
        Notice::Imported(count) => format!("{}: {count}", t.power_store_imported),
        Notice::ImportFailed(path) => format!("{}: {path}", t.power_store_import_failed),
        Notice::OptionsSaved(id) => format!("{}: {id}", t.power_store_options_saved),
        Notice::OptionsRejected => t.power_store_options_rejected.to_string(),
        Notice::Copied(id) => format!("{}: {id}", t.power_store_copied),
    }
}

pub(super) fn power_store_operation_name(
    t: &crate::locale::Strings,
    kind: OperationKind,
) -> &'static str {
    match kind {
        OperationKind::Install => t.power_store_install,
        OperationKind::Update => t.power_store_update,
        OperationKind::Uninstall => t.power_store_uninstall,
    }
}

/// Row button caption for the running operation, for example "Updating %45".
fn power_store_row_label(
    t: &crate::locale::Strings,
    kind: OperationKind,
    progress: Option<OperationProgress>,
    cancelling: bool,
) -> String {
    if cancelling {
        return t.power_store_cancelling.to_string();
    }
    let stage = match (kind, progress.map(|progress| progress.stage)) {
        // WinGet reports `Installing` for upgrades; call that "updating" for update rows.
        (OperationKind::Update, None | Some(ProgressStage::Installing)) => t.power_store_updating,
        (OperationKind::Install, None) => t.power_store_stage_installing,
        (OperationKind::Uninstall, None) => t.power_store_stage_uninstalling,
        (_, Some(stage)) => power_store_stage(t, stage),
    };
    match progress.and_then(|progress| progress.percent) {
        Some(percent) => format!("{stage} %{percent}"),
        None => stage.to_string(),
    }
}

fn power_store_stage(t: &crate::locale::Strings, stage: ProgressStage) -> &'static str {
    match stage {
        ProgressStage::Queued => t.power_store_stage_queued,
        ProgressStage::Downloading => t.power_store_stage_downloading,
        ProgressStage::Installing => t.power_store_stage_installing,
        ProgressStage::Uninstalling => t.power_store_stage_uninstalling,
        ProgressStage::Finishing => t.power_store_stage_finishing,
    }
}
