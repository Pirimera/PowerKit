//! Right-side overlay drawers for PowerStore: package inspector (details + options),
//! operation history, PowerStore settings and per-manager settings.
use super::super::settings_ui;
use super::manager_icon::manager_icon;
use super::store_page::{package_logo_tile, power_store_operation_name};
use super::view_helpers::{
    DRAWER_ICON_BOX, DrawerChrome, PillTone, accent_bar, compact_empty_row, drawer_icon_badge,
    drawer_section, drawer_shell, empty_card, equal_button_row, fact_row, grouped_item,
    grouped_setting_row, grouped_surface, hairline, icon_button, icon_label, inset_box,
    inset_notice, sized_icon, status_pill,
};
use super::{Message, OptionsField, Shell};
use powerkit_app::controller::power_store::ImportRow;
use powerkit_app::controller::power_store::{
    Action, DetailsView, DrawerView, InspectorView, ManagerPresence, ManagerStatus, ProbeView,
};
use powerkit_core::power_store::ManagerKind;
use powerkit_core::power_store::{
    ARCHITECTURES, HistoryEntry, HistoryOutcome, INSTALL_SCOPES, InstallOptions, OperationKind,
    PackageDetails, PackageKey, VCPKG_TRIPLETS,
};
use windows_reactor::*;

const MANAGER_TILE_LIST: f64 = 34.0;
const MANAGER_TILE_HERO: f64 = 42.0;

const GLYPH_CLOSE: &str = "\u{E711}";
const GLYPH_COPY: &str = "\u{E8C8}";
const GLYPH_BROWSE: &str = "\u{E838}";
const GLYPH_CHEVRON: &str = "\u{E76C}";
const GLYPH_BACK: &str = "\u{E72B}";
const GLYPH_RESET: &str = "\u{E7A7}";
const GLYPH_HISTORY: &str = "\u{E81C}";
const GLYPH_SETTINGS: &str = "\u{E713}";
const GLYPH_DELETE: &str = "\u{E74D}";
const GLYPH_ERROR: &str = "\u{EA39}";
const GLYPH_CHECK: &str = "\u{E73E}";
const GLYPH_UPDATE: &str = "\u{E777}";
const GLYPH_DOWNLOAD: &str = "\u{E896}";
const GLYPH_UPLOAD: &str = "\u{E898}";
const GLYPH_INFO: &str = "\u{E946}";
const GLYPH_APPS: &str = "\u{E71D}";
const GLYPH_SYNC: &str = "\u{E72C}";
const GLYPH_DISPLAY: &str = "\u{E790}";
const GLYPH_SAVE: &str = "\u{E74E}";
const GLYPH_PACKAGE: &str = "\u{E7B8}";
const GLYPH_SKIP: &str = "\u{E893}";

impl Shell {
    pub(super) fn power_store_drawer_overlay(&self, context: &ViewContext<Self>) -> Option<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();

        let (leading, title, title_badge, subtitle, sticky_bar, body): (
            Option<View>,
            String,
            Option<String>,
            String,
            Option<View>,
            Vec<View>,
        ) = match &store.drawer {
            DrawerView::Hidden => return None,
            DrawerView::Inspector(inspector) => {
                let icon = store
                    .packages
                    .iter()
                    .find(|pkg| pkg.key == inspector.key)
                    .and_then(|pkg| pkg.icon.as_ref());
                (
                    Some(package_logo_tile(&inspector.name, icon, DRAWER_ICON_BOX)),
                    inspector.name.clone(),
                    None,
                    format!("{} · {}", inspector.key.id, inspector.key.source),
                    None,
                    self.inspector_drawer_body(context, inspector),
                )
            }
            DrawerView::History => {
                let count_badge =
                    (!store.history.is_empty()).then(|| store.history.len().to_string());
                let sticky = self.history_sticky_bar(context);
                (
                    Some(drawer_icon_badge(GLYPH_HISTORY)),
                    t.power_store_history.to_string(),
                    count_badge,
                    String::new(),
                    sticky,
                    self.history_drawer_body(),
                )
            }
            DrawerView::Settings => {
                let installed = installed_manager_count(&store.managers);
                let subtitle = if store.managers.is_empty() {
                    String::new()
                } else {
                    format!(
                        "{installed}/{} {}",
                        store.managers.len(),
                        t.power_store_manager_installed
                    )
                };
                (
                    Some(drawer_icon_badge(GLYPH_SETTINGS)),
                    t.power_store_settings_title.to_string(),
                    None,
                    subtitle,
                    None,
                    self.settings_drawer_body(context),
                )
            }
            DrawerView::Manager(kind) => (
                Some(icon_button(
                    GLYPH_BACK,
                    14.0,
                    34.0,
                    context.message(Message::PowerStore(Action::ShowSettingsDrawer)),
                    t.power_store_settings_title,
                )),
                t.power_store_manager_details.to_string(),
                None,
                t.power_store_settings_title.to_string(),
                None,
                self.manager_drawer_body(context, *kind),
            ),
        };

        Some(drawer_shell(
            DrawerChrome {
                leading,
                title,
                title_badge,
                subtitle,
                sticky_bar,
                body,
                close_tooltip: t.power_store_close,
            },
            || context.message(Message::PowerStore(Action::CloseDrawer)),
        ))
    }

    fn inspector_drawer_body(
        &self,
        context: &ViewContext<Self>,
        inspector: &InspectorView,
    ) -> Vec<View> {
        let t = self.t();
        let own = inspector.key.is_own_package();
        let message = |action: Action| context.message(Message::PowerStore(action));
        let dash = |value: &str| -> String {
            if value.is_empty() {
                "—".into()
            } else {
                value.to_string()
            }
        };

        let mut summary_lines: Vec<View> = Vec::new();

        // Compact status pills at the top of the summary card when relevant
        let mut status_pills: Vec<View> = Vec::new();
        if inspector.ignored {
            status_pills.push(status_pill(
                Some(GLYPH_CLOSE),
                t.power_store_badge_ignored,
                PillTone::Muted,
            ));
        }
        if let Some(version) = &inspector.skipped_version {
            status_pills.push(status_pill(
                Some(GLYPH_SKIP),
                &format!("{} ({version})", t.power_store_badge_skipped),
                PillTone::Warning,
            ));
        }
        if !inspector.options.is_empty() {
            status_pills.push(status_pill(
                Some(GLYPH_SETTINGS),
                t.power_store_configured,
                PillTone::Neutral,
            ));
        }
        if !status_pills.is_empty() {
            summary_lines.push(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .keyed_children(
                        status_pills
                            .into_iter()
                            .enumerate()
                            .map(|(index, pill)| KeyedView::new(index, pill)),
                    ),
            );
        }

        let version_value: View = if inspector.installed && !inspector.available.is_empty() {
            let available = TextBlock::new()
                .text(format!("→ {}", inspector.available))
                .font_size(13.0)
                .font_weight(if inspector.actionable_update {
                    FontWeight::SEMI_BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_wrapping(TextWrapping::Wrap);
            let available: View = if inspector.actionable_update {
                available.foreground(ThemeBrush::AccentText).into()
            } else {
                available.opacity(0.8).into()
            };
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(dash(&inspector.version))
                        .font_size(13.0)
                        .opacity(0.68),
                    available,
                ))
        } else {
            let shown = if inspector.installed {
                dash(&inspector.version)
            } else {
                dash(&inspector.available)
            };
            TextBlock::new()
                .text(shown)
                .font_size(13.0)
                .text_wrapping(TextWrapping::Wrap)
                .into()
        };

        let scope_label = match inspector.scope.as_str() {
            "user" => t.power_store_scope_user,
            "machine" => t.power_store_scope_machine,
            _ => t.power_store_default,
        };

        let source_value: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(6.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                manager_icon(inspector.key.manager, 16.0),
                TextBlock::new()
                    .text(inspector.key.source.as_str())
                    .font_size(13.0)
                    .vertical_alignment(VerticalAlignment::Center),
            ));

        let mut facts = vec![
            fact_row(t.power_store_version, version_value),
            fact_row(t.power_store_source, source_value),
            fact_row(
                t.power_store_scope,
                TextBlock::new().text(scope_label).font_size(13.0).into(),
            ),
        ];
        if !inspector.install_date.is_empty() {
            facts.push(fact_row(
                t.power_store_install_date,
                TextBlock::new()
                    .text(inspector.install_date.as_str())
                    .font_size(13.0)
                    .into(),
            ));
        }
        if !inspector.updated_at.is_empty() {
            facts.push(fact_row(
                t.power_store_updated_date,
                TextBlock::new()
                    .text(inspector.updated_at.as_str())
                    .font_size(13.0)
                    .into(),
            ));
        }
        summary_lines.push(settings_ui::stack(facts, 8.0));

        // Inset package ID bar with copy action
        let id_bar = Border::new()
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(6.0))
            .padding(Thickness::new(10.0, 5.0, 6.0, 5.0))
            .content(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto])
                    .column_spacing(8.0)
                    .children((
                        TextBlock::new()
                            .grid_column(0)
                            .text(inspector.key.id.as_str())
                            .font_size(12.0)
                            .opacity(0.84)
                            .is_text_selection_enabled(true)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .vertical_alignment(VerticalAlignment::Center),
                        Button::new()
                            .grid_column(1)
                            .style(ButtonStyle::Subtle)
                            .on_click(message(Action::CopyId(inspector.key.id.clone())))
                            .content(icon_label(GLYPH_COPY, t.power_store_manager_copy_hint))
                            .tooltip(t.power_store_copy_id),
                    )),
            );
        summary_lines.push(id_bar);

        // Balanced primary and secondary action rows
        let mut primary_actions: Vec<View> = Vec::new();
        let mut secondary_actions: Vec<View> = Vec::new();
        if !inspector.installed {
            primary_actions.push(
                Button::new()
                    .style(ButtonStyle::Accent)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .on_click(message(Action::Queue(
                        OperationKind::Install,
                        inspector.key.clone(),
                        inspector.name.clone(),
                    )))
                    .content(icon_label(GLYPH_DOWNLOAD, t.power_store_install)),
            );
        } else if !own {
            if inspector.actionable_update {
                primary_actions.push(
                    Button::new()
                        .style(ButtonStyle::Accent)
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .on_click(message(Action::Queue(
                            OperationKind::Update,
                            inspector.key.clone(),
                            inspector.name.clone(),
                        )))
                        .content(icon_label(GLYPH_UPDATE, t.power_store_update)),
                );
            }
            primary_actions.push(
                Button::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .on_click(message(Action::Queue(
                        OperationKind::Uninstall,
                        inspector.key.clone(),
                        inspector.name.clone(),
                    )))
                    .content(icon_label(GLYPH_DELETE, t.power_store_uninstall)),
            );
            if inspector.actionable_update && !inspector.available.is_empty() {
                secondary_actions.push(
                    Button::new()
                        .style(ButtonStyle::Subtle)
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .on_click(message(Action::SkipVersion(
                            inspector.key.clone(),
                            inspector.available.clone(),
                        )))
                        .content(icon_label(GLYPH_SKIP, t.power_store_skip_version)),
                );
            }
            if inspector.ignored {
                secondary_actions.push(
                    Button::new()
                        .style(ButtonStyle::Subtle)
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .on_click(message(Action::Unignore(inspector.key.clone())))
                        .content(icon_label(GLYPH_CHECK, t.power_store_unignore)),
                );
            } else {
                secondary_actions.push(
                    Button::new()
                        .style(ButtonStyle::Subtle)
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .on_click(message(Action::Ignore(inspector.key.clone())))
                        .content(icon_label(GLYPH_CLOSE, t.power_store_ignore)),
                );
            }
        }
        if !primary_actions.is_empty() {
            summary_lines.push(hairline());
            summary_lines.push(equal_button_row(primary_actions));
        }
        if !secondary_actions.is_empty() {
            summary_lines.push(equal_button_row(secondary_actions));
        }

        let mut rows = vec![settings_ui::surface(settings_ui::stack(
            summary_lines,
            10.0,
        ))];

        // Manifest details card
        let detail_card: View = match &inspector.details {
            DetailsView::Hidden | DetailsView::Loading(_) => settings_ui::surface(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        ProgressRing::new()
                            .width(16.0)
                            .height(16.0)
                            .is_active(true)
                            .is_indeterminate(true),
                        TextBlock::new()
                            .text(t.power_store_details_loading)
                            .font_size(12.5)
                            .opacity(0.76),
                    )),
            ),
            DetailsView::Failed(_, error) => Border::new()
                .background(ThemeBrush::SystemCriticalBackground)
                .border_brush(ThemeBrush::SystemCritical)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::xy(14.0, 12.0))
                .content(settings_ui::stack(
                    vec![
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(8.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                sized_icon(GLYPH_ERROR, 14.0),
                                TextBlock::new()
                                    .text(t.power_store_details_failed)
                                    .font_size(13.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .foreground(ThemeBrush::SystemCritical),
                            )),
                        settings_ui::caption(error),
                    ],
                    6.0,
                )),
            DetailsView::Ready(details) => {
                settings_ui::surface(settings_ui::stack(details_parts(t, details), 10.0))
            }
        };
        rows.push(drawer_section(
            GLYPH_INFO,
            t.power_store_details,
            None,
            false,
        ));
        rows.push(detail_card);

        // Per-package install options editor
        // A manager whose adapters ignore every option gets no options section at all.
        if !own && inspector.key.manager.supports_install_options() {
            rows.push(drawer_section(
                GLYPH_SETTINGS,
                t.power_store_options_title,
                None,
                false,
            ));
            rows.push(self.inspector_options_card(context, inspector));
        }
        rows
    }

    fn inspector_options_card(
        &self,
        context: &ViewContext<Self>,
        inspector: &InspectorView,
    ) -> View {
        let t = self.t();
        let draft = self
            .power_store_options_draft
            .clone()
            .unwrap_or_else(|| inspector.options.clone());
        let dirty = draft != inspector.options;

        let text = |field: OptionsField, value: &Option<String>, placeholder: &str| -> View {
            TextBox::new()
                .text(value.clone().unwrap_or_default())
                .placeholder_text(placeholder)
                .on_text_changed(
                    context.callback(move |text| Message::EditPowerStoreOptions(field, text)),
                )
                .into()
        };
        let manager = inspector.key.manager;
        let field_label = |label: &'static str| -> View {
            TextBlock::new()
                .text(label)
                .font_size(12.5)
                .font_weight(FontWeight::SEMI_BOLD)
                .opacity(0.86)
                .into()
        };

        // Only the fields this manager's adapter honors: the others would be ignored.
        let mut rows: Vec<View> = vec![
            TextBlock::new()
                .text(t.power_store_options_lead)
                .font_size(12.0)
                .opacity(0.70)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            hairline(),
        ];

        if manager.supports_default_scope() {
            let scope_choices = [
                (None, t.power_store_default),
                (Some(INSTALL_SCOPES[0]), t.power_store_scope_user),
                (Some(INSTALL_SCOPES[1]), t.power_store_scope_machine),
            ];
            rows.push(settings_ui::stack(
                vec![
                    field_label(t.power_store_scope),
                    segmented_picker(&scope_choices, draft.scope.as_deref(), false, |value| {
                        context.message(Message::EditPowerStoreOptions(
                            OptionsField::Scope,
                            value.unwrap_or_default().into(),
                        ))
                    }),
                ],
                6.0,
            ));
        }
        if manager.supports_default_architecture() {
            let mut arch_choices: Vec<(Option<&'static str>, &'static str)> =
                vec![(None, t.power_store_default)];
            arch_choices.extend(ARCHITECTURES.iter().map(|&value| (Some(value), value)));
            rows.push(settings_ui::stack(
                vec![
                    field_label(t.power_store_architecture),
                    segmented_picker(
                        &arch_choices,
                        draft.architecture.as_deref(),
                        true,
                        |value| {
                            context.message(Message::EditPowerStoreOptions(
                                OptionsField::Architecture,
                                value.unwrap_or_default().into(),
                            ))
                        },
                    ),
                ],
                6.0,
            ));
        }
        if manager.supports_version() {
            rows.push(settings_ui::stack(
                vec![
                    field_label(t.power_store_install_version),
                    text(OptionsField::Version, &draft.version, t.power_store_default),
                ],
                6.0,
            ));
        }
        if manager.supports_extra_args() {
            rows.push(settings_ui::stack(
                vec![
                    field_label(t.power_store_extra_args),
                    text(OptionsField::ExtraArgs, &draft.extra_args, ""),
                ],
                6.0,
            ));
        }

        rows.push(hairline());
        rows.push(
            Button::new()
                .style(if dirty {
                    ButtonStyle::Accent
                } else {
                    ButtonStyle::Default
                })
                .is_enabled(dirty)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .on_click(context.message(Message::SavePowerStoreOptions))
                .content(icon_label(GLYPH_SAVE, t.power_store_save)),
        );
        settings_ui::surface(settings_ui::stack(rows, 11.0))
    }

    fn history_sticky_bar(&self, context: &ViewContext<Self>) -> Option<View> {
        let store = &self.snapshot.power_store;
        if store.history.is_empty() {
            return None;
        }
        let t = self.t();
        let (succeeded, failed, cancelled) = history_counts(&store.history);
        let mut pills: Vec<View> = Vec::new();
        if succeeded > 0 {
            pills.push(status_pill(
                Some(GLYPH_CHECK),
                &succeeded.to_string(),
                PillTone::Success,
            ));
        }
        if failed > 0 {
            pills.push(status_pill(
                Some(GLYPH_ERROR),
                &failed.to_string(),
                PillTone::Critical,
            ));
        }
        if cancelled > 0 {
            pills.push(status_pill(
                Some(GLYPH_CLOSE),
                &cancelled.to_string(),
                PillTone::Muted,
            ));
        }
        let summary = StackPanel::new()
            .grid_column(0)
            .orientation(Orientation::Horizontal)
            .spacing(6.0)
            .vertical_alignment(VerticalAlignment::Center)
            .keyed_children(
                pills
                    .into_iter()
                    .enumerate()
                    .map(|(index, pill)| KeyedView::new(index, pill)),
            );

        Some(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(10.0)
                .children((
                    summary,
                    Button::new()
                        .grid_column(1)
                        .style(ButtonStyle::Subtle)
                        .on_click(context.message(Message::PowerStore(Action::ClearHistory)))
                        .content(icon_label(GLYPH_DELETE, t.power_store_clear_history)),
                )),
        )
    }

    fn history_drawer_body(&self) -> Vec<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        if store.history.is_empty() {
            return vec![empty_card(GLYPH_HISTORY, t.power_store_history_empty)];
        }
        store
            .history
            .iter()
            .map(|entry| history_row(t, entry))
            .collect()
    }

    fn manager_switch(&self, context: &ViewContext<Self>, kind: ManagerKind, on: bool) -> View {
        // No minimum width: the stock 154 px leaves a gap before the `>` button.
        settings_ui::toggle_switch(ToggleSwitch::new().is_on(on).min_width(0.0).on_toggled(
            context.callback(move |on| Message::PowerStore(Action::SetManagerEnabled(kind, on))),
        ))
    }

    /// One manager's detail drawer: identity hero card with switch and detected binary,
    /// install command hint when missing, and per-manager defaults. Reached from the
    /// chevron in the settings drawer.
    fn manager_drawer_body(&self, context: &ViewContext<Self>, kind: ManagerKind) -> Vec<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let message = |action: Action| context.message(Message::PowerStore(action));
        let settings = store
            .manager_settings
            .get(&kind)
            .cloned()
            .unwrap_or_default();
        let presence = store
            .managers
            .iter()
            .find(|manager| manager.kind == kind)
            .map_or(ManagerPresence::Missing, |manager| manager.presence);
        let available = presence == ManagerPresence::Available;

        let (found, version, checking) = match store.manager_probes.get(&kind) {
            None | Some(ProbeView::Checking) => {
                (t.power_store_manager_checking.to_string(), None, true)
            }
            Some(ProbeView::Done(probe)) => (
                probe
                    .path
                    .clone()
                    .unwrap_or_else(|| t.power_store_manager_not_found.to_string()),
                probe.version.clone(),
                false,
            ),
        };

        // Top hero card: framed icon, manager name + install badge under it, switch on the right,
        // plus integrated detected program path and version readout below an inner divider.
        let hero_header = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((manager_icon_tile(kind, MANAGER_TILE_HERO, !available),)),
                StackPanel::new()
                    .grid_column(1)
                    .spacing(4.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        TextBlock::new()
                            .text(manager_name(kind))
                            .font_size(16.0)
                            .font_weight(FontWeight::SEMI_BOLD),
                        install_badge(t, presence),
                    )),
                StackPanel::new()
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((self.manager_switch(context, kind, settings.enabled),)),
            ));

        let detected_status: View = if checking {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    ProgressRing::new()
                        .width(14.0)
                        .height(14.0)
                        .is_active(true)
                        .is_indeterminate(true),
                    TextBlock::new()
                        .text(t.power_store_manager_checking)
                        .font_size(11.5)
                        .opacity(0.72),
                ))
        } else if let Some(version) = version {
            status_pill(
                None,
                &format!("{} {version}", t.power_store_manager_version),
                PillTone::Neutral,
            )
        } else {
            StackPanel::new().into()
        };

        let detected_header = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                TextBlock::new()
                    .grid_column(0)
                    .text(t.power_store_manager_detected)
                    .font_size(12.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .opacity(0.72)
                    .vertical_alignment(VerticalAlignment::Center),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((detected_status,)),
            ));

        let detected_path = inset_box(
            TextBlock::new()
                .text(found)
                .font_size(12.0)
                .opacity(if available && !checking { 0.86 } else { 0.65 })
                .is_text_selection_enabled(true)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        );

        let mut rows: Vec<View> = vec![settings_ui::surface(settings_ui::stack(
            vec![
                hero_header,
                hairline(),
                settings_ui::stack(vec![detected_header, detected_path], 6.0),
            ],
            12.0,
        ))];

        if !available && let Some(hint) = kind.install_hint() {
            let hint_header = Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(8.0)
                .children((
                    accent_bar(16.0, ThemeBrush::Accent.into()).grid_column(0),
                    TextBlock::new()
                        .grid_column(1)
                        .text(t.power_store_manager_install_hint)
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .vertical_alignment(VerticalAlignment::Center),
                    Button::new()
                        .grid_column(2)
                        .style(ButtonStyle::Subtle)
                        .on_click(message(Action::CopyId(hint.to_string())))
                        .content(icon_label(GLYPH_COPY, t.power_store_manager_copy_hint)),
                ));
            let hint_box = inset_box(
                TextBlock::new()
                    .text(hint)
                    .font_size(12.0)
                    .opacity(0.88)
                    .is_text_selection_enabled(true)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            );
            rows.push(settings_ui::surface(settings_ui::stack(
                vec![hint_header, hint_box],
                8.0,
            )));
        }

        if !settings.enabled {
            rows.push(inset_notice(GLYPH_INFO, t.power_store_managers_lead));
            return rows;
        }

        let mut config_rows: Vec<View> = Vec::new();
        if kind.supports_executable() {
            let custom = settings.executable.is_some();
            let location = settings
                .executable
                .as_deref()
                .unwrap_or(t.power_store_manager_auto_path);
            let mut buttons: Vec<View> = vec![icon_button(
                GLYPH_BROWSE,
                14.0,
                32.0,
                message(Action::PickManagerExecutable(kind)),
                t.power_store_manager_browse,
            )];
            if custom {
                buttons.push(icon_button(
                    GLYPH_RESET,
                    14.0,
                    32.0,
                    message(Action::ResetManagerExecutable(kind)),
                    t.power_store_manager_reset,
                ));
            }
            let program_header = Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(8.0)
                .children((
                    TextBlock::new()
                        .grid_column(0)
                        .text(t.power_store_manager_program)
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .vertical_alignment(VerticalAlignment::Center),
                    StackPanel::new()
                        .grid_column(1)
                        .orientation(Orientation::Horizontal)
                        .spacing(4.0)
                        .keyed_children(
                            buttons
                                .into_iter()
                                .enumerate()
                                .map(|(index, btn)| KeyedView::new(index, btn)),
                        ),
                ));
            let path_box = inset_box(
                TextBlock::new()
                    .text(location)
                    .font_size(12.0)
                    .opacity(if custom { 0.88 } else { 0.62 })
                    .is_text_selection_enabled(true)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            );
            config_rows.push(grouped_item(settings_ui::stack(
                vec![program_header, path_box],
                8.0,
            )));
        }

        if kind.supports_default_scope() {
            let choices = [
                (None, t.power_store_manager_scope_auto),
                (Some(INSTALL_SCOPES[0]), t.power_store_scope_user),
                (Some(INSTALL_SCOPES[1]), t.power_store_scope_machine),
            ];
            config_rows.push(grouped_item(settings_ui::stack(
                vec![
                    TextBlock::new()
                        .text(t.power_store_manager_scope)
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                    segmented_picker(&choices, settings.scope.as_deref(), false, |value| {
                        message(Action::SetManagerScope(kind, value.map(str::to_string)))
                    }),
                ],
                8.0,
            )));
        }

        if kind.supports_default_architecture() {
            let mut choices: Vec<(Option<&'static str>, &'static str)> =
                vec![(None, t.power_store_manager_scope_auto)];
            choices.extend(ARCHITECTURES.iter().map(|&value| (Some(value), value)));
            config_rows.push(grouped_item(settings_ui::stack(
                vec![
                    TextBlock::new()
                        .text(t.power_store_manager_architecture)
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                    segmented_picker(&choices, settings.architecture.as_deref(), true, |value| {
                        message(Action::SetManagerArchitecture(
                            kind,
                            value.map(str::to_string),
                        ))
                    }),
                    TextBlock::new()
                        .text(t.power_store_manager_architecture_note)
                        .font_size(11.5)
                        .opacity(0.68)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                ],
                8.0,
            )));
        }

        if kind.supports_default_triplet() {
            let choices: Vec<(&'static str, &'static str)> = VCPKG_TRIPLETS
                .iter()
                .map(|value| (*value, *value))
                .collect();
            config_rows.push(grouped_item(manager_choice_block(
                context,
                kind,
                t.power_store_manager_triplet,
                t.power_store_manager_scope_auto,
                &choices,
                settings.triplet.as_deref(),
                Action::SetManagerTriplet,
            )));
        }

        if !config_rows.is_empty() {
            rows.push(drawer_section(
                GLYPH_SETTINGS,
                t.power_store_options,
                None,
                false,
            ));
            rows.push(grouped_surface(config_rows));
        }
        rows
    }

    fn settings_drawer_body(&self, context: &ViewContext<Self>) -> Vec<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let message = |action: Action| context.message(Message::PowerStore(action));

        let mut rows: Vec<View> = vec![
            drawer_section(
                GLYPH_APPS,
                t.power_store_managers,
                Some(store.managers.len()),
                true,
            ),
            TextBlock::new()
                .text(t.power_store_managers_lead)
                .font_size(12.0)
                .opacity(0.70)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        ];

        // Grouped Windows 11 Settings-style list for all package managers
        let manager_rows: Vec<View> = store
            .managers
            .iter()
            .map(|manager| {
                let kind = manager.kind;
                let settings = store
                    .manager_settings
                    .get(&kind)
                    .cloned()
                    .unwrap_or_default();
                let available = manager.presence == ManagerPresence::Available;
                let details = icon_button(
                    GLYPH_CHEVRON,
                    12.0,
                    30.0,
                    message(Action::OpenManagerDrawer(kind)),
                    t.power_store_manager_details,
                );
                let controls = StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        self.manager_switch(context, kind, settings.enabled),
                        details,
                    ));
                Border::new().padding(Thickness::xy(14.0, 10.0)).content(
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                        .column_spacing(12.0)
                        .children((
                            StackPanel::new()
                                .grid_column(0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children(
                                    (manager_icon_tile(kind, MANAGER_TILE_LIST, !available),),
                                ),
                            StackPanel::new()
                                .grid_column(1)
                                .spacing(3.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((
                                    TextBlock::new()
                                        .text(manager_name(kind))
                                        .font_size(13.5)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .opacity(if available { 1.0 } else { 0.72 }),
                                    install_badge(t, manager.presence),
                                )),
                            StackPanel::new()
                                .grid_column(2)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((controls,)),
                        )),
                )
            })
            .collect();
        if !manager_rows.is_empty() {
            rows.push(grouped_surface(manager_rows));
        }

        // Update checks group (interval stepper + notifications toggle)
        let stepper_control = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(6.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                Button::new()
                    .width(30.0)
                    .height(30.0)
                    .on_click(message(Action::StepCheckMinutes(false)))
                    .content(settings_ui::STEPPER_DOWN),
                Border::new()
                    .min_width(68.0)
                    .background(ThemeBrush::SolidBackground)
                    .border_brush(ThemeBrush::CardStroke)
                    .border_thickness(Thickness::uniform(1.0))
                    .corner_radius(CornerRadius::uniform(6.0))
                    .padding(Thickness::xy(10.0, 4.0))
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(
                        TextBlock::new()
                            .text(t.duration(u64::from(store.check_minutes)))
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .horizontal_alignment(HorizontalAlignment::Center)
                            .vertical_alignment(VerticalAlignment::Center),
                    ),
                Button::new()
                    .width(30.0)
                    .height(30.0)
                    .on_click(message(Action::StepCheckMinutes(true)))
                    .content(settings_ui::STEPPER_UP),
            ));

        let notify_toggle = settings_ui::toggle_switch(
            ToggleSwitch::new()
                .is_on(store.notify)
                .min_width(0.0)
                .on_toggled(context.callback(|on| Message::PowerStore(Action::SetNotify(on)))),
        );

        rows.push(drawer_section(
            GLYPH_SYNC,
            t.power_store_updates_section,
            None,
            false,
        ));
        rows.push(grouped_surface(vec![
            grouped_setting_row(t.power_store_check_interval, None, stepper_control),
            grouped_setting_row(t.power_store_notify, None, notify_toggle),
        ]));

        // Display group
        let remote_icons_toggle = settings_ui::toggle_switch(
            ToggleSwitch::new()
                .is_on(store.remote_icons)
                .min_width(0.0)
                .on_toggled(context.callback(|on| Message::PowerStore(Action::SetRemoteIcons(on)))),
        );
        rows.push(drawer_section(
            GLYPH_DISPLAY,
            t.power_store_display_section,
            None,
            false,
        ));
        rows.push(grouped_surface(vec![grouped_setting_row(
            t.power_store_remote_icons,
            Some(t.power_store_remote_icons_lead),
            remote_icons_toggle,
        )]));

        // Backup / Transfer group
        rows.push(drawer_section(
            GLYPH_PACKAGE,
            t.power_store_backup_section,
            None,
            false,
        ));
        rows.push(settings_ui::surface(settings_ui::stack(
            vec![
                TextBlock::new()
                    .text(format!(
                        "{} / {}",
                        t.power_store_export, t.power_store_import
                    ))
                    .font_size(13.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                TextBlock::new()
                    .text(t.power_store_transfer_lead)
                    .font_size(12.0)
                    .opacity(0.72)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
                equal_button_row(vec![
                    Button::new()
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .on_click(message(Action::Export))
                        .content(icon_label(GLYPH_UPLOAD, t.power_store_export)),
                    Button::new()
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .on_click(message(Action::Import))
                        .content(icon_label(GLYPH_DOWNLOAD, t.power_store_import)),
                ]),
            ],
            10.0,
        )));

        if !store.import.is_empty() {
            rows.push(drawer_section(
                GLYPH_DOWNLOAD,
                t.power_store_imported,
                Some(store.import.len()),
                false,
            ));
            let import_items: Vec<View> = store
                .import
                .iter()
                .map(|row| import_row(context, row))
                .collect();
            rows.push(grouped_surface(import_items));
            rows.push(equal_button_row(vec![
                Button::new()
                    .style(ButtonStyle::Accent)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .is_enabled(store.import.iter().any(|row| row.selected))
                    .on_click(message(Action::InstallImport))
                    .content(icon_label(GLYPH_DOWNLOAD, t.power_store_install_selected)),
                Button::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .on_click(message(Action::ClearImport))
                    .content(icon_label(GLYPH_CLOSE, t.power_store_clear_list)),
            ]));
        }

        // Ignored packages
        rows.push(drawer_section(
            GLYPH_CLOSE,
            t.power_store_ignored,
            (!store.ignored.is_empty()).then_some(store.ignored.len()),
            false,
        ));
        if store.ignored.is_empty() {
            rows.push(compact_empty_row(t.power_store_none));
        } else {
            let ignored_rows: Vec<View> = store
                .ignored
                .iter()
                .map(|key| {
                    pinned_package_row(
                        key,
                        None,
                        t.power_store_remove,
                        message(Action::Unignore(key.clone())),
                    )
                })
                .collect();
            rows.push(grouped_surface(ignored_rows));
        }

        // Skipped versions
        rows.push(drawer_section(
            GLYPH_SKIP,
            t.power_store_skipped,
            (!store.skipped.is_empty()).then_some(store.skipped.len()),
            false,
        ));
        if store.skipped.is_empty() {
            rows.push(compact_empty_row(t.power_store_none));
        } else {
            let skipped_rows: Vec<View> = store
                .skipped
                .iter()
                .map(|(key, version)| {
                    pinned_package_row(
                        key,
                        Some(version.as_str()),
                        t.power_store_remove,
                        message(Action::Unskip(key.clone())),
                    )
                })
                .collect();
            rows.push(grouped_surface(skipped_rows));
        }
        rows
    }
}

fn manager_icon_tile(kind: ManagerKind, tile_size: f64, dimmed: bool) -> View {
    let inner = (tile_size - 12.0).max(16.0);
    let icon = manager_icon(kind, inner);
    let content: View = if dimmed {
        StackPanel::new()
            .opacity(0.55)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .children((icon,))
    } else {
        icon
    };
    Border::new()
        .width(tile_size)
        .height(tile_size)
        .corner_radius(CornerRadius::uniform(8.0))
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .content(content)
}

/// Segmented pill control matching the main PowerStore toolbar. When `auto_first` is true
/// (e.g. a 5-option architecture row), the first ("Default"/"Auto") label takes its natural
/// width while the remaining short tokens share equal columns so nothing clips.
fn segmented_picker(
    choices: &[(Option<&'static str>, &'static str)],
    current: Option<&str>,
    auto_first: bool,
    on_pick: impl Fn(Option<&'static str>) -> Callback<()>,
) -> View {
    let columns: Vec<GridLength> = choices
        .iter()
        .enumerate()
        .map(|(index, _)| {
            if auto_first && index == 0 {
                GridLength::Auto
            } else {
                GridLength::STAR
            }
        })
        .collect();
    let buttons: Vec<KeyedView> = choices
        .iter()
        .enumerate()
        .map(|(index, &(value, label))| {
            let active = current == value;
            let col = i32::try_from(index).unwrap_or(0);
            let btn = Button::new()
                .grid_column(col)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .style(if active {
                    ButtonStyle::Accent
                } else {
                    ButtonStyle::Subtle
                })
                .on_click(on_pick(value))
                .content(
                    TextBlock::new()
                        .text(label)
                        .font_size(12.0)
                        .font_weight(if active {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .horizontal_alignment(HorizontalAlignment::Center),
                );
            KeyedView::new(index, btn)
        })
        .collect();
    Border::new()
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .padding(Thickness::uniform(3.0))
        .content(
            Grid::new()
                .columns(columns)
                .column_spacing(2.0)
                .keyed_children(buttons),
        )
}

fn manager_name(kind: ManagerKind) -> &'static str {
    match kind {
        ManagerKind::WinGet => "WinGet",
        ManagerKind::Scoop => "Scoop",
        ManagerKind::Chocolatey => "Chocolatey",
        ManagerKind::Npm => "npm",
        ManagerKind::Pip => "pip",
        ManagerKind::Cargo => "Cargo",
        ManagerKind::DotnetTool => ".NET Tool",
        ManagerKind::PowerShell => "Windows PowerShell",
        ManagerKind::PowerShell7 => "PowerShell 7",
        ManagerKind::Vcpkg => "vcpkg",
    }
}

/// Small pill that says whether the manager is installed on this PC. It does not show
/// the on/off state; the switch beside it does.
fn install_badge(t: &powerkit_core::locale::Strings, presence: ManagerPresence) -> View {
    match presence {
        ManagerPresence::Available => status_pill(
            Some(GLYPH_CHECK),
            t.power_store_manager_installed,
            PillTone::Success,
        ),
        ManagerPresence::Missing => {
            status_pill(None, t.power_store_manager_missing, PillTone::Muted)
        }
        ManagerPresence::TooOld => {
            status_pill(None, t.power_store_manager_too_old, PillTone::Warning)
        }
    }
}

fn package_label(key: &PackageKey) -> String {
    format!("{} ({})", key.id, key.source)
}

/// Label above a full-width drop-down whose first entry is "automatic" (no stored value)
/// followed by `choices` as `(stored value, label)`.
fn manager_choice_block(
    context: &ViewContext<Shell>,
    kind: ManagerKind,
    title: &str,
    auto_label: &'static str,
    choices: &[(&'static str, &'static str)],
    current: Option<&str>,
    action: fn(ManagerKind, Option<String>) -> Action,
) -> View {
    let selected = current
        .and_then(|current| choices.iter().position(|(value, _)| *value == current))
        .map_or(0, |index| index + 1);
    let values: Vec<&'static str> = choices.iter().map(|(value, _)| *value).collect();
    let labels: Vec<&'static str> = std::iter::once(auto_label)
        .chain(choices.iter().map(|(_, label)| *label))
        .collect();
    settings_ui::stack(
        vec![
            TextBlock::new()
                .text(title)
                .font_size(13.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            ComboBox::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .items_source(labels)
                .selected_index(selected)
                .on_selection_changed(context.callback(move |index: Option<usize>| match index {
                    Some(chosen) if chosen != selected => {
                        let value = chosen.checked_sub(1).map(|index| values[index].to_string());
                        Message::PowerStore(action(kind, value))
                    }
                    _ => Message::Noop,
                }))
                .into(),
        ],
        8.0,
    )
}

fn pinned_package_row(
    key: &PackageKey,
    skipped_version: Option<&str>,
    remove_label: &str,
    on_remove: Callback<()>,
) -> View {
    let mut sub_parts: Vec<View> = vec![
        TextBlock::new()
            .text(key.source.as_str())
            .font_size(11.5)
            .opacity(0.65)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
    ];
    if let Some(version) = skipped_version {
        sub_parts.push(status_pill(None, version, PillTone::Warning));
    }
    let meta = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(6.0)
        .vertical_alignment(VerticalAlignment::Center)
        .keyed_children(
            sub_parts
                .into_iter()
                .enumerate()
                .map(|(index, view)| KeyedView::new(index, view)),
        );
    Border::new().padding(Thickness::xy(14.0, 10.0)).content(
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(10.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((manager_icon(key.manager, 18.0),)),
                StackPanel::new()
                    .grid_column(1)
                    .spacing(3.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        TextBlock::new()
                            .text(key.id.as_str())
                            .font_size(13.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .max_lines(1),
                        meta,
                    )),
                Button::new()
                    .grid_column(2)
                    .style(ButtonStyle::Subtle)
                    .vertical_alignment(VerticalAlignment::Center)
                    .on_click(on_remove)
                    .content(remove_label),
            )),
    )
}

fn import_row(context: &ViewContext<Shell>, row: &ImportRow) -> View {
    let key = row.package.key.clone();
    let opts = options_summary(&row.package.options);
    let mut label_lines: Vec<View> = vec![
        TextBlock::new()
            .text(key.id.as_str())
            .font_size(13.0)
            .font_weight(FontWeight::SEMI_BOLD)
            .text_trimming(TextTrimming::CharacterEllipsis)
            .max_lines(1)
            .into(),
    ];
    let sub = if opts.is_empty() {
        key.source.clone()
    } else {
        format!("{} · {opts}", key.source)
    };
    label_lines.push(
        TextBlock::new()
            .text(sub)
            .font_size(11.5)
            .opacity(0.68)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
    );
    Border::new().padding(Thickness::xy(14.0, 9.0)).content(
        Grid::new()
            .columns([GridLength::Auto, GridLength::Auto, GridLength::STAR])
            .column_spacing(10.0)
            .children((
                CheckBox::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .is_checked(row.selected)
                    .on_is_checked_changed(context.callback(move |selected| {
                        Message::PowerStore(Action::SetImport(key.clone(), selected))
                    })),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((manager_icon(row.package.key.manager, 16.0),)),
                StackPanel::new()
                    .grid_column(2)
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .keyed_children(
                        label_lines
                            .into_iter()
                            .enumerate()
                            .map(|(index, line)| KeyedView::new(index, line)),
                    ),
            )),
    )
}

fn options_summary(options: &InstallOptions) -> String {
    [
        options.scope.as_deref(),
        options.architecture.as_deref(),
        options.version.as_deref(),
        options.extra_args.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

fn details_parts(t: &powerkit_core::locale::Strings, details: &PackageDetails) -> Vec<View> {
    let mut parts = Vec::new();
    if !details.description.is_empty() {
        parts.push(
            TextBlock::new()
                .text(details.description.as_str())
                .font_size(13.0)
                .opacity(0.88)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        );
    }
    let mut meta_items = Vec::new();
    for (label, value, url) in [
        (
            t.power_store_publisher,
            &details.publisher,
            &details.publisher_url,
        ),
        (t.power_store_author, &details.author, &String::new()),
        (
            t.power_store_license,
            &details.license,
            &details.license_url,
        ),
        (t.power_store_homepage, &details.homepage, &details.homepage),
    ] {
        if value.is_empty() && url.is_empty() {
            continue;
        }
        let text = if value.is_empty() { url } else { value };
        meta_items.push(detail_field(label, text, url));
    }
    if !meta_items.is_empty() {
        if !parts.is_empty() {
            parts.push(hairline());
        }
        parts.push(settings_ui::stack(meta_items, 8.0));
    }
    if parts.is_empty() {
        parts.push(settings_ui::caption(t.power_store_none));
    }
    parts
}

/// Manifest URLs open in the browser only when they are plain http(s) links.
fn detail_field(label: &str, text: &str, url: &str) -> View {
    let web = url.starts_with("https://") || url.starts_with("http://");
    let value: View = match HyperlinkButton::new().navigate_uri(url) {
        Ok(link) if web => link.content(text),
        _ => TextBlock::new()
            .text(text)
            .font_size(13.0)
            .is_text_selection_enabled(true)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
    };
    StackPanel::new().spacing(2.0).children((
        TextBlock::new()
            .text(label)
            .font_size(11.5)
            .font_weight(FontWeight::SEMI_BOLD)
            .opacity(0.62),
        value,
    ))
}

fn operation_glyph(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Install => GLYPH_DOWNLOAD,
        OperationKind::Update => GLYPH_UPDATE,
        OperationKind::Uninstall => GLYPH_DELETE,
    }
}

fn history_row(t: &powerkit_core::locale::Strings, entry: &HistoryEntry) -> View {
    let (outcome_badge, stripe_brush): (View, Brush) = match entry.outcome {
        HistoryOutcome::Succeeded => (
            status_pill(
                Some(GLYPH_CHECK),
                t.power_store_succeeded,
                PillTone::Success,
            ),
            Color::rgb(0x2D, 0x8A, 0x4E).into(),
        ),
        HistoryOutcome::Cancelled => (
            status_pill(Some(GLYPH_CLOSE), t.power_store_cancelled, PillTone::Muted),
            Color::rgb(0x70, 0x74, 0x7A).into(),
        ),
        HistoryOutcome::Failed => (
            status_pill(
                Some(GLYPH_ERROR),
                t.power_store_operation_failed,
                PillTone::Critical,
            ),
            ThemeBrush::SystemCritical.into(),
        ),
    };

    let op_badge = Border::new()
        .width(28.0)
        .height(28.0)
        .corner_radius(CornerRadius::uniform(6.0))
        .background(ThemeBrush::SolidBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .vertical_alignment(VerticalAlignment::Center)
        .content(sized_icon(operation_glyph(entry.kind), 13.0));

    let header = Grid::new()
        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
        .column_spacing(10.0)
        .children((
            StackPanel::new()
                .grid_column(0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((op_badge,)),
            StackPanel::new()
                .grid_column(1)
                .spacing(1.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(entry.name.as_str())
                        .font_size(13.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(power_store_operation_name(t, entry.kind))
                        .font_size(11.5)
                        .opacity(0.65),
                )),
            StackPanel::new()
                .grid_column(2)
                .vertical_alignment(VerticalAlignment::Top)
                .children((outcome_badge,)),
        ));

    // Grid keeps the package label bounded in a STAR column so long IDs ellipsize cleanly
    // rather than overflowing past the right edge of the card.
    let meta = Grid::new()
        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
        .column_spacing(6.0)
        .children((
            StackPanel::new()
                .grid_column(0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((manager_icon(entry.key.manager, 14.0),)),
            TextBlock::new()
                .grid_column(1)
                .text(package_label(&entry.key))
                .font_size(11.5)
                .opacity(0.68)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .max_lines(1)
                .vertical_alignment(VerticalAlignment::Center),
            TextBlock::new()
                .grid_column(2)
                .text(entry.time.as_str())
                .font_size(11.0)
                .opacity(0.55)
                .vertical_alignment(VerticalAlignment::Center),
        ));

    let mut parts = vec![header, meta];
    if !entry.log.is_empty() {
        let log_border: Brush = if entry.outcome == HistoryOutcome::Failed {
            ThemeBrush::SystemCritical.into()
        } else {
            ThemeBrush::CardStroke.into()
        };
        parts.push(
            Border::new()
                .background(ThemeBrush::SolidBackground)
                .border_brush(log_border)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(6.0))
                .padding(Thickness::xy(10.0, 8.0))
                .margin(Thickness::new(0.0, 2.0, 0.0, 0.0))
                .content(
                    TextBlock::new()
                        .text(entry.log.as_str())
                        .font_size(11.5)
                        .opacity(0.84)
                        .is_text_selection_enabled(true)
                        .text_wrapping(TextWrapping::Wrap),
                ),
        );
    }

    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .content(
            Grid::new().children((
                Border::new()
                    .width(3.0)
                    .corner_radius(CornerRadius::uniform(2.0))
                    .background(stripe_brush)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .vertical_alignment(VerticalAlignment::Stretch)
                    .margin(Thickness::new(4.0, 8.0, 0.0, 8.0)),
                Border::new()
                    .padding(Thickness::new(15.0, 11.0, 14.0, 11.0))
                    .content(settings_ui::stack(parts, 7.0)),
            )),
        )
}

fn history_counts(entries: &[HistoryEntry]) -> (usize, usize, usize) {
    entries
        .iter()
        .fold((0, 0, 0), |(ok, fail, cancel), entry| match entry.outcome {
            HistoryOutcome::Succeeded => (ok + 1, fail, cancel),
            HistoryOutcome::Failed => (ok, fail + 1, cancel),
            HistoryOutcome::Cancelled => (ok, fail, cancel + 1),
        })
}

fn installed_manager_count(managers: &[ManagerStatus]) -> usize {
    managers
        .iter()
        .filter(|manager| manager.presence == ManagerPresence::Available)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(outcome: HistoryOutcome) -> HistoryEntry {
        HistoryEntry {
            time: "2026-11-20 13:40".into(),
            kind: OperationKind::Update,
            key: PackageKey::new(ManagerKind::WinGet, "winget", "Git.Git"),
            name: "Git".into(),
            outcome,
            log: String::new(),
        }
    }

    #[test]
    fn history_counts_separates_outcomes() {
        let entries = [
            entry(HistoryOutcome::Succeeded),
            entry(HistoryOutcome::Failed),
            entry(HistoryOutcome::Succeeded),
            entry(HistoryOutcome::Cancelled),
        ];
        assert_eq!(history_counts(&entries), (2, 1, 1));
    }

    #[test]
    fn installed_manager_count_counts_only_available() {
        let managers = [
            ManagerStatus {
                kind: ManagerKind::WinGet,
                presence: ManagerPresence::Available,
            },
            ManagerStatus {
                kind: ManagerKind::Chocolatey,
                presence: ManagerPresence::Missing,
            },
            ManagerStatus {
                kind: ManagerKind::Scoop,
                presence: ManagerPresence::Available,
            },
            ManagerStatus {
                kind: ManagerKind::Vcpkg,
                presence: ManagerPresence::TooOld,
            },
        ];
        assert_eq!(installed_manager_count(&managers), 2);
    }

    #[test]
    fn options_summary_joins_only_present_fields() {
        let opts = InstallOptions {
            scope: Some("user".into()),
            architecture: None,
            version: Some("2.0".into()),
            extra_args: Some("--silent".into()),
        };
        assert_eq!(options_summary(&opts), "user · 2.0 · --silent");
        assert_eq!(options_summary(&InstallOptions::default()), "");
    }
}
