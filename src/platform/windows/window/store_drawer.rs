//! Right-side overlay drawers for PowerStore: package inspector (details + options),
//! operation history and PowerStore settings.
use super::super::power_store::session::ImportRow;
use super::super::power_store::{
    Action, DetailsView, DrawerView, InspectorView, ManagerKind, ManagerPresence, ProbeView,
};
use super::super::settings_ui;
use super::manager_icon::manager_icon;
use super::store_page::{button_row, icon_label, power_store_operation_name, sized_icon};
use super::{Message, OptionsField, Shell};
use crate::power_store::{
    ARCHITECTURES, HistoryEntry, HistoryOutcome, INSTALL_SCOPES, InstallOptions, OperationKind,
    PackageDetails, PackageKey, VCPKG_TRIPLETS,
};
use windows_reactor::*;

const DRAWER_WIDTH: f64 = 416.0;
const GLYPH_CLOSE: &str = "\u{E711}";
const GLYPH_COPY: &str = "\u{E8C8}";
const MANAGER_ICON_SIZE: f64 = 32.0;
const GLYPH_BROWSE: &str = "\u{E838}";
const GLYPH_CHEVRON: &str = "\u{E76C}";
const GLYPH_BACK: &str = "\u{E72B}";
const GLYPH_RESET: &str = "\u{E7A7}";

impl Shell {
    pub(super) fn power_store_drawer_overlay(&self, context: &ViewContext<Self>) -> Option<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let (title, subtitle, body) = match &store.drawer {
            DrawerView::Hidden => return None,
            DrawerView::Inspector(inspector) => (
                inspector.name.clone(),
                format!("{} · {}", inspector.key.id, inspector.key.source),
                self.inspector_drawer_body(context, inspector),
            ),
            DrawerView::History => (
                t.power_store_history.to_string(),
                String::new(),
                self.history_drawer_body(context),
            ),
            DrawerView::Settings => (
                t.power_store_settings_title.to_string(),
                String::new(),
                self.settings_drawer_body(context),
            ),
            DrawerView::Manager(kind) => (
                manager_name(*kind).to_string(),
                t.power_store_managers.to_string(),
                self.manager_drawer_body(context, *kind),
            ),
        };

        let scrim = Button::new()
            .grid_column(0)
            .style(ButtonStyle::Subtle)
            .opacity(0.28)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .on_click(context.message(Message::PowerStore(Action::CloseDrawer)))
            .content("");

        let mut header_labels = vec![settings_ui::heading(&title, 18.0).into()];
        if !subtitle.is_empty() {
            header_labels.push(settings_ui::caption(&subtitle));
        }
        let header = Grid::new()
            .grid_row(0)
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .margin(Thickness::new(20.0, 16.0, 14.0, 12.0))
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .keyed_children(
                        header_labels
                            .into_iter()
                            .enumerate()
                            .map(|(index, view)| KeyedView::new(index, view)),
                    ),
                Button::new()
                    .grid_column(1)
                    .width(34.0)
                    .height(32.0)
                    .style(ButtonStyle::Subtle)
                    .vertical_alignment(VerticalAlignment::Top)
                    .on_click(context.message(Message::PowerStore(Action::CloseDrawer)))
                    .content(sized_icon(GLYPH_CLOSE, 12.0))
                    .tooltip(t.power_store_close),
            ));

        let divider = Border::new()
            .grid_row(1)
            .height(1.0)
            .background(ThemeBrush::CardStroke);

        let scroll = ScrollViewer::new()
            .grid_row(2)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .margin(Thickness::new(20.0, 14.0, 20.0, 24.0))
                    .children((settings_ui::stack(body, 10.0),)),
            );

        let panel = Border::new()
            .grid_column(1)
            .width(DRAWER_WIDTH)
            .vertical_alignment(VerticalAlignment::Stretch)
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(1.0, 0.0, 0.0, 0.0))
            .content(
                Grid::new()
                    .rows([GridLength::Auto, GridLength::Auto, GridLength::STAR])
                    .children((header, divider, scroll)),
            );

        Some(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .children((scrim, panel)),
        )
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

        let version_line = if inspector.installed && !inspector.available.is_empty() {
            format!("{} → {}", dash(&inspector.version), inspector.available)
        } else if inspector.installed {
            dash(&inspector.version).to_string()
        } else {
            dash(&inspector.available).to_string()
        };
        let scope_label = match inspector.scope.as_str() {
            "user" => t.power_store_scope_user,
            "machine" => t.power_store_scope_machine,
            _ => t.power_store_default,
        };

        let mut summary_lines = vec![
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(8.0)
                .children((
                    settings_ui::caption(&format!(
                        "{}: {} · {}: {} · {}: {}",
                        t.power_store_version,
                        version_line,
                        t.power_store_source,
                        inspector.key.source,
                        t.power_store_scope,
                        scope_label,
                    )),
                    Button::new()
                        .grid_column(1)
                        .style(ButtonStyle::Subtle)
                        .on_click(message(Action::CopyId(inspector.key.id.clone())))
                        .content(icon_label(GLYPH_COPY, t.power_store_copy_id))
                        .tooltip(t.power_store_copy_id),
                )),
        ];
        if !inspector.install_date.is_empty() || !inspector.updated_at.is_empty() {
            summary_lines.push(settings_ui::caption(&format!(
                "{}: {} · {}: {}",
                t.power_store_install_date,
                dash(&inspector.install_date),
                t.power_store_updated_date,
                dash(&inspector.updated_at),
            )));
        }

        let mut actions = Vec::new();
        if !inspector.installed {
            actions.push(
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(message(Action::Queue(
                        OperationKind::Install,
                        inspector.key.clone(),
                        inspector.name.clone(),
                    )))
                    .content(t.power_store_install),
            );
        } else if !own {
            if inspector.actionable_update {
                actions.push(
                    Button::new()
                        .style(ButtonStyle::Accent)
                        .on_click(message(Action::Queue(
                            OperationKind::Update,
                            inspector.key.clone(),
                            inspector.name.clone(),
                        )))
                        .content(t.power_store_update),
                );
            }
            actions.push(
                Button::new()
                    .on_click(message(Action::Queue(
                        OperationKind::Uninstall,
                        inspector.key.clone(),
                        inspector.name.clone(),
                    )))
                    .content(t.power_store_uninstall),
            );
            if inspector.actionable_update && !inspector.available.is_empty() {
                actions.push(
                    Button::new()
                        .on_click(message(Action::SkipVersion(
                            inspector.key.clone(),
                            inspector.available.clone(),
                        )))
                        .content(t.power_store_skip_version),
                );
            }
            if inspector.ignored {
                actions.push(
                    Button::new()
                        .on_click(message(Action::Unignore(inspector.key.clone())))
                        .content(t.power_store_unignore),
                );
            } else {
                actions.push(
                    Button::new()
                        .on_click(message(Action::Ignore(inspector.key.clone())))
                        .content(t.power_store_ignore),
                );
            }
        }
        if !actions.is_empty() {
            summary_lines.push(button_row(actions));
        }

        let mut rows = vec![settings_ui::surface(settings_ui::stack(summary_lines, 8.0))];

        // Manifest details card
        let detail_parts = match &inspector.details {
            DetailsView::Hidden | DetailsView::Loading(_) => {
                vec![settings_ui::caption(t.power_store_details_loading)]
            }
            DetailsView::Failed(_, error) => vec![
                TextBlock::new()
                    .text(t.power_store_details_failed)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                settings_ui::caption(error),
            ],
            DetailsView::Ready(details) => details_parts(t, details),
        };
        rows.push(settings_ui::section(t.power_store_details));
        rows.push(settings_ui::surface(settings_ui::stack(detail_parts, 6.0)));

        // Per-package install options editor
        if !own {
            rows.push(settings_ui::section(t.power_store_options_title));
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
        let pick = |field: OptionsField, current: &Option<String>, values: &[&'static str]| {
            let mut buttons = vec![
                Button::new()
                    .style(if current.is_none() {
                        ButtonStyle::Accent
                    } else {
                        ButtonStyle::Default
                    })
                    .on_click(context.message(Message::EditPowerStoreOptions(field, String::new())))
                    .content(t.power_store_default),
            ];
            buttons.extend(values.iter().map(|&value| {
                let label = match value {
                    "user" => t.power_store_scope_user,
                    "machine" => t.power_store_scope_machine,
                    other => other,
                };
                Button::new()
                    .style(if current.as_deref() == Some(value) {
                        ButtonStyle::Accent
                    } else {
                        ButtonStyle::Default
                    })
                    .on_click(context.message(Message::EditPowerStoreOptions(field, value.into())))
                    .content(label)
            }));
            button_row(buttons)
        };
        let text = |field: OptionsField, value: &Option<String>| -> View {
            TextBox::new()
                .text(value.clone().unwrap_or_default())
                .on_text_changed(
                    context.callback(move |text| Message::EditPowerStoreOptions(field, text)),
                )
                .into()
        };
        settings_ui::surface(settings_ui::stack(
            vec![
                settings_ui::caption(t.power_store_options_lead),
                TextBlock::new()
                    .text(t.power_store_scope)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                pick(OptionsField::Scope, &draft.scope, &INSTALL_SCOPES),
                TextBlock::new()
                    .text(t.power_store_architecture)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                pick(
                    OptionsField::Architecture,
                    &draft.architecture,
                    &ARCHITECTURES,
                ),
                TextBlock::new()
                    .text(t.power_store_install_version)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                text(OptionsField::Version, &draft.version),
                TextBlock::new()
                    .text(t.power_store_extra_args)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                text(OptionsField::ExtraArgs, &draft.extra_args),
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(context.message(Message::SavePowerStoreOptions))
                    .content(t.power_store_save),
            ],
            8.0,
        ))
    }

    fn history_drawer_body(&self, context: &ViewContext<Self>) -> Vec<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        if store.history.is_empty() {
            return vec![settings_ui::caption(t.power_store_history_empty)];
        }
        let mut rows = vec![settings_ui::card(
            t.power_store_clear_history,
            Button::new()
                .on_click(context.message(Message::PowerStore(Action::ClearHistory)))
                .content(t.power_store_clear_history),
        )];
        rows.extend(store.history.iter().map(|entry| history_row(t, entry)));
        rows
    }

    fn manager_switch(&self, context: &ViewContext<Self>, kind: ManagerKind, on: bool) -> View {
        // No minimum width: the stock 154 px leaves a gap before the `>` button.
        settings_ui::toggle_switch(ToggleSwitch::new().is_on(on).min_width(0.0).on_toggled(
            context.callback(move |on| Message::PowerStore(Action::SetManagerEnabled(kind, on))),
        ))
    }

    /// One manager's detail drawer: switch, program path (CLI managers) and default
    /// install scope (WinGet). Reached from the chevron in the settings drawer.
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
        let mut rows: Vec<View> = vec![
            Button::new()
                .style(ButtonStyle::Subtle)
                .horizontal_alignment(HorizontalAlignment::Left)
                .on_click(message(Action::ShowSettingsDrawer))
                .content(icon_label(GLYPH_BACK, t.power_store_settings_title)),
            manager_card(
                t,
                kind,
                presence,
                self.manager_switch(context, kind, settings.enabled),
            ),
        ];
        let (found, version) = match store.manager_probes.get(&kind) {
            None | Some(ProbeView::Checking) => (t.power_store_manager_checking.to_string(), None),
            Some(ProbeView::Done(probe)) => (
                probe
                    .path
                    .clone()
                    .unwrap_or_else(|| t.power_store_manager_not_found.to_string()),
                probe.version.clone(),
            ),
        };
        rows.push(settings_ui::described_card(
            t.power_store_manager_detected,
            settings_ui::caption(&found),
            TextBlock::new().text(
                version
                    .map(|version| format!("{} {version}", t.power_store_manager_version))
                    .unwrap_or_default(),
            ),
        ));
        if presence != ManagerPresence::Available
            && let Some(hint) = kind.install_hint()
        {
            rows.push(settings_ui::described_card(
                t.power_store_manager_install_hint,
                settings_ui::caption(hint),
                Button::new()
                    .on_click(message(Action::CopyId(hint.to_string())))
                    .content(icon_label(GLYPH_COPY, t.power_store_manager_copy_hint)),
            ));
        }
        if !settings.enabled {
            rows.push(settings_ui::caption(t.power_store_managers_lead));
            return rows;
        }
        if kind.supports_executable() {
            let location = settings
                .executable
                .as_deref()
                .unwrap_or(t.power_store_manager_auto_path);
            let mut buttons = vec![
                Button::new()
                    .width(36.0)
                    .height(32.0)
                    .style(ButtonStyle::Subtle)
                    .on_click(message(Action::PickManagerExecutable(kind)))
                    .content(sized_icon(GLYPH_BROWSE, 14.0))
                    .tooltip(t.power_store_manager_browse),
            ];
            if settings.executable.is_some() {
                buttons.push(
                    Button::new()
                        .width(36.0)
                        .height(32.0)
                        .style(ButtonStyle::Subtle)
                        .on_click(message(Action::ResetManagerExecutable(kind)))
                        .content(sized_icon(GLYPH_RESET, 14.0))
                        .tooltip(t.power_store_manager_reset),
                );
            }
            rows.push(settings_ui::described_card(
                t.power_store_manager_program,
                settings_ui::caption(location),
                button_row(buttons),
            ));
        }
        if kind.supports_default_scope() {
            rows.push(manager_choice_card(
                context,
                kind,
                t.power_store_manager_scope,
                t.power_store_manager_scope_auto,
                &[
                    (INSTALL_SCOPES[0], t.power_store_scope_user),
                    (INSTALL_SCOPES[1], t.power_store_scope_machine),
                ],
                settings.scope.as_deref(),
                Action::SetManagerScope,
            ));
        }
        if kind.supports_default_architecture() {
            let choices: Vec<(&'static str, &'static str)> =
                ARCHITECTURES.iter().map(|value| (*value, *value)).collect();
            rows.push(manager_choice_card(
                context,
                kind,
                t.power_store_manager_architecture,
                t.power_store_manager_scope_auto,
                &choices,
                settings.architecture.as_deref(),
                Action::SetManagerArchitecture,
            ));
            rows.push(settings_ui::caption(
                t.power_store_manager_architecture_note,
            ));
        }
        if kind.supports_default_triplet() {
            let choices: Vec<(&'static str, &'static str)> = VCPKG_TRIPLETS
                .iter()
                .map(|value| (*value, *value))
                .collect();
            rows.push(manager_choice_card(
                context,
                kind,
                t.power_store_manager_triplet,
                t.power_store_manager_scope_auto,
                &choices,
                settings.triplet.as_deref(),
                Action::SetManagerTriplet,
            ));
        }
        rows
    }

    fn settings_drawer_body(&self, context: &ViewContext<Self>) -> Vec<View> {
        let store = &self.snapshot.power_store;
        let t = self.t();
        let message = |action: Action| context.message(Message::PowerStore(action));
        let mut rows = vec![
            settings_ui::section(t.power_store_managers),
            settings_ui::caption(t.power_store_managers_lead),
        ];
        for manager in &store.managers {
            let kind = manager.kind;
            let settings = store
                .manager_settings
                .get(&kind)
                .cloned()
                .unwrap_or_default();
            let details = Button::new()
                .width(32.0)
                .height(32.0)
                .style(ButtonStyle::Subtle)
                .on_click(message(Action::OpenManagerDrawer(kind)))
                .content(sized_icon(GLYPH_CHEVRON, 12.0))
                .tooltip(t.power_store_manager_details);
            rows.push(manager_card(
                t,
                kind,
                manager.presence,
                button_row(vec![
                    self.manager_switch(context, kind, settings.enabled),
                    details,
                ]),
            ));
        }
        rows.extend([
            settings_ui::section(t.power_store_updates_section),
            settings_ui::stepper_card(
                t.power_store_check_interval,
                t.duration(u64::from(store.check_minutes)),
                Button::new()
                    .width(40.0)
                    .on_click(message(Action::StepCheckMinutes(false)))
                    .content(settings_ui::STEPPER_DOWN),
                Button::new()
                    .width(40.0)
                    .on_click(message(Action::StepCheckMinutes(true)))
                    .content(settings_ui::STEPPER_UP),
            ),
            settings_ui::card(
                t.power_store_notify,
                settings_ui::toggle_switch(
                    ToggleSwitch::new().is_on(store.notify).on_toggled(
                        context.callback(|on| Message::PowerStore(Action::SetNotify(on))),
                    ),
                ),
            ),
            settings_ui::section(t.power_store_display_section),
            settings_ui::described_card(
                t.power_store_remote_icons,
                settings_ui::caption(t.power_store_remote_icons_lead),
                settings_ui::toggle_switch(
                    ToggleSwitch::new().is_on(store.remote_icons).on_toggled(
                        context.callback(|on| Message::PowerStore(Action::SetRemoteIcons(on))),
                    ),
                ),
            ),
            settings_ui::section(t.power_store_backup_section),
            settings_ui::described_card(
                &format!("{} / {}", t.power_store_export, t.power_store_import),
                settings_ui::caption(t.power_store_transfer_lead),
                button_row(vec![
                    Button::new()
                        .on_click(message(Action::Export))
                        .content(t.power_store_export),
                    Button::new()
                        .on_click(message(Action::Import))
                        .content(t.power_store_import),
                ]),
            ),
        ]);
        if !store.import.is_empty() {
            rows.push(settings_ui::section(&format!(
                "{}: {}",
                t.power_store_imported,
                store.import.len()
            )));
            rows.extend(store.import.iter().map(|row| import_row(context, row)));
            rows.push(button_row(vec![
                Button::new()
                    .style(ButtonStyle::Accent)
                    .is_enabled(store.import.iter().any(|row| row.selected))
                    .on_click(message(Action::InstallImport))
                    .content(t.power_store_install_selected),
                Button::new()
                    .on_click(message(Action::ClearImport))
                    .content(t.power_store_clear_list),
            ]));
        }
        rows.push(settings_ui::section(t.power_store_ignored));
        if store.ignored.is_empty() {
            rows.push(settings_ui::caption(t.power_store_none));
        }
        rows.extend(store.ignored.iter().map(|key| {
            settings_ui::card(
                &package_label(key),
                Button::new()
                    .on_click(message(Action::Unignore(key.clone())))
                    .content(t.power_store_remove),
            )
        }));
        rows.push(settings_ui::section(t.power_store_skipped));
        if store.skipped.is_empty() {
            rows.push(settings_ui::caption(t.power_store_none));
        }
        rows.extend(store.skipped.iter().map(|(key, version)| {
            settings_ui::card(
                &format!("{} · {version}", package_label(key)),
                Button::new()
                    .on_click(message(Action::Unskip(key.clone())))
                    .content(t.power_store_remove),
            )
        }));
        rows
    }
}

/// Settings card with the manager's icon, name and install badge on the left.
fn manager_card(
    t: &crate::locale::Strings,
    kind: ManagerKind,
    presence: ManagerPresence,
    control: View,
) -> View {
    let labels = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(12.0)
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            manager_icon(kind, MANAGER_ICON_SIZE),
            settings_ui::stack(
                vec![
                    TextBlock::new().text(manager_name(kind)).into(),
                    install_badge(t, presence),
                ],
                4.0,
            ),
        ));
    settings_ui::row(labels, control)
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
fn install_badge(t: &crate::locale::Strings, presence: ManagerPresence) -> View {
    let (text, (r, g, b)) = match presence {
        ManagerPresence::Available => (t.power_store_manager_installed, (0x1E, 0x7B, 0x3C)),
        ManagerPresence::Missing => (t.power_store_manager_missing, (0x6E, 0x6E, 0x6E)),
        ManagerPresence::TooOld => (t.power_store_manager_too_old, (0x9A, 0x67, 0x00)),
    };
    Border::new()
        .corner_radius(CornerRadius::uniform(10.0))
        .padding(Thickness::xy(8.0, 1.0))
        .background(Color::rgb(r, g, b))
        .horizontal_alignment(HorizontalAlignment::Left)
        .content(
            TextBlock::new()
                .text(text)
                .font_size(11.0)
                .foreground(Color::rgb(0xFF, 0xFF, 0xFF)),
        )
}

fn package_label(key: &PackageKey) -> String {
    format!("{} ({})", key.id, key.source)
}

/// A row with a drop-down whose first entry is "automatic" (no stored value) followed by
/// `choices` as `(stored value, label)`; picking an entry sends `action(kind, value)`.
fn manager_choice_card(
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
    settings_ui::card(
        title,
        ComboBox::new()
            .min_width(180.0)
            .items_source(labels)
            .selected_index(selected)
            .on_selection_changed(context.callback(move |index: Option<usize>| match index {
                Some(chosen) if chosen != selected => {
                    let value = chosen.checked_sub(1).map(|index| values[index].to_string());
                    Message::PowerStore(action(kind, value))
                }
                _ => Message::Noop,
            })),
    )
}

fn import_row(context: &ViewContext<Shell>, row: &ImportRow) -> View {
    let key = row.package.key.clone();
    let mut label = package_label(&key);
    if !row.package.options.is_empty() {
        label.push_str(" · ");
        label.push_str(&options_summary(&row.package.options));
    }
    settings_ui::surface(
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(12.0)
            .children((
                CheckBox::new()
                    .grid_column(0)
                    .is_checked(row.selected)
                    .on_is_checked_changed(context.callback(move |selected| {
                        Message::PowerStore(Action::SetImport(key.clone(), selected))
                    })),
                TextBlock::new()
                    .grid_column(1)
                    .text(label)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_wrapping(TextWrapping::Wrap),
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

fn details_parts(t: &crate::locale::Strings, details: &PackageDetails) -> Vec<View> {
    let mut parts = Vec::new();
    if !details.description.is_empty() {
        parts.push(
            TextBlock::new()
                .text(details.description.as_str())
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        );
    }
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
        parts.push(detail_line(label, text, url));
    }
    if parts.is_empty() {
        parts.push(settings_ui::caption(t.power_store_none));
    }
    parts
}

/// Manifest URLs open in the browser only when they are plain http(s) links.
fn detail_line(label: &str, text: &str, url: &str) -> View {
    let caption = format!("{label}: {text}");
    let web = url.starts_with("https://") || url.starts_with("http://");
    match HyperlinkButton::new().navigate_uri(url) {
        Ok(link) if web => link.content(caption),
        _ => settings_ui::caption(&caption),
    }
}

fn history_row(t: &crate::locale::Strings, entry: &HistoryEntry) -> View {
    let outcome = match entry.outcome {
        HistoryOutcome::Succeeded => t.power_store_succeeded,
        HistoryOutcome::Cancelled => t.power_store_cancelled,
        HistoryOutcome::Failed => t.power_store_operation_failed,
    };
    let mut parts = vec![
        TextBlock::new()
            .text(format!(
                "{}: {}",
                power_store_operation_name(t, entry.kind),
                entry.name
            ))
            .font_weight(FontWeight::SEMI_BOLD)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
        settings_ui::caption(&format!(
            "{} · {outcome} · {}",
            entry.time,
            package_label(&entry.key)
        )),
    ];
    if !entry.log.is_empty() {
        parts.push(settings_ui::caption(&entry.log));
    }
    settings_ui::surface(settings_ui::stack(parts, 4.0))
}
