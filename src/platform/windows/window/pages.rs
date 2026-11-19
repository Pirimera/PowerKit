//! Per-tool settings pages composed from shared `settings_ui` surfaces.
use super::super::settings_ui;
use super::{
    AppCommand, CaptureTarget, ColorTarget, LANG_ORDER, MODE_ORDER, Message, Page,
    PowerMouseAction, RELEASE_NOTES_URI, RESIZE_ENCODER_ORDER, RESIZE_PNG_ORDER, RULER_MODE_ORDER,
    RULER_UNIT_ORDER, Shell, THEME_ORDER, installed_about_facts, page_copy,
};
use crate::always_on::MAX_INTERVAL_HOURS;
use crate::image_resizer::FallbackEncoder;
use crate::power_mouse::{Action, ActionSlot, Binding, PAGE_TRIGGERS, TILT_TRIGGERS, Trigger};
use std::time::Duration;
use windows_reactor::*;

const LABEL_ABOUT_NAME: &str = "PowerKit";
const GLYPH_INFO: &str = "\u{E946}";
const GLYPH_HISTORY: &str = "\u{E81C}";
const HERO_FADE: Duration = Duration::from_millis(900);

impl Shell {
    pub(super) fn page_view(&self, context: &ViewContext<Self>) -> View {
        if self.page == Page::General {
            return settings_ui::stack(self.general(context), 12.0);
        }
        let (title, lead) = page_copy(self.page, self.snapshot.ui_language);
        let mut rows = vec![
            settings_ui::heading(title, 28.0).into(),
            TextBlock::new()
                .text(lead)
                .text_wrapping(TextWrapping::Wrap)
                .opacity(0.8)
                .margin(Thickness::new(0.0, 0.0, 0.0, 12.0))
                .into(),
        ];
        rows.extend(match self.page {
            Page::AlwaysOn => self.always_on(context),
            Page::AlwaysTop => self.always_top(context),
            Page::ScreenRuler => self.screen_ruler(context),
            Page::ImageResizer => self.image_resizer(context),
            Page::PowerMouse => self.power_mouse(context),
            Page::PowerStore | Page::MediaRenamer => Vec::new(),
            Page::General => Vec::new(),
            Page::Settings => self.settings(context),
        });
        settings_ui::stack(rows, 8.0)
    }

    fn always_on(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let custom = format!(
            "{} ({})",
            t.mode_custom,
            t.duration(u64::from(s.interval_minutes))
        );
        let number = |value: u32, maximum: u32| {
            NumberBox::new()
                .minimum(0.0)
                .maximum(f64::from(maximum))
                .value(f64::from(value))
                .min_width(88.0)
        };
        let interval = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .children((
                number(s.interval_minutes / 60, MAX_INTERVAL_HOURS)
                    .on_value_changed(context.callback(Message::IntervalHours)),
                TextBlock::new()
                    .text(t.unit_hours)
                    .vertical_alignment(VerticalAlignment::Center),
                number(s.interval_minutes % 60, 59)
                    .on_value_changed(context.callback(Message::IntervalMinutes)),
                TextBlock::new()
                    .text(t.unit_minutes)
                    .vertical_alignment(VerticalAlignment::Center),
            ));
        let expiry = settings_ui::stack(
            vec![
                DatePicker::new()
                    .on_selected_date_changed(context.callback(Message::ExpiryDate))
                    .into(),
                TimePicker::new()
                    .on_selected_time_changed(context.callback(Message::ExpiryTime))
                    .into(),
                Button::new()
                    .style(ButtonStyle::Accent)
                    .is_enabled(self.expiry_time.is_some())
                    .on_click(context.message(Message::StartExpiry))
                    .content(t.expiry_start),
                settings_ui::caption(t.expiry_hint),
            ],
            8.0,
        );
        let bind_status = match &s.bound_app {
            Some(app) => t.process_status.replace("{app}", app),
            None => t.bind_hint.into(),
        };
        let battery_floor = match s.battery_floor {
            0 => t.off.to_string(),
            floor => format!("%{floor}"),
        };
        vec![
            settings_ui::status_card(&s.status, t.always_on_lead),
            settings_ui::section(t.mode),
            self.choices(
                context,
                &MODE_ORDER,
                [
                    t.mode_off,
                    t.mode_half,
                    t.mode_hour,
                    t.mode_two,
                    &custom,
                    t.mode_forever,
                ],
            ),
            settings_ui::described_card(
                t.interval_title,
                settings_ui::caption(t.interval_hint),
                interval,
            ),
            settings_ui::section(t.expiry_title),
            settings_ui::surface(expiry),
            settings_ui::section(t.bind_title),
            settings_ui::surface(settings_ui::stack(
                vec![
                    settings_ui::caption(&bind_status),
                    Button::new()
                        .on_click(
                            context.message(Message::Command(AppCommand::AlwaysOnBindProcess)),
                        )
                        .content(t.bind_action),
                ],
                8.0,
            )),
            self.toggle_card(context, AppCommand::ToggleDisplay, t.display),
            settings_ui::section(t.section_controls),
            self.hotkey_card(
                context,
                AppCommand::CaptureAlwaysOnHotkey,
                t.hotkey_always_on,
                &s.always_on_hotkey,
            ),
            self.toggle_described(
                context,
                AppCommand::ToggleAlwaysOnWarn,
                t.warn_toggle,
                t.warn_hint,
            ),
            settings_ui::section(t.power_title),
            self.toggle_described(context, AppCommand::ToggleAcOnly, t.ac_only, t.ac_only_hint),
            self.stepper_card(
                context,
                t.battery_floor,
                battery_floor,
                AppCommand::BatteryFloorDown,
                AppCommand::BatteryFloorUp,
            ),
            settings_ui::caption(t.battery_floor_hint),
        ]
    }

    fn always_top(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let launch_status = format!("{}\n{}", s.top_status, t.always_top_launch_hint);
        vec![
            settings_ui::described_card(
                t.pin,
                settings_ui::caption(&launch_status),
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(context.message(Message::Command(AppCommand::TogglePin)))
                    .content(t.pin_action),
            ),
            settings_ui::section(t.section_shortcuts),
            self.hotkey_card(
                context,
                AppCommand::CapturePinHotkey,
                t.hotkey_pin,
                &s.pin_hotkey,
            ),
            self.hotkey_card(
                context,
                AppCommand::CaptureOpacityUpHotkey,
                t.hotkey_op_up,
                &s.opacity_up_hotkey,
            ),
            self.hotkey_card(
                context,
                AppCommand::CaptureOpacityDownHotkey,
                t.hotkey_op_down,
                &s.opacity_down_hotkey,
            ),
            settings_ui::section(t.section_border),
            self.toggle_card(context, AppCommand::ToggleFrameAccent, t.frame_accent),
            settings_ui::card(
                t.frame_color,
                self.color_button(context, ColorTarget::Frame, s.frame_color),
            ),
            self.stepper_card(
                context,
                t.frame_opacity,
                t.percent(s.frame_opacity),
                AppCommand::FrameOpacityDown,
                AppCommand::FrameOpacityUp,
            ),
            self.stepper_card(
                context,
                t.frame_thickness,
                format!("{} px", s.frame_thickness),
                AppCommand::FrameThicknessDown,
                AppCommand::FrameThicknessUp,
            ),
            settings_ui::section(t.section_behavior),
            self.toggle_card(context, AppCommand::ToggleSound, t.sound),
        ]
    }

    fn screen_ruler(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let measure = Button::new()
            .style(ButtonStyle::Accent)
            .on_click(context.message(Message::Command(AppCommand::ToggleMeasure)))
            .content(if s.ruler_active {
                t.ruler_stop
            } else {
                t.ruler_start
            });
        let status = if s.ruler_active {
            t.ruler_on
        } else {
            t.ruler_off
        };
        let launch_status = format!("{status}\n{}", t.ruler_launch_hint);
        vec![
            settings_ui::described_card(
                t.ruler_title,
                settings_ui::caption(&launch_status),
                measure,
            ),
            settings_ui::section(t.section_shortcuts),
            self.hotkey_card(
                context,
                AppCommand::CaptureRulerHotkey,
                t.hotkey_ruler,
                &s.ruler_hotkey,
            ),
            settings_ui::section(t.section_measurement),
            settings_ui::wide_card(
                t.ruler_mode,
                self.choices(
                    context,
                    &RULER_MODE_ORDER,
                    [
                        t.ruler_none,
                        t.ruler_bounds,
                        t.ruler_spacing,
                        t.ruler_horizontal,
                        t.ruler_vertical,
                    ],
                ),
            ),
            settings_ui::wide_card(
                t.ruler_unit,
                self.choices(
                    context,
                    &RULER_UNIT_ORDER,
                    [t.ruler_px, t.ruler_in, t.ruler_cm, t.ruler_mm, t.ruler_dip],
                ),
            ),
            self.toggle_card(
                context,
                AppCommand::ToggleRulerContinuous,
                t.ruler_continuous,
            ),
            self.toggle_card(context, AppCommand::ToggleRulerChannel, t.ruler_channel),
            self.toggle_card(context, AppCommand::ToggleRulerFeet, t.ruler_feet),
            self.stepper_card(
                context,
                t.ruler_tolerance,
                s.ruler_tolerance.to_string(),
                AppCommand::RulerToleranceDown,
                AppCommand::RulerToleranceUp,
            ),
            settings_ui::caption(t.ruler_lead),
            settings_ui::section(t.section_appearance),
            settings_ui::card(
                t.ruler_color,
                self.color_button(context, ColorTarget::Ruler, s.ruler_color),
            ),
        ]
    }

    fn image_resizer(&self, context: &ViewContext<Self>) -> Vec<View> {
        let resize = &self.snapshot.resize;
        let t = self.t();
        let filename = self
            .filename_draft
            .clone()
            .unwrap_or_else(|| resize.filename.clone());
        let filename_row = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(filename)
                    .on_text_changed(context.callback(Message::EditFilename)),
                Button::new()
                    .grid_column(1)
                    .is_enabled(self.filename_draft.is_some())
                    .on_click(context.message(Message::ApplyFilename))
                    .content(t.resize_file_apply),
            ));
        vec![
            settings_ui::described_card(
                t.resize_launch,
                settings_ui::caption(t.resize_launch_hint),
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .on_click(context.message(Message::Command(AppCommand::ResizeOpen)))
                    .content(t.resize_open),
            ),
            settings_ui::section(t.resize_options),
            settings_ui::caption(t.resize_options_hint),
            self.toggle_card(context, AppCommand::ToggleResizeShrink, t.resize_shrink),
            self.toggle_card(context, AppCommand::ToggleResizeReplace, t.resize_replace),
            self.toggle_card(
                context,
                AppCommand::ToggleResizeOrientation,
                t.resize_orientation,
            ),
            settings_ui::section(t.resize_section_file),
            settings_ui::wide_card(
                t.resize_file,
                settings_ui::stack(
                    vec![filename_row, settings_ui::caption(t.resize_file_hint)],
                    6.0,
                ),
            ),
            self.toggle_card(
                context,
                AppCommand::ToggleResizeKeepDate,
                t.resize_keep_date,
            ),
            self.toggle_card(context, AppCommand::ToggleResizeMetadata, t.resize_metadata),
            settings_ui::section(t.resize_section_encoding),
            self.stepper_card(
                context,
                t.resize_jpeg,
                resize.jpeg_quality.to_string(),
                AppCommand::ResizeJpegDown,
                AppCommand::ResizeJpegUp,
            ),
            self.stepper_card(
                context,
                t.resize_tiff,
                t.resize_tiff_label(resize.tiff).to_string(),
                AppCommand::ResizeTiffDown,
                AppCommand::ResizeTiffUp,
            ),
            settings_ui::wide_card(
                t.resize_encoder,
                self.choices(
                    context,
                    &RESIZE_ENCODER_ORDER,
                    [
                        FallbackEncoder::Jpeg.label(),
                        FallbackEncoder::Png.label(),
                        FallbackEncoder::Bmp.label(),
                        FallbackEncoder::Gif.label(),
                        FallbackEncoder::Tiff.label(),
                    ],
                ),
            ),
            settings_ui::wide_card(
                t.resize_png,
                self.choices(
                    context,
                    &RESIZE_PNG_ORDER,
                    [t.png_default, t.png_on, t.png_off],
                ),
            ),
        ]
    }

    fn power_mouse(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let m = self.t().mouse;
        let status = if !s.mouse.enabled {
            m.status_off
        } else if s.mouse_elevated {
            m.status_elevated
        } else {
            m.status_on
        };
        let status_caption = format!("{status}\n{}", m.elevated_hint);
        let mut rows = vec![
            self.toggle_described(context, AppCommand::ToggleMouse, m.enabled, &status_caption),
            settings_ui::section(m.section_buttons),
        ];
        for &trigger in PAGE_TRIGGERS {
            rows.push(self.mouse_binding_card(context, trigger));
        }
        rows.push(settings_ui::section(m.section_tilt));
        rows.push(settings_ui::caption(m.tilt_hint));
        for &trigger in TILT_TRIGGERS {
            rows.push(self.mouse_binding_card(context, trigger));
        }
        rows.push(settings_ui::section(m.section_extra_keys));
        rows.push(settings_ui::caption(m.fkey_hint));
        let capturing_fkey = s.capturing == Some(CaptureTarget::MouseAddFKey);
        let add_label = if capturing_fkey {
            m.capturing_fkey
        } else {
            m.add_extra_key
        };
        rows.push(
            Button::new()
                .style(capturing_fkey.then_some(ButtonStyle::Accent))
                .on_click(context.message(Message::PowerMouse(PowerMouseAction::BeginAddFKey)))
                .content(add_label),
        );
        let mut fkeys: Vec<_> = s
            .mouse
            .bindings
            .keys()
            .copied()
            .filter(|trigger| trigger.is_function_key())
            .collect();
        fkeys.sort();
        for trigger in fkeys {
            rows.push(self.mouse_binding_card(context, trigger));
            rows.push(
                Button::new()
                    .on_click(
                        context.message(Message::PowerMouse(PowerMouseAction::RemoveFKey {
                            trigger,
                        })),
                    )
                    .content(m.remove_key),
            );
        }
        rows.push(
            Button::new()
                .on_click(context.message(Message::PowerMouse(PowerMouseAction::ResetDefaults)))
                .content(m.reset_defaults),
        );
        rows
    }

    fn mouse_binding_card(&self, context: &ViewContext<Self>, trigger: Trigger) -> View {
        let m = self.t().mouse;
        let current = self.snapshot.mouse.binding(trigger);
        let kinds = available_kinds(trigger);
        let kind = binding_kind(&current);
        let kind_index = kinds.iter().position(|item| *item == kind);
        let kind_labels: Vec<&str> = kinds.iter().map(|item| item.label(m)).collect();
        let kind_source = current.clone();
        let kind_combo = ComboBox::new()
            .min_width(220.0)
            .items_source(kind_labels)
            .selected_index(kind_index)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                let Some(index) = index else {
                    return Message::Noop;
                };
                let Some(&next_kind) = kinds.get(index) else {
                    return Message::Noop;
                };
                if next_kind == kind {
                    return Message::Noop;
                }
                let binding = coerce_binding(&kind_source, next_kind);
                Message::PowerMouse(PowerMouseAction::SetBinding { trigger, binding })
            }));
        let mut body = vec![settings_ui::card(m.binding_kind, kind_combo)];
        match &current {
            Binding::PassThrough => {}
            Binding::Single(action) => {
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Single,
                    m.slot_action,
                    action,
                ));
            }
            Binding::LongPress { short, long } => {
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Short,
                    m.slot_short,
                    short,
                ));
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Long,
                    m.slot_long,
                    long,
                ));
                body.push(settings_ui::caption(m.long_press_hint));
            }
            Binding::Gesture {
                click,
                up,
                down,
                left,
                right,
            } => {
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Click,
                    m.slot_click,
                    click,
                ));
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Up,
                    m.slot_up,
                    up,
                ));
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Down,
                    m.slot_down,
                    down,
                ));
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Left,
                    m.slot_left,
                    left,
                ));
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Right,
                    m.slot_right,
                    right,
                ));
                body.push(settings_ui::caption(m.gesture_hint));
            }
        }
        settings_ui::wide_card(m.trigger(trigger), settings_ui::stack(body, 8.0))
    }

    fn mouse_action_slot(
        &self,
        context: &ViewContext<Self>,
        trigger: Trigger,
        current: Binding,
        slot: ActionSlot,
        label: &str,
        action: &Action,
    ) -> View {
        let m = self.t().mouse;
        let catalog_len = Action::CATALOG.len();
        let mut labels: Vec<String> = m.action_labels().into_iter().map(str::to_string).collect();
        let record_label = match action {
            Action::Custom(hotkey) => hotkey.label(),
            _ => m.record_shortcut.to_string(),
        };
        labels.push(record_label);
        let selected = match action {
            Action::Custom(_) => Some(catalog_len),
            other => other.catalog_index(),
        };
        let capturing =
            self.snapshot.capturing == Some(CaptureTarget::MouseCustom { trigger, slot });
        let combo = ComboBox::new()
            .min_width(220.0)
            .items_source(labels)
            .selected_index(selected)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                let Some(index) = index else {
                    return Message::Noop;
                };
                if index == catalog_len {
                    return Message::PowerMouse(PowerMouseAction::BeginCustomCapture {
                        trigger,
                        slot,
                    });
                }
                let Some(next) = Action::CATALOG.get(index).cloned() else {
                    return Message::Noop;
                };
                let binding = current.with_slot(slot, next);
                if binding == current {
                    return Message::Noop;
                }
                Message::PowerMouse(PowerMouseAction::SetBinding { trigger, binding })
            }));
        let control: View = if capturing {
            settings_ui::stack(
                vec![combo.into(), settings_ui::caption(self.t().hotkey_capture)],
                4.0,
            )
        } else {
            combo.into()
        };
        settings_ui::card(label, control)
    }

    fn general(&self, context: &ViewContext<Self>) -> Vec<View> {
        let t = self.t();
        let mut about_rows = vec![
            settings_ui::heading(LABEL_ABOUT_NAME, 20.0).into(),
            settings_ui::caption(t.about_sub),
        ];
        about_rows.extend(installed_about_facts(t).map(|(name, value)| {
            Grid::new()
                .columns([GridLength::Pixel(120.0), GridLength::STAR])
                .column_spacing(12.0)
                .children((
                    TextBlock::new().grid_column(0).text(name).opacity(0.8),
                    TextBlock::new()
                        .grid_column(1)
                        .text(value)
                        .is_text_selection_enabled(true)
                        .text_wrapping(TextWrapping::Wrap),
                ))
        }));
        let about_header = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(10.0)
            .children((
                FontIcon::new()
                    .grid_column(0)
                    .glyph(GLYPH_INFO)
                    .vertical_alignment(VerticalAlignment::Center),
                TextBlock::new()
                    .grid_column(1)
                    .text(t.general_about)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center),
            ));
        let changelog_header = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(10.0)
            .children((
                FontIcon::new()
                    .grid_column(0)
                    .glyph(GLYPH_HISTORY)
                    .vertical_alignment(VerticalAlignment::Center),
                TextBlock::new()
                    .grid_column(1)
                    .text(t.general_changelog)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center),
            ));
        let notes_link: View = match HyperlinkButton::new().navigate_uri(RELEASE_NOTES_URI) {
            Ok(button) => button
                .horizontal_alignment(HorizontalAlignment::Left)
                .content(
                    TextBlock::new()
                        .text(t.release_notes)
                        .text_wrapping(TextWrapping::Wrap),
                ),
            Err(_) => TextBlock::new()
                .text(t.release_notes)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        };
        let changelog_body = crate::changelog::home_notes();
        vec![
            self.home_hero(),
            StackPanel::new()
                .spacing(8.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(t.welcome_title)
                        .font_size(32.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_wrapping(TextWrapping::Wrap)
                        .horizontal_alignment(HorizontalAlignment::Center),
                    TextBlock::new()
                        .text(t.welcome_lead)
                        .text_wrapping(TextWrapping::Wrap)
                        .opacity(0.85)
                        .horizontal_alignment(HorizontalAlignment::Center),
                    TextBlock::new()
                        .text(t.welcome_explore)
                        .text_wrapping(TextWrapping::Wrap)
                        .opacity(0.75)
                        .horizontal_alignment(HorizontalAlignment::Center),
                )),
            settings_ui::section(t.general_tools),
            self.tool_tiles(context),
            about_header,
            settings_ui::surface(settings_ui::stack(about_rows, 6.0)),
            changelog_header,
            settings_ui::surface(settings_ui::stack(
                vec![
                    TextBlock::new()
                        .text(format!(
                            "{} v{}",
                            t.general_version,
                            env!("CARGO_PKG_VERSION")
                        ))
                        .font_weight(FontWeight::SEMI_BOLD)
                        .into(),
                    TextBlock::new()
                        .text(changelog_body)
                        .text_wrapping(TextWrapping::Wrap)
                        .opacity(0.85)
                        .into(),
                    notes_link,
                ],
                8.0,
            )),
        ]
    }

    fn home_hero(&self) -> View {
        let phase = self.snapshot.hero_phase;
        let tiles = [
            (Page::PowerStore.glyph(), 36.0, 18.0, 0usize),
            (Page::MediaRenamer.glyph(), 120.0, 8.0, 1),
            (Page::AlwaysOn.glyph(), 210.0, 22.0, 2),
            (Page::AlwaysTop.glyph(), 68.0, 72.0, 3),
            (Page::ScreenRuler.glyph(), 168.0, 64.0, 4),
            (Page::ImageResizer.glyph(), 258.0, 78.0, 5),
            (Page::PowerMouse.glyph(), 108.0, 118.0, 6),
            (Page::General.selected_glyph(), 188.0, 128.0, 7),
        ];
        let children: Vec<KeyedView> = tiles
            .into_iter()
            .map(|(glyph, left, top, index)| {
                let pulse = ((usize::from(phase).wrapping_add(index * 3)) % 6) as f64;
                let opacity = 0.45 + (pulse / 5.0) * 0.55;
                let size = if index == 7 { 56.0 } else { 44.0 };
                KeyedView::new(
                    index,
                    Border::new()
                        .width(size)
                        .height(size)
                        .corner_radius(CornerRadius::uniform(12.0))
                        .background(ThemeBrush::CardBackground)
                        .border_brush(ThemeBrush::CardStroke)
                        .border_thickness(Thickness::uniform(1.0))
                        .horizontal_alignment(HorizontalAlignment::Left)
                        .vertical_alignment(VerticalAlignment::Top)
                        .margin(Thickness::new(left, top, 0.0, 0.0))
                        .opacity(opacity)
                        .opacity_transition(HERO_FADE)
                        .content(
                            FontIcon::new()
                                .glyph(glyph)
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .vertical_alignment(VerticalAlignment::Center),
                        ),
                )
            })
            .collect();
        Grid::new()
            .height(200.0)
            .max_width(360.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .margin(Thickness::new(0.0, 8.0, 0.0, 8.0))
            .keyed_children(children)
    }

    fn tool_tiles(&self, context: &ViewContext<Self>) -> View {
        let t = self.t();
        let tools = [
            (
                Page::PowerStore,
                t.nav_power_store,
                AppCommand::ToggleModulePowerStore,
            ),
            (
                Page::MediaRenamer,
                t.rename.nav,
                AppCommand::ToggleModuleMediaRenamer,
            ),
            (
                Page::AlwaysOn,
                t.nav_always_on,
                AppCommand::ToggleModuleAlwaysOn,
            ),
            (
                Page::AlwaysTop,
                t.nav_always_top,
                AppCommand::ToggleModuleAlwaysTop,
            ),
            (
                Page::ScreenRuler,
                t.nav_screen_ruler,
                AppCommand::ToggleModuleScreenRuler,
            ),
            (
                Page::ImageResizer,
                t.nav_image_resizer,
                AppCommand::ToggleModuleImageResizer,
            ),
            (Page::PowerMouse, t.mouse.nav, AppCommand::ToggleMouse),
        ];
        let mut left = Vec::new();
        let mut right = Vec::new();
        for (index, (page, title, command)) in tools.into_iter().enumerate() {
            let tile = KeyedView::new(page.tag(), self.tool_tile(context, page, title, command));
            if index % 2 == 0 {
                left.push(tile);
            } else {
                right.push(tile);
            }
        }
        Grid::new()
            .columns([GridLength::STAR, GridLength::STAR])
            .column_spacing(12.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .spacing(12.0)
                    .keyed_children(left),
                StackPanel::new()
                    .grid_column(1)
                    .spacing(12.0)
                    .keyed_children(right),
            ))
    }

    fn tool_tile(
        &self,
        context: &ViewContext<Self>,
        page: Page,
        title: &str,
        command: AppCommand,
    ) -> View {
        settings_ui::surface(
            StackPanel::new().spacing(12.0).children((
                FontIcon::new()
                    .glyph(page.glyph())
                    .horizontal_alignment(HorizontalAlignment::Left),
                TextBlock::new()
                    .text(title)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_wrapping(TextWrapping::Wrap),
                settings_ui::toggle_switch(
                    ToggleSwitch::new()
                        .is_on(self.toggled(command))
                        .horizontal_alignment(HorizontalAlignment::Left)
                        .on_toggled(context.callback(move |on| Message::Toggle(command, on))),
                ),
            )),
        )
    }

    fn settings(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let language = ComboBox::new()
            .min_width(180.0)
            .items_source([t.lang_system, t.lang_tr, t.lang_en, t.lang_ar])
            .selected_index(self.chosen(&LANG_ORDER))
            .on_selection_changed(
                context.callback(move |index: Option<usize>| Message::Choose(&LANG_ORDER, index)),
            );
        let theme = ComboBox::new()
            .min_width(180.0)
            .items_source([t.theme_system, t.theme_light, t.theme_dark])
            .selected_index(self.chosen(&THEME_ORDER))
            .on_selection_changed(
                context.callback(move |index: Option<usize>| Message::Choose(&THEME_ORDER, index)),
            );
        let color_link = HyperlinkButton::new()
            .on_click(context.message(Message::Command(AppCommand::OpenColorSettings)))
            .content(t.color_settings);
        let opacity = s.shell_opacity;
        let slider = Slider::new()
            .minimum(50.0)
            .maximum(100.0)
            .step_frequency(1.0)
            .value(opacity as f64)
            .min_width(200.0)
            .on_value_changed(
                context
                    .callback(move |value: f64| Message::ApplyShellOpacity(value.round() as i32)),
            );
        vec![
            settings_ui::section(t.general_startup),
            self.toggle_described(
                context,
                AppCommand::ToggleStartup,
                t.startup,
                t.startup_hint,
            ),
            settings_ui::section(t.general_appearance),
            settings_ui::described_card(
                t.language,
                settings_ui::caption(t.language_hint),
                language,
            ),
            settings_ui::described_card(t.theme, color_link, theme),
            settings_ui::described_card(
                t.shell_opacity,
                settings_ui::caption(t.shell_opacity_hint),
                StackPanel::new()
                    .spacing(8.0)
                    .children((TextBlock::new().text(format!("%{opacity}")), slider)),
            ),
            self.toggle_described(
                context,
                AppCommand::ToggleShowTray,
                t.show_tray,
                t.show_tray_hint,
            ),
            settings_ui::section(t.general_session),
            settings_ui::card(
                &s.hint,
                Button::new()
                    .on_click(context.message(Message::Command(AppCommand::Exit)))
                    .content(t.exit),
            ),
        ]
    }

    fn toggle_switch(&self, context: &ViewContext<Self>, command: AppCommand) -> View {
        settings_ui::toggle_switch(
            ToggleSwitch::new()
                .is_on(self.toggled(command))
                .on_toggled(context.callback(move |on| Message::Toggle(command, on))),
        )
    }

    fn toggle_card(&self, context: &ViewContext<Self>, command: AppCommand, label: &str) -> View {
        settings_ui::card(label, self.toggle_switch(context, command))
    }

    fn toggle_described(
        &self,
        context: &ViewContext<Self>,
        command: AppCommand,
        title: &str,
        description: &str,
    ) -> View {
        settings_ui::described_card(
            title,
            settings_ui::caption(description),
            self.toggle_switch(context, command),
        )
    }

    fn hotkey_card(
        &self,
        context: &ViewContext<Self>,
        command: AppCommand,
        label: &str,
        hotkey: &str,
    ) -> View {
        let capturing = self.snapshot.capturing == Some(CaptureTarget::Hotkey(command));
        let text = if capturing {
            self.t().hotkey_capture
        } else {
            hotkey
        };
        settings_ui::hotkey_card(
            label,
            settings_ui::hotkey_button(capturing)
                .on_click(context.message(Message::CaptureHotkey(command)))
                .content(text),
        )
    }

    pub(super) fn choices<const N: usize>(
        &self,
        context: &ViewContext<Self>,
        order: &'static [AppCommand; N],
        labels: [&str; N],
    ) -> View {
        let selected = self.chosen(order);
        settings_ui::stack(
            labels
                .into_iter()
                .enumerate()
                .map(|(index, label)| {
                    let on = selected == Some(index);
                    settings_ui::choice_button(on)
                        .on_click(context.message(Message::Choose(order, Some(index))))
                        .content(settings_ui::choice_content(label, on))
                })
                .collect(),
            6.0,
        )
    }

    fn stepper_card(
        &self,
        context: &ViewContext<Self>,
        title: &str,
        value: String,
        down: AppCommand,
        up: AppCommand,
    ) -> View {
        settings_ui::stepper_card(
            title,
            value,
            Button::new()
                .width(40.0)
                .on_click(context.message(Message::Command(down)))
                .content(settings_ui::STEPPER_DOWN),
            Button::new()
                .width(40.0)
                .on_click(context.message(Message::Command(up)))
                .content(settings_ui::STEPPER_UP),
        )
    }

    fn color_button(
        &self,
        context: &ViewContext<Self>,
        target: ColorTarget,
        colorref: u32,
    ) -> View {
        let [r, g, b, _] = colorref.to_le_bytes();
        Button::new()
            .content(settings_ui::color_swatch(colorref))
            .flyout_with(
                Flyout::rich(
                    ColorPicker::new()
                        .color(Color::rgb(r, g, b))
                        .is_alpha_enabled(false)
                        .on_color_changed(context.callback(move |color: Color| {
                            Message::ApplyColor(
                                target,
                                u32::from_le_bytes([color.r, color.g, color.b, 0]),
                            )
                        })),
                )
                .placement(FlyoutPlacement::Bottom),
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindingKind {
    PassThrough,
    Single,
    LongPress,
    Gesture,
}

impl BindingKind {
    fn label(self, m: &crate::locale::MouseStrings) -> &'static str {
        match self {
            Self::PassThrough => m.pass_through,
            Self::Single => m.kind_single,
            Self::LongPress => m.kind_long_press,
            Self::Gesture => m.kind_gesture,
        }
    }
}

fn available_kinds(trigger: Trigger) -> &'static [BindingKind] {
    if trigger.is_tilt() {
        &[BindingKind::PassThrough, BindingKind::Single]
    } else if trigger.is_function_key() {
        &[
            BindingKind::PassThrough,
            BindingKind::Single,
            BindingKind::LongPress,
        ]
    } else {
        &[
            BindingKind::PassThrough,
            BindingKind::Single,
            BindingKind::LongPress,
            BindingKind::Gesture,
        ]
    }
}

fn binding_kind(binding: &Binding) -> BindingKind {
    match binding {
        Binding::PassThrough => BindingKind::PassThrough,
        Binding::Single(_) => BindingKind::Single,
        Binding::LongPress { .. } => BindingKind::LongPress,
        Binding::Gesture { .. } => BindingKind::Gesture,
    }
}

fn primary_action(binding: &Binding) -> Action {
    match binding {
        Binding::PassThrough => Action::Copy,
        Binding::Single(action) => action.clone(),
        Binding::LongPress { short, .. } => short.clone(),
        Binding::Gesture { click, .. } => click.clone(),
    }
}

fn coerce_binding(current: &Binding, kind: BindingKind) -> Binding {
    let primary = primary_action(current);
    match kind {
        BindingKind::PassThrough => Binding::PassThrough,
        BindingKind::Single => Binding::Single(primary),
        BindingKind::LongPress => match current {
            Binding::LongPress { short, long } => Binding::LongPress {
                short: short.clone(),
                long: long.clone(),
            },
            Binding::Gesture { click, up, .. } => Binding::LongPress {
                short: click.clone(),
                long: up.clone(),
            },
            _ => Binding::LongPress {
                short: primary,
                long: Action::Paste,
            },
        },
        BindingKind::Gesture => match current {
            Binding::Gesture {
                click,
                up,
                down,
                left,
                right,
            } => Binding::Gesture {
                click: click.clone(),
                up: up.clone(),
                down: down.clone(),
                left: left.clone(),
                right: right.clone(),
            },
            Binding::LongPress { short, long } => Binding::Gesture {
                click: short.clone(),
                up: long.clone(),
                down: Action::None,
                left: Action::None,
                right: Action::None,
            },
            _ => Binding::Gesture {
                click: primary,
                up: Action::VolumeUp,
                down: Action::VolumeDown,
                left: Action::None,
                right: Action::None,
            },
        },
    }
}
