//! PowerStore main workspace: sticky header, unified filter/action bar, sticky column
//! header, scrollable unified package table and the Discover search dialog.
use super::super::settings_ui;
use super::manager_icon::manager_icon;
use super::view_helpers::{button_row, fill_item_width, icon_button_width, icon_label, sized_icon};
use super::{Command, Message, Shell};
use powerkit_app::controller::power_store::{Action, DrawerView, Notice, OpResult, Status};
use powerkit_app::controller::power_store::{PackageRow, RowOperation};
use powerkit_core::power_store::{
    LIST_WORKER_STOPPED, ListMode, ManagerKind, NO_MANAGERS_AVAILABLE, NO_SEARCH_ANSWER,
    OperationKind, OperationProgress, ProgressStage, ScopeFilter, SortKey, StatusFilter,
    WINGET_UNAVAILABLE_MISSING, WINGET_UNAVAILABLE_TOO_OLD, sort_choice_from_index,
    sort_choice_index,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use windows_reactor::*;

const COL_CHECK_WIDTH: f64 = 34.0;
const COL_VERSION_WIDTH: f64 = 168.0;
const SOURCE_ICON_SIZE: f64 = 18.0;
/// Room for the manager badge, its gap and a name such as `powershell7`.
const COL_SOURCE_WIDTH: f64 = 142.0;
const COL_ACTIONS_WIDTH: f64 = 198.0;

/// Compact filter control widths leaving generous width for the search box at 980 DIP.
const FILTER_SOURCE_MIN_WIDTH: f64 = 144.0;
const FILTER_SCOPE_WIDTH: f64 = 152.0;
const FILTER_SORT_WIDTH: f64 = 192.0;

const GLYPH_SEARCH: &str = "\u{E721}";
const GLYPH_HISTORY: &str = "\u{E81C}";
const GLYPH_SETTINGS: &str = "\u{E713}";
const GLYPH_SYNC: &str = "\u{E72C}";
const GLYPH_DELETE: &str = "\u{E74D}";
const GLYPH_IGNORE: &str = "\u{E711}";
const GLYPH_CANCEL: &str = "\u{E711}";
const GLYPH_MORE: &str = "\u{E712}";
const GLYPH_FILTER: &str = "\u{E71C}";
const GLYPH_CHEVRON_DOWN: &str = "\u{E70D}";
const GLYPH_UPDATE: &str = "\u{E777}";
const GLYPH_CHECK: &str = "\u{E73E}";
const GLYPH_ERROR: &str = "\u{EA39}";
const GLYPH_WARNING: &str = "\u{E7BA}";
const GLYPH_STORE: &str = "\u{E719}";
const GLYPH_APPS: &str = "\u{E71D}";
const GLYPH_CLOCK: &str = "\u{E823}";
const GLYPH_PACKAGE: &str = "\u{E7B8}";

/// Muted Fluent-compatible palette with >5:1 contrast against white monogram text
/// in both Light and Dark themes.
const MONOGRAM_PALETTE: [(u8, u8, u8); 8] = [
    (0x2B, 0x5B, 0x84),
    (0x1E, 0x6F, 0x5C),
    (0x5B, 0x4B, 0x8A),
    (0x8C, 0x4A, 0x32),
    (0x2E, 0x6B, 0x4E),
    (0x7A, 0x49, 0x6E),
    (0x3B, 0x69, 0x78),
    (0x7D, 0x5A, 0x2B),
];

/// What a package row needs from the page. It owns its sender and copies, so the table can
/// build rows later, when the virtualized list realizes them, not only during `view`.
#[derive(Clone)]
struct RowScope {
    sender: LocalSender<Message>,
    t: &'static powerkit_core::locale::Strings,
    /// Discover rows offer Install instead of Update and Uninstall.
    discover: bool,
    sort_by: SortKey,
}

impl RowScope {
    fn message(&self, message: Message) -> Callback<()> {
        self.sender.message(message)
    }

    fn callback<T>(&self, map: impl Fn(T) -> Message + 'static) -> Callback<T> {
        self.sender.callback(map)
    }
}

/// One item of the package table, in display order. Rows refer to the snapshot by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableEntry {
    /// Group heading above the update or installed block; `count` is the rows it covers.
    Divider {
        updates: bool,
        count: usize,
    },
    Row(usize),
    /// Notice below the rows when the list was cut at `MAX_VISIBLE_PACKAGES`.
    Truncated,
}

/// Lays out dividers and rows. Dividers appear only in the unified installed list and
/// only when some package has an actionable update.
fn table_entries(packages: &[PackageRow], grouped: bool, truncated: bool) -> Vec<TableEntry> {
    let updatable = packages.iter().filter(|row| row.actionable_update).count();
    let regular = packages.len() - updatable;
    let grouped = grouped && updatable > 0;
    let mut entries = Vec::with_capacity(packages.len() + 3);
    let (mut shown_updates, mut shown_installed) = (false, false);
    for (index, package) in packages.iter().enumerate() {
        if grouped {
            if package.actionable_update && !shown_updates {
                shown_updates = true;
                entries.push(TableEntry::Divider {
                    updates: true,
                    count: updatable,
                });
            } else if !package.actionable_update && !shown_installed {
                shown_installed = true;
                entries.push(TableEntry::Divider {
                    updates: false,
                    count: regular,
                });
            }
        }
        entries.push(TableEntry::Row(index));
    }
    if truncated {
        entries.push(TableEntry::Truncated);
    }
    entries
}

/// Stable identity of an item; the virtualized list reuses realized items by this key.
fn entry_key(entry: TableEntry, packages: &[PackageRow]) -> String {
    match entry {
        TableEntry::Divider { updates: true, .. } => "grp-updates".to_string(),
        TableEntry::Divider { updates: false, .. } => "grp-installed".to_string(),
        TableEntry::Row(index) => {
            let key = &packages[index].key;
            format!("{}|{}", key.source, key.id)
        }
        TableEntry::Truncated => "truncated".to_string(),
    }
}

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
        if let Some(strip) = self.power_store_refresh_strip() {
            top_chrome.push(strip);
        }
        if let Some(banner) = self.power_store_discover_banner(context) {
            top_chrome.push(banner);
        }
        if let Some(strip) = self.power_store_live_strip(context) {
            top_chrome.push(strip);
        }
        if let Some(error_card) = self.power_store_error_card(context) {
            top_chrome.push(error_card);
        }
        top_chrome.push(self.power_store_toolbar(context));
        top_chrome.push(self.power_store_table_header(context));

        let sticky_top = StackPanel::new()
            .grid_row(0)
            .margin(Thickness::new(24.0, 16.0, 24.0, 0.0))
            .children((settings_ui::stack(top_chrome, 10.0),));

        let table_scroll = ScrollViewer::new()
            .grid_row(1)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            // The virtualized list is the scroller's direct content so it realizes only the
            // rows in view.
            .content(self.power_store_table_body(context));

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
        let history_open = matches!(store.drawer, DrawerView::History);
        let settings_open = matches!(store.drawer, DrawerView::Settings | DrawerView::Manager(_));

        // Title + live status pill beside it
        let mut title_items: Vec<View> =
            vec![settings_ui::heading(t.power_store_title, 26.0).into()];
        if let Some(pill) = self.power_store_status_pill() {
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

        let subtitle_row = self.power_store_header_subtitle();

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
                    .children((title_row, subtitle_row)),
                StackPanel::new()
                    .grid_column(1)
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
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
                        separator,
                        Button::new()
                            .style(if history_open {
                                ButtonStyle::Accent
                            } else {
                                ButtonStyle::Subtle
                            })
                            .on_click(
                                context.message(Message::PowerStore(Action::ToggleHistoryDrawer)),
                            )
                            .content(icon_label(GLYPH_HISTORY, t.power_store_history)),
                        Button::new()
                            .style(if settings_open {
                                ButtonStyle::Accent
                            } else {
                                ButtonStyle::Subtle
                            })
                            .on_click(
                                context.message(Message::PowerStore(Action::ToggleSettingsDrawer)),
                            )
                            .content(icon_label(GLYPH_SETTINGS, t.power_store_settings)),
                    )),
            ))
    }

    /// Compact live status badge next to the workspace title: checking spinner or calm
    /// "up to date" confirmation. Update counts live on the status filter pills below.
    fn power_store_status_pill(&self) -> Option<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        if store.check.checking {
            return Some(
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
                                    .text(t.power_store_checking)
                                    .font_size(11.5)
                                    .opacity(0.85)
                                    .vertical_alignment(VerticalAlignment::Center),
                            )),
                    ),
            );
        }
        if store.check.count == Some(0) && store.total_installed > 0 {
            return Some(
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
                                    .text(t.power_store_up_to_date)
                                    .font_size(11.5)
                                    .opacity(0.8)
                                    .vertical_alignment(VerticalAlignment::Center),
                            )),
                    ),
            );
        }
        None
    }

    fn power_store_header_subtitle(&self) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();

        if matches!(store.status, Status::Loading) && store.packages.is_empty() {
            return settings_ui::caption(t.power_store_loading);
        }

        let mut items: Vec<View> = Vec::new();
        // Keep running/pending operations visible even if filtered out of the table.
        if let Some(running) = &store.op.running {
            let mut active = format!(
                "{}: {}",
                running.name,
                power_store_row_label(t, running.kind, store.op.progress, store.op.cancelling)
            );
            if store.op.pending > 0 {
                active.push_str(&format!(
                    " · {} {}",
                    store.op.pending, t.power_store_queue_pending
                ));
            }
            items.push(
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
                            .text(active)
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .foreground(ThemeBrush::AccentText)
                            .vertical_alignment(VerticalAlignment::Center),
                    )),
            );
        } else if store.op.pending > 0 {
            items.push(
                TextBlock::new()
                    .text(format!(
                        "{} {}",
                        store.op.pending, t.power_store_queue_pending
                    ))
                    .font_size(12.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(ThemeBrush::AccentText)
                    .vertical_alignment(VerticalAlignment::Center)
                    .into(),
            );
        }

        let mut meta = Vec::new();
        if store.total_installed > 0 {
            meta.push(format!(
                "{}: {}",
                t.power_store_installed, store.total_installed
            ));
        }
        if let Some(stamp) = &store.check.last_checked {
            meta.push(format!("{}: {stamp}", t.power_store_last_checked));
        }
        if !meta.is_empty() {
            if !items.is_empty() {
                items.push(
                    TextBlock::new()
                        .text("·")
                        .font_size(12.5)
                        .opacity(0.45)
                        .vertical_alignment(VerticalAlignment::Center)
                        .into(),
                );
            }
            items.push(
                TextBlock::new()
                    .text(meta.join("  ·  "))
                    .font_size(12.5)
                    .opacity(0.72)
                    .vertical_alignment(VerticalAlignment::Center)
                    .into(),
            );
        }

        if items.is_empty() {
            return TextBlock::new()
                .text(t.power_store_lead)
                .font_size(12.5)
                .opacity(0.72)
                .text_wrapping(TextWrapping::Wrap)
                .into();
        }

        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .keyed_children(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| KeyedView::new(index, item)),
            )
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
        let headline = format!(
            "{}: \"{}\"",
            t.power_store_discover_results, store.discover_query
        );
        let meta = format!(
            "{source}  ·  {} {}",
            store.visible_count, t.power_store_showing
        );

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
                            accent_bar(28.0).grid_column(0),
                            StackPanel::new()
                                .grid_column(1)
                                .spacing(2.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((
                                    StackPanel::new()
                                        .orientation(Orientation::Horizontal)
                                        .spacing(7.0)
                                        .vertical_alignment(VerticalAlignment::Center)
                                        .children((
                                            sized_icon(GLYPH_SEARCH, 13.0),
                                            TextBlock::new()
                                                .text(headline)
                                                .font_weight(FontWeight::SEMI_BOLD)
                                                .text_trimming(TextTrimming::CharacterEllipsis)
                                                .max_lines(1),
                                        )),
                                    TextBlock::new().text(meta).font_size(12.0).opacity(0.72),
                                )),
                            StackPanel::new()
                                .grid_column(2)
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

    /// Slim non-intrusive bar shown while a background refresh updates the cached list.
    fn power_store_refresh_strip(&self) -> Option<View> {
        let store = &self.snapshot.power_store;
        if !store.refreshing {
            return None;
        }
        let t = self.t();
        let text = if store.from_cache {
            t.power_store_refreshing_cached
        } else {
            t.power_store_refreshing
        };
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
                            ProgressRing::new()
                                .grid_column(0)
                                .width(14.0)
                                .height(14.0)
                                .is_active(true)
                                .is_indeterminate(true)
                                .vertical_alignment(VerticalAlignment::Center),
                            TextBlock::new()
                                .grid_column(1)
                                .text(text)
                                .font_size(12.0)
                                .opacity(0.85)
                                .text_wrapping(TextWrapping::Wrap)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                ),
        )
    }

    /// Failure and one-shot notices only. Running and waiting operations appear on the
    /// package's own table row, never as a banner above the filters.
    fn power_store_live_strip(&self, context: &ViewContext<Self>) -> Option<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let (headline, is_error) = if let Some(notice) = &store.notice {
            (power_store_notice(t, notice), notice_is_error(notice))
        } else if let Some((label, OpResult::Failed(error))) = &store.op.last {
            (
                format!(
                    "{}: {} · {} · {error}",
                    power_store_operation_name(t, label.kind),
                    label.name,
                    t.power_store_operation_failed
                ),
                true,
            )
        } else {
            return None;
        };

        let (bg, border, glyph): (Brush, Brush, &str) = if is_error {
            (
                ThemeBrush::SystemCriticalBackground.into(),
                ThemeBrush::SystemCritical.into(),
                GLYPH_ERROR,
            )
        } else {
            (
                ThemeBrush::CardBackground.into(),
                ThemeBrush::CardStroke.into(),
                GLYPH_CHECK,
            )
        };
        let leading: View = if is_error {
            sized_icon(glyph, 14.0)
        } else {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(10.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((accent_bar(18.0), sized_icon(glyph, 14.0)))
        };
        let message = TextBlock::new()
            .grid_column(1)
            .text(headline)
            .font_size(12.5)
            .text_wrapping(TextWrapping::Wrap)
            .vertical_alignment(VerticalAlignment::Center);
        let message: View = if is_error {
            message.foreground(ThemeBrush::SystemCritical).into()
        } else {
            message.into()
        };

        Some(
            Border::new()
                .background(bg)
                .border_brush(border)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(12.0, 8.0))
                .content(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                        .column_spacing(10.0)
                        .children((
                            StackPanel::new()
                                .grid_column(0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((leading,)),
                            message,
                            Button::new()
                                .grid_column(2)
                                .width(icon_button_width(11.0, 30.0))
                                .height(28.0)
                                .style(ButtonStyle::Subtle)
                                .vertical_alignment(VerticalAlignment::Center)
                                .on_click(
                                    context.message(Message::PowerStore(Action::DismissNotice)),
                                )
                                .content(sized_icon(GLYPH_CANCEL, 11.0))
                                .tooltip(t.power_store_close),
                        )),
                ),
        )
    }

    fn power_store_error_card(&self, context: &ViewContext<Self>) -> Option<View> {
        let t = self.t();
        let (title, detail, offer_settings) = match &self.snapshot.power_store.status {
            Status::Unavailable(message) => match message.as_str() {
                WINGET_UNAVAILABLE_MISSING => (
                    t.power_store_unavailable_missing,
                    t.power_store_unavailable_missing_detail,
                    false,
                ),
                WINGET_UNAVAILABLE_TOO_OLD => (
                    t.power_store_unavailable_too_old,
                    t.power_store_unavailable_too_old_detail,
                    false,
                ),
                NO_MANAGERS_AVAILABLE => (
                    t.power_store_no_managers,
                    t.power_store_no_managers_detail,
                    true,
                ),
                _ => (t.power_store_unavailable, message.as_str(), false),
            },
            Status::Failed(message) => (t.power_store_failed, list_error_text(t, message), false),
            _ => return None,
        };

        let action: View = if offer_settings {
            Button::new()
                .grid_column(2)
                .style(ButtonStyle::Accent)
                .vertical_alignment(VerticalAlignment::Center)
                .on_click(context.message(Message::PowerStore(Action::ToggleSettingsDrawer)))
                .content(icon_label(GLYPH_SETTINGS, t.power_store_settings))
        } else {
            StackPanel::new().grid_column(2).into()
        };

        Some(
            Border::new()
                .background(ThemeBrush::SystemCriticalBackground)
                .border_brush(ThemeBrush::SystemCritical)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(14.0, 12.0))
                .content(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                        .column_spacing(12.0)
                        .children((
                            StackPanel::new()
                                .grid_column(0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((sized_icon(GLYPH_WARNING, 18.0),)),
                            StackPanel::new()
                                .grid_column(1)
                                .spacing(2.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((
                                    TextBlock::new()
                                        .text(title)
                                        .font_size(14.5)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .foreground(ThemeBrush::SystemCritical)
                                        .text_wrapping(TextWrapping::Wrap),
                                    settings_ui::caption(detail),
                                )),
                            action,
                        )),
                ),
        )
    }

    fn power_store_toolbar(&self, context: &ViewContext<Self>) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let installed_mode = store.mode == ListMode::Installed;
        let loading = matches!(store.status, Status::Loading) || store.refreshing;

        let source_filter = self.power_store_source_filter(context);

        // Scope filter ComboBox
        let scope_labels = [
            t.power_store_scope_all.to_string(),
            t.power_store_scope_user.to_string(),
            t.power_store_scope_machine.to_string(),
        ];
        let current_scope = store.query.scope;
        let scope_combo = ComboBox::new()
            .grid_column(2)
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
            .grid_column(3)
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

        // Generous local filter input with leading search glyph and optional clear button
        let has_filter_text = !store.query.filter_text.is_empty();
        let clear_btn: View = if has_filter_text {
            Button::new()
                .grid_column(2)
                .width(icon_button_width(11.0, 32.0))
                .height(32.0)
                .style(ButtonStyle::Subtle)
                .on_click(
                    context.message(Message::PowerStore(Action::SetFilterText(String::new()))),
                )
                .content(sized_icon(GLYPH_IGNORE, 11.0))
                .tooltip(t.power_store_close)
        } else {
            StackPanel::new().grid_column(2).into()
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
                    .text(store.query.filter_text.clone())
                    .placeholder_text(t.power_store_filter_hint)
                    .on_text_changed(context.callback(Message::PowerStoreFilterText)),
                clear_btn,
            ));

        let filters_row = Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::Auto,
                GridLength::Pixel(FILTER_SCOPE_WIDTH),
                GridLength::Pixel(FILTER_SORT_WIDTH),
            ])
            .column_spacing(8.0)
            .children((
                search_box,
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((source_filter,)),
                scope_combo,
                sort_combo,
            ));

        let row_divider = Border::new()
            .height(1.0)
            .background(ThemeBrush::CardStroke)
            .opacity(0.55);

        let actions_row = if store.selected_count > 0 {
            self.power_store_selection_bar(context, installed_mode)
        } else {
            self.power_store_browse_bar(context, installed_mode, loading)
        };

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

    /// Second toolbar row when no packages are checked: one-click status filter pills on
    /// the left and primary update/refresh actions on the right.
    fn power_store_browse_bar(
        &self,
        context: &ViewContext<Self>,
        installed_mode: bool,
        loading: bool,
    ) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();

        let left: View = if installed_mode {
            let update_count = store.check.count.unwrap_or(0);
            let updates_label = if update_count > 0 {
                format!("{} ({update_count})", t.power_store_status_updates)
            } else {
                t.power_store_status_updates.to_string()
            };
            let pinned_label = if store.pinned_count > 0 {
                format!("{} ({})", t.power_store_status_pinned, store.pinned_count)
            } else {
                t.power_store_status_pinned.to_string()
            };
            let pill = |filter: StatusFilter, label: String| -> View {
                let active = store.query.status == filter;
                Button::new()
                    .style(if active {
                        ButtonStyle::Accent
                    } else {
                        ButtonStyle::Subtle
                    })
                    .on_click(context.message(Message::PowerStore(Action::SetStatusFilter(filter))))
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
            Border::new()
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
                        .children((
                            pill(StatusFilter::All, t.power_store_status_all.to_string()),
                            pill(StatusFilter::UpdatesOnly, updates_label),
                            pill(
                                StatusFilter::UpToDate,
                                t.power_store_status_current.to_string(),
                            ),
                            pill(StatusFilter::Pinned, pinned_label),
                        )),
                )
        } else {
            settings_ui::caption(&format!(
                "{} {}",
                store.visible_count, t.power_store_showing
            ))
        };

        let mut action_buttons: Vec<View> = Vec::new();
        if installed_mode {
            let visible_updates = store.visible_updatable_count;
            let update_label = if visible_updates > 0 {
                format!("{} ({visible_updates})", t.power_store_update_all)
            } else {
                t.power_store_update_all.to_string()
            };
            action_buttons.push(
                Button::new()
                    .style(ButtonStyle::Accent)
                    .is_enabled(visible_updates > 0)
                    .on_click(context.message(Message::PowerStore(Action::QueueVisibleUpdates)))
                    .content(icon_label(GLYPH_UPDATE, &update_label)),
            );
        } else {
            action_buttons.push(
                Button::new()
                    .style(ButtonStyle::Accent)
                    .is_enabled(false)
                    .content(t.power_store_install_selected),
            );
        }

        if store.op.pending > 0 {
            // Keep Refresh compact as an icon button while Cancel pending shares the row.
            action_buttons.push(
                Button::new()
                    .on_click(context.message(Message::Command(Command::PowerStoreCancelPending)))
                    .content(format!(
                        "{} ({})",
                        t.power_store_cancel_pending, store.op.pending
                    )),
            );
            action_buttons.push(
                Button::new()
                    .width(icon_button_width(13.0, 34.0))
                    .height(32.0)
                    .is_enabled(!loading)
                    .on_click(context.message(Message::Command(Command::PowerStoreRefresh)))
                    .content(sized_icon(GLYPH_SYNC, 13.0))
                    .tooltip(t.power_store_refresh),
            );
        } else {
            action_buttons.push(
                Button::new()
                    .is_enabled(!loading)
                    .on_click(context.message(Message::Command(Command::PowerStoreRefresh)))
                    .content(icon_label(GLYPH_SYNC, t.power_store_refresh)),
            );
        }

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((left,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((button_row(action_buttons),)),
            ))
    }

    /// Second toolbar row when one or more rows are checked: contextual bulk actions
    /// appear without cluttering the normal browsing state.
    fn power_store_selection_bar(&self, context: &ViewContext<Self>, installed_mode: bool) -> View {
        let store = &self.snapshot.power_store;
        let t = self.t();

        let left = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(10.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                accent_bar(20.0),
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
                    .content(icon_label(GLYPH_CANCEL, t.power_store_clear_selection)),
            ));

        let primary_action = if !installed_mode {
            Button::new()
                .style(ButtonStyle::Accent)
                .is_enabled(store.selected_count > 0)
                .on_click(context.message(Message::PowerStore(Action::QueueSelected(
                    OperationKind::Install,
                ))))
                .content(format!(
                    "{} ({})",
                    t.power_store_install_selected, store.selected_count
                ))
        } else {
            Button::new()
                .style(ButtonStyle::Accent)
                .is_enabled(store.selected_updatable_count > 0)
                .on_click(context.message(Message::PowerStore(Action::QueueSelected(
                    OperationKind::Update,
                ))))
                .content(icon_label(
                    GLYPH_UPDATE,
                    &format!(
                        "{} ({})",
                        t.power_store_update_selected, store.selected_updatable_count
                    ),
                ))
        };

        let mut action_buttons = vec![primary_action];
        if installed_mode {
            action_buttons.push(
                Button::new()
                    .is_enabled(store.selected_removable_count > 0)
                    .on_click(context.message(Message::PowerStore(Action::QueueSelected(
                        OperationKind::Uninstall,
                    ))))
                    .content(icon_label(GLYPH_DELETE, t.power_store_uninstall)),
            );
            action_buttons.push(
                Button::new()
                    .is_enabled(store.selected_removable_count > 0)
                    .on_click(context.message(Message::PowerStore(Action::IgnoreSelected)))
                    .content(icon_label(GLYPH_IGNORE, t.power_store_ignore)),
            );
        }
        if store.op.pending > 0 {
            action_buttons.push(
                Button::new()
                    .on_click(context.message(Message::Command(Command::PowerStoreCancelPending)))
                    .content(format!(
                        "{} ({})",
                        t.power_store_cancel_pending, store.op.pending
                    )),
            );
        }

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((left,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((button_row(action_buttons),)),
            ))
    }

    /// Multi-select source filter: a dropdown-styled button whose flyout lists one
    /// icon-labelled checkbox per available source.
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
                let item_content = StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        source_badge_icon(source, 16.0),
                        TextBlock::new()
                            .text(source.as_str())
                            .vertical_alignment(VerticalAlignment::Center),
                    ));
                CheckBox::new()
                    .is_checked(picked.contains(&source.to_ascii_lowercase()))
                    .on_is_checked_changed(context.callback(move |checked| {
                        Message::PowerStore(Action::SetSourceChecked(value.clone(), checked))
                    }))
                    .content(item_content)
            })
            .collect();
        rows.push(
            Border::new()
                .height(1.0)
                .background(ThemeBrush::CardStroke)
                .margin(Thickness::xy(0.0, 4.0))
                .into(),
        );
        rows.push(
            Button::new()
                .style(ButtonStyle::Subtle)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .is_enabled(!picked.is_empty())
                .on_click(context.message(Message::PowerStore(Action::ClearSourceFilter)))
                .content(t.power_store_clear_sources),
        );

        let button_content = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(7.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((sized_icon(GLYPH_FILTER, 12.0),)),
                TextBlock::new()
                    .grid_column(1)
                    .text(label)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_trimming(TextTrimming::CharacterEllipsis),
                StackPanel::new()
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .opacity(0.7)
                    .children((sized_icon(GLYPH_CHEVRON_DOWN, 10.0),)),
            ));

        Button::new()
            .min_width(FILTER_SOURCE_MIN_WIDTH)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Stretch)
            .style(if picked.is_empty() {
                ButtonStyle::Default
            } else {
                ButtonStyle::Accent
            })
            .content(button_content)
            .flyout_with(
                Flyout::rich(
                    StackPanel::new()
                        .min_width(210.0)
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
                .opacity(0.72)
                .vertical_alignment(VerticalAlignment::Center)
                .horizontal_alignment(if right {
                    HorizontalAlignment::Right
                } else {
                    HorizontalAlignment::Left
                })
        };

        let mut app_header_label = if store.visible_count > 0 {
            format!("{} ({})", t.power_store_col_app, store.visible_count)
        } else {
            t.power_store_col_app.to_string()
        };
        if store.query.sort_by == SortKey::Name {
            app_header_label.push_str(if store.query.sort_ascending {
                " ↑"
            } else {
                " ↓"
            });
        }

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
        let margin = Thickness::new(24.0, 0.0, 24.0, 20.0);

        if store.packages.is_empty() {
            let empty_content: View = if matches!(store.status, Status::Loading) {
                StackPanel::new()
                    .spacing(12.0)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .children((
                        ProgressRing::new()
                            .width(28.0)
                            .height(28.0)
                            .is_active(true)
                            .is_indeterminate(true)
                            .horizontal_alignment(HorizontalAlignment::Center),
                        TextBlock::new()
                            .text(t.power_store_loading)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .horizontal_alignment(HorizontalAlignment::Center),
                        TextBlock::new()
                            .text(t.power_store_lead)
                            .font_size(12.5)
                            .opacity(0.72)
                            .text_wrapping(TextWrapping::Wrap)
                            .horizontal_alignment(HorizontalAlignment::Center),
                    ))
            } else {
                let filtered = !store.query.filter_text.is_empty()
                    || !store.query.sources.is_empty()
                    || store.query.status != StatusFilter::All
                    || store.query.scope != ScopeFilter::All;
                let glyph = if filtered || store.mode == ListMode::Discover {
                    GLYPH_SEARCH
                } else {
                    GLYPH_PACKAGE
                };
                StackPanel::new()
                    .spacing(10.0)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .children((
                        StackPanel::new()
                            .opacity(0.45)
                            .horizontal_alignment(HorizontalAlignment::Center)
                            .children((sized_icon(glyph, 32.0),)),
                        TextBlock::new()
                            .text(t.power_store_empty)
                            .font_size(14.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .horizontal_alignment(HorizontalAlignment::Center),
                    ))
            };
            return Border::new()
                .margin(margin)
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
                .corner_radius(CornerRadius::new(0.0, 0.0, 8.0, 8.0))
                .padding(Thickness::xy(24.0, 44.0))
                .content(empty_content);
        }

        let entries = table_entries(
            &store.packages,
            store.mode == ListMode::Installed && store.query.status == StatusFilter::All,
            store.truncated,
        );
        let keys: Rc<[String]> = entries
            .iter()
            .map(|entry| entry_key(*entry, &store.packages))
            .collect();
        let mut hasher = DefaultHasher::new();
        keys.hash(&mut hasher);
        let key_revision = hasher.finish();

        let scope = RowScope {
            sender: context.sender(),
            t,
            discover: store.mode == ListMode::Discover,
            sort_by: store.query.sort_by,
        };
        let packages = Rc::clone(&store.packages);
        let entries: Rc<[TableEntry]> = entries.into();
        let count = entries.len();
        let source = VirtualSource::new(
            key_revision,
            count,
            move |index: usize| keys[index].clone(),
            move |index: usize| {
                Self::table_item(&scope, entries[index], &packages, index + 1 == count)
            },
        );
        ItemsRepeater::new()
            .virtual_source(source)
            .margin(margin)
            .into()
    }

    /// Builds one realized item of the table. Takes no `self`: it runs after `view`, from the
    /// virtualized list, so everything it needs comes from `scope` and the shared rows.
    fn table_item(
        scope: &RowScope,
        entry: TableEntry,
        packages: &[PackageRow],
        is_last: bool,
    ) -> View {
        let t = scope.t;
        let item = match entry {
            TableEntry::Divider { updates, count } => {
                let label = if updates {
                    t.power_store_group_updates
                } else {
                    t.power_store_group_installed
                };
                group_divider(label, count, updates)
            }
            // A row is last only when no truncation notice follows it.
            TableEntry::Row(index) => Self::power_store_table_row(scope, &packages[index], is_last),
            TableEntry::Truncated => Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
                .corner_radius(CornerRadius::new(0.0, 0.0, 8.0, 8.0))
                .padding(Thickness::xy(14.0, 10.0))
                .content(settings_ui::caption(t.power_store_truncated)),
        };
        fill_item_width(item)
    }

    fn power_store_table_row(context: &RowScope, package: &PackageRow, is_last: bool) -> View {
        let t = context.t;
        let own = package.key.is_own_package();

        // Column 1: Framed 32px logo + single-line App Name with crisp pill tags + subtle metadata.
        // Keep the name TextBlock out of a horizontal StackPanel so CharacterEllipsis measures
        // against finite column width rather than clipping mid-glyph.
        let mut badge_items: Vec<View> = Vec::new();
        if package.ignored {
            badge_items.push(row_pill_badge(
                t.power_store_badge_ignored,
                BadgeTone::Muted,
            ));
        } else if package.skipped {
            badge_items.push(row_pill_badge(
                t.power_store_badge_skipped,
                BadgeTone::Warning,
            ));
        }
        if package.configured {
            badge_items.push(row_pill_badge(t.power_store_configured, BadgeTone::Subtle));
        }
        let name_block = TextBlock::new()
            .text(package.name.as_str())
            .font_size(13.5)
            .font_weight(FontWeight::SEMI_BOLD)
            .text_trimming(TextTrimming::CharacterEllipsis)
            .max_lines(1)
            .vertical_alignment(VerticalAlignment::Center);
        let title_line: View = if badge_items.is_empty() {
            name_block.into()
        } else {
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    name_block.grid_column(0),
                    StackPanel::new()
                        .grid_column(1)
                        .orientation(Orientation::Horizontal)
                        .spacing(5.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .keyed_children(
                            badge_items
                                .into_iter()
                                .enumerate()
                                .map(|(index, view)| KeyedView::new(index, view)),
                        ),
                ))
        };

        let mut subtitle = package.key.id.clone();
        match context.sort_by {
            SortKey::UpdatedDate if !package.updated_at.is_empty() => {
                subtitle.push_str("  ·  ");
                subtitle.push_str(&package.updated_at);
            }
            SortKey::InstalledDate if !package.install_date.is_empty() => {
                subtitle.push_str("  ·  ");
                subtitle.push_str(&package.install_date);
            }
            _ if !package.install_date.is_empty() => {
                subtitle.push_str("  ·  ");
                subtitle.push_str(&package.install_date);
            }
            _ => {}
        }

        let name_text = StackPanel::new().spacing(2.0).children((
            title_line,
            TextBlock::new()
                .text(subtitle)
                .font_size(11.5)
                .opacity(0.62)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1),
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
            .column_spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((package_logo_tile(
                        &package.name,
                        package.icon.as_ref(),
                        LOGO_TILE_SIZE,
                    ),)),
                open_details,
            ));

        // Column 2: Version (stacked current -> available with AccentText when actionable)
        fn dash(value: &str) -> &str {
            if value.is_empty() { "—" } else { value }
        }
        let version_cell: View = if package.installed && !package.available.is_empty() {
            let available_text = TextBlock::new()
                .text(format!("→ {}", package.available))
                .font_size(12.5)
                .font_weight(if package.actionable_update {
                    FontWeight::SEMI_BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1);
            let available_view: View = if package.actionable_update {
                available_text.foreground(ThemeBrush::AccentText).into()
            } else {
                available_text.opacity(0.75).into()
            };
            StackPanel::new()
                .grid_column(2)
                .spacing(1.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(dash(&package.version))
                        .font_size(11.5)
                        .opacity(0.58)
                        .text_trimming(TextTrimming::CharacterEllipsis)
                        .max_lines(1),
                    available_view,
                ))
        } else {
            let shown = if package.installed {
                dash(&package.version)
            } else {
                dash(&package.available)
            };
            TextBlock::new()
                .grid_column(2)
                .text(shown)
                .font_size(12.5)
                .opacity(0.82)
                .vertical_alignment(VerticalAlignment::Center)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1)
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
                    TextBlock::new()
                        .text(source_name)
                        .font_size(12.5)
                        .text_trimming(TextTrimming::CharacterEllipsis)
                        .max_lines(1),
                    TextBlock::new()
                        .text(scope_label)
                        .font_size(10.5)
                        .opacity(0.58)
                        .text_trimming(TextTrimming::CharacterEllipsis)
                        .max_lines(1),
                )),
            None => TextBlock::new()
                .text(source_name)
                .font_size(12.5)
                .vertical_alignment(VerticalAlignment::Center)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1)
                .into(),
        };
        let source_icon: View = if package.store_managed {
            sized_icon(GLYPH_STORE, SOURCE_ICON_SIZE - 2.0)
        } else {
            manager_icon(package.key.manager, SOURCE_ICON_SIZE)
        };
        let source_cell: View = Grid::new()
            .grid_column(3)
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((source_icon,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((source_text,)),
            ));

        // Column 4: Primary CTA + Overflow Menu (`⋯`)
        let row_operation = package.operation;
        let primary_btn = if let Some(operation) = row_operation {
            // The queue state lives on the package's own row: inline ring + stage/percent
            // while running, clock + "waiting" while queued.
            match operation {
                RowOperation::Running {
                    kind,
                    progress,
                    cancelling,
                } => {
                    let label = power_store_row_label(t, kind, progress, cancelling);
                    let ring = match progress.and_then(|p| p.percent) {
                        Some(percent) if !cancelling => ProgressRing::new()
                            .width(12.0)
                            .height(12.0)
                            .is_active(true)
                            .is_indeterminate(false)
                            .minimum(0.0)
                            .maximum(100.0)
                            .value(f64::from(percent)),
                        _ => ProgressRing::new()
                            .width(12.0)
                            .height(12.0)
                            .is_active(true)
                            .is_indeterminate(true),
                    };
                    Button::new().is_enabled(false).content(
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(6.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                ring,
                                TextBlock::new()
                                    .text(label)
                                    .font_size(12.5)
                                    .vertical_alignment(VerticalAlignment::Center),
                            )),
                    )
                }
                RowOperation::Pending { .. } => Button::new().is_enabled(false).content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(6.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            sized_icon(GLYPH_CLOCK, 12.0),
                            TextBlock::new()
                                .text(t.power_store_waiting)
                                .font_size(12.5)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                ),
            }
        } else if context.discover {
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
            // Subtle style keeps up-to-date rows visually calm so Accent update buttons stand out.
            Button::new()
                .style(ButtonStyle::Subtle)
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
                .width(icon_button_width(12.0, 32.0))
                .height(32.0)
                .style(ButtonStyle::Subtle)
                .is_enabled(!cancelling)
                .on_click(context.message(Message::Command(Command::PowerStoreCancelRunning)))
                .content(sized_icon(GLYPH_CANCEL, 12.0))
                .tooltip(t.power_store_cancel_running),
            Some(RowOperation::Pending { index, .. }) => Button::new()
                .width(icon_button_width(12.0, 32.0))
                .height(32.0)
                .style(ButtonStyle::Subtle)
                .on_click(context.message(Message::PowerStore(Action::RemovePending(index))))
                .content(sized_icon(GLYPH_CANCEL, 12.0))
                .tooltip(t.power_store_remove_pending),
            None => Self::row_overflow_menu(context, package),
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

        let row_grid = Grid::new()
            .columns(table_columns())
            .column_spacing(12.0)
            .children((
                CheckBox::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .is_checked(package.selected)
                    .on_is_checked_changed(context.callback(move |selected| {
                        Message::PowerStore(Action::SetSelected(check_key.clone(), selected))
                    })),
                app_cell,
                version_cell,
                source_cell,
                actions_cell,
            ));

        let mut layers: Vec<KeyedView> = vec![KeyedView::new(
            "row-content",
            Border::new()
                .padding(Thickness::xy(14.0, 8.0))
                .content(row_grid),
        )];
        if package.selected {
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
        if let Some(RowOperation::Running {
            progress,
            cancelling,
            ..
        }) = row_operation
        {
            let bar = match progress.and_then(|p| p.percent) {
                Some(percent) if !cancelling => ProgressBar::new()
                    .height(2.0)
                    .minimum(0.0)
                    .maximum(100.0)
                    .value(f64::from(percent))
                    .is_indeterminate(false),
                _ => ProgressBar::new().height(2.0).is_indeterminate(true),
            };
            layers.push(KeyedView::new(
                "row-progress",
                bar.vertical_alignment(VerticalAlignment::Bottom),
            ));
        }

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
            .corner_radius(radius)
            .content(Grid::new().keyed_children(layers))
    }

    fn row_overflow_menu(context: &RowScope, package: &PackageRow) -> View {
        let t = context.t;
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
            .width(icon_button_width(14.0, 32.0))
            .height(32.0)
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
        use powerkit_app::controller::power_store::ManagerPresence;
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

        let field_label = |label: &str| {
            TextBlock::new()
                .text(label)
                .font_size(12.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .opacity(0.8)
        };

        let dialog_body = Grid::new()
            .min_width(440.0)
            .key_accelerators(KeyAccelerators::new([KeyAccelerator::new(
                AcceleratorKey::Enter,
                AcceleratorModifiers::None,
                context.message(Message::SubmitDiscover),
            )]))
            .children((StackPanel::new().spacing(14.0).children((
                settings_ui::caption(t.power_store_discover_dialog_lead),
                StackPanel::new().spacing(6.0).children((
                    field_label(t.power_store_search_hint),
                    TextBox::new()
                        .text(self.discover_query_draft.clone())
                        .placeholder_text(t.power_store_search_hint)
                        .on_text_changed(context.callback(Message::EditDiscoverQuery)),
                )),
                StackPanel::new().spacing(6.0).children((
                    field_label(t.power_store_source),
                    ComboBox::new()
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .items_source(source_options)
                        .selected_index(self.discover_source_index)
                        .on_selection_changed(context.callback(Message::PickDiscoverSource)),
                )),
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

const LOGO_TILE_SIZE: f64 = 32.0;

#[derive(Clone, Copy)]
enum BadgeTone {
    Muted,
    Warning,
    Subtle,
}

fn row_pill_badge(label: &str, tone: BadgeTone) -> View {
    match tone {
        BadgeTone::Muted => Border::new()
            .corner_radius(CornerRadius::uniform(4.0))
            .padding(Thickness::xy(6.0, 1.0))
            .background(Color::rgb(0x6E, 0x6E, 0x6E))
            .vertical_alignment(VerticalAlignment::Center)
            .content(
                TextBlock::new()
                    .text(label)
                    .font_size(10.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(Color::rgb(0xFF, 0xFF, 0xFF)),
            ),
        BadgeTone::Warning => Border::new()
            .corner_radius(CornerRadius::uniform(4.0))
            .padding(Thickness::xy(6.0, 1.0))
            .background(Color::rgb(0x9A, 0x67, 0x00))
            .vertical_alignment(VerticalAlignment::Center)
            .content(
                TextBlock::new()
                    .text(label)
                    .font_size(10.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(Color::rgb(0xFF, 0xFF, 0xFF)),
            ),
        BadgeTone::Subtle => Border::new()
            .corner_radius(CornerRadius::uniform(4.0))
            .padding(Thickness::xy(6.0, 1.0))
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .vertical_alignment(VerticalAlignment::Center)
            .content(TextBlock::new().text(label).font_size(10.5).opacity(0.8)),
    }
}

fn accent_bar(height: f64) -> Border {
    Border::new()
        .width(3.0)
        .height(height)
        .corner_radius(CornerRadius::uniform(2.0))
        .background(ThemeBrush::Accent)
        .vertical_alignment(VerticalAlignment::Center)
}

fn monogram_color(name: &str) -> Color {
    let mut hasher = DefaultHasher::new();
    name.to_ascii_lowercase().hash(&mut hasher);
    let (r, g, b) = MONOGRAM_PALETTE[(hasher.finish() as usize) % MONOGRAM_PALETTE.len()];
    Color::rgb(r, g, b)
}

/// Framed square package logo tile, or a deterministic muted-Fluent monogram tile when
/// no local or remote icon has loaded yet.
pub(super) fn package_logo_tile(name: &str, icon: Option<&Arc<[u8]>>, tile_size: f64) -> View {
    let radius = (tile_size * 0.22).round().max(6.0);
    match icon {
        Some(png) => {
            let inner = (tile_size - 8.0).max(16.0);
            Border::new()
                .width(tile_size)
                .height(tile_size)
                .corner_radius(CornerRadius::uniform(radius))
                .background(ThemeBrush::SolidBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .content(
                    Image::new()
                        .width(inner)
                        .height(inner)
                        .stretch(Stretch::Uniform)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .vertical_alignment(VerticalAlignment::Center)
                        .source_data(EncodedImage::new(Arc::clone(png))),
                )
        }
        None => {
            let initial: String = name
                .chars()
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().collect())
                .unwrap_or_else(|| "?".into());
            Border::new()
                .width(tile_size)
                .height(tile_size)
                .corner_radius(CornerRadius::uniform(radius))
                .background(monogram_color(name))
                .content(
                    TextBlock::new()
                        .text(initial)
                        .font_size((tile_size * 0.42).round().max(12.0))
                        .font_weight(FontWeight::SEMI_BOLD)
                        .foreground(Color::rgb(0xFF, 0xFF, 0xFF))
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .vertical_alignment(VerticalAlignment::Center),
                )
        }
    }
}

/// Maps a source filter string to its package manager logo or a Fluent glyph.
fn source_badge_icon(source: &str, size: f64) -> View {
    match source.to_ascii_lowercase().as_str() {
        "winget" => manager_icon(ManagerKind::WinGet, size),
        "msstore" => sized_icon(GLYPH_STORE, size - 2.0),
        "scoop" => manager_icon(ManagerKind::Scoop, size),
        "chocolatey" => manager_icon(ManagerKind::Chocolatey, size),
        "npm" => manager_icon(ManagerKind::Npm, size),
        "pip" => manager_icon(ManagerKind::Pip, size),
        "cargo" => manager_icon(ManagerKind::Cargo, size),
        "dotnet" => manager_icon(ManagerKind::DotnetTool, size),
        "powershell" => manager_icon(ManagerKind::PowerShell, size),
        "powershell7" => manager_icon(ManagerKind::PowerShell7, size),
        "vcpkg" => manager_icon(ManagerKind::Vcpkg, size),
        _ => sized_icon(GLYPH_APPS, size - 2.0),
    }
}

fn power_store_sort_label(t: &powerkit_core::locale::Strings, key: SortKey) -> &'static str {
    match key {
        SortKey::Name => t.power_store_sort_name,
        SortKey::Id => t.power_store_sort_id,
        SortKey::InstalledDate => t.power_store_sort_installed,
        SortKey::UpdatedDate => t.power_store_sort_updated,
    }
}

fn group_divider(label: &str, count: usize, updates: bool) -> View {
    let count_badge: View = if updates {
        Border::new()
            .corner_radius(CornerRadius::uniform(9.0))
            .padding(Thickness::xy(8.0, 1.0))
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::Accent)
            .border_thickness(Thickness::uniform(1.0))
            .vertical_alignment(VerticalAlignment::Center)
            .content(
                TextBlock::new()
                    .text(count.to_string())
                    .font_size(11.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(ThemeBrush::AccentText),
            )
    } else {
        Border::new()
            .corner_radius(CornerRadius::uniform(9.0))
            .padding(Thickness::xy(7.0, 1.0))
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .vertical_alignment(VerticalAlignment::Center)
            .content(
                TextBlock::new()
                    .text(count.to_string())
                    .font_size(11.0)
                    .opacity(0.75),
            )
    };

    let content: View = if updates {
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                accent_bar(14.0),
                sized_icon(GLYPH_UPDATE, 12.0),
                TextBlock::new()
                    .text(label)
                    .font_size(12.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(ThemeBrush::AccentText)
                    .vertical_alignment(VerticalAlignment::Center),
                count_badge,
            ))
    } else {
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                StackPanel::new()
                    .opacity(0.68)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((sized_icon(GLYPH_APPS, 12.0),)),
                TextBlock::new()
                    .text(label)
                    .font_size(12.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .opacity(0.78)
                    .vertical_alignment(VerticalAlignment::Center),
                count_badge,
            ))
    };

    Border::new()
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::new(1.0, 0.0, 1.0, 1.0))
        .padding(Thickness::xy(14.0, 7.0))
        .content(content)
}

fn notice_is_error(notice: &Notice) -> bool {
    matches!(
        notice,
        Notice::ExportFailed(_)
            | Notice::ImportFailed(_)
            | Notice::OptionsRejected
            | Notice::RefreshFailed(_)
    )
}

fn power_store_notice(t: &powerkit_core::locale::Strings, notice: &Notice) -> String {
    match notice {
        Notice::Exported(count, path) => format!("{}: {count} · {path}", t.power_store_exported),
        Notice::ExportFailed(error) => format!("{}: {error}", t.power_store_export_failed),
        Notice::Imported(count) => format!("{}: {count}", t.power_store_imported),
        Notice::ImportFailed(path) => format!("{}: {path}", t.power_store_import_failed),
        Notice::OptionsSaved(id) => format!("{}: {id}", t.power_store_options_saved),
        Notice::OptionsRejected => t.power_store_options_rejected.to_string(),
        Notice::Copied(id) => format!("{}: {id}", t.power_store_copied),
        Notice::RefreshFailed(error) => {
            format!(
                "{}: {}",
                t.power_store_refresh_failed,
                list_error_text(t, error)
            )
        }
    }
}

/// Words a list error, translating the sentinels the session reports.
fn list_error_text<'a>(t: &'a powerkit_core::locale::Strings, message: &'a str) -> &'a str {
    match message {
        WINGET_UNAVAILABLE_MISSING => t.power_store_unavailable_missing,
        WINGET_UNAVAILABLE_TOO_OLD => t.power_store_unavailable_too_old,
        NO_MANAGERS_AVAILABLE => t.power_store_no_managers,
        NO_SEARCH_ANSWER => t.power_store_no_search_answer,
        LIST_WORKER_STOPPED => t.power_store_worker_stopped,
        _ => message,
    }
}

pub(super) fn power_store_operation_name(
    t: &powerkit_core::locale::Strings,
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
    t: &powerkit_core::locale::Strings,
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

fn power_store_stage(t: &powerkit_core::locale::Strings, stage: ProgressStage) -> &'static str {
    match stage {
        ProgressStage::Queued => t.power_store_stage_queued,
        ProgressStage::Downloading => t.power_store_stage_downloading,
        ProgressStage::Installing => t.power_store_stage_installing,
        ProgressStage::Uninstalling => t.power_store_stage_uninstalling,
        ProgressStage::Finishing => t.power_store_stage_finishing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use powerkit_core::power_store::{ManagerKind, PackageKey};

    fn row(id: &str, update: bool) -> PackageRow {
        PackageRow {
            key: PackageKey::new(ManagerKind::WinGet, "winget", id),
            operation: None,
            name: id.into(),
            version: "1.0".into(),
            available: String::new(),
            actionable_update: update,
            installed: true,
            ignored: false,
            skipped: false,
            selected: false,
            configured: false,
            store_managed: false,
            scope: String::new(),
            install_date: String::new(),
            updated_at: String::new(),
            icon: None,
        }
    }

    #[test]
    fn unified_installed_list_groups_updates_above_the_rest() {
        let packages = [row("A", true), row("B", false), row("C", false)];
        let entries = table_entries(&packages, true, false);
        assert_eq!(
            entries,
            [
                TableEntry::Divider {
                    updates: true,
                    count: 1
                },
                TableEntry::Row(0),
                TableEntry::Divider {
                    updates: false,
                    count: 2
                },
                TableEntry::Row(1),
                TableEntry::Row(2),
            ]
        );
    }

    #[test]
    fn filtered_or_update_free_lists_have_no_dividers_and_keep_the_truncation_notice_last() {
        let packages = [row("A", false), row("B", false)];
        assert_eq!(
            table_entries(&packages, true, false),
            [TableEntry::Row(0), TableEntry::Row(1)]
        );
        let updates = [row("A", true)];
        assert_eq!(
            table_entries(&updates, false, true),
            [TableEntry::Row(0), TableEntry::Truncated]
        );
    }

    #[test]
    fn keys_keep_the_row_identity_and_fixed_divider_names() {
        let packages = [row("Alpha.App", true)];
        assert_eq!(entry_key(TableEntry::Row(0), &packages), "winget|Alpha.App");
        let divider = TableEntry::Divider {
            updates: false,
            count: 3,
        };
        assert_eq!(entry_key(divider, &packages), "grp-installed");
        assert_eq!(entry_key(TableEntry::Truncated, &packages), "truncated");
    }

    #[test]
    fn monogram_color_is_case_insensitive_and_deterministic() {
        assert_eq!(monogram_color("PowerToys"), monogram_color("powertoys"));
        assert_eq!(monogram_color("git"), monogram_color("GIT"));
    }

    #[test]
    fn error_notices_are_classified_separately_from_confirmations() {
        assert!(notice_is_error(&Notice::ExportFailed("disk".into())));
        assert!(notice_is_error(&Notice::ImportFailed("bad.json".into())));
        assert!(notice_is_error(&Notice::OptionsRejected));
        assert!(notice_is_error(&Notice::RefreshFailed("timeout".into())));
        assert!(!notice_is_error(&Notice::Copied("Git.Git".into())));
        assert!(!notice_is_error(&Notice::OptionsSaved("Git.Git".into())));
        assert!(!notice_is_error(&Notice::Imported(3)));
        assert!(!notice_is_error(&Notice::Exported(3, "a.json".into())));
    }
}
