//! Per-tool settings pages composed from shared `settings_ui` surfaces.
use super::super::settings_ui;
use super::{
    AppCommand, ColorTarget, LANG_ORDER, MODE_ORDER, Message, Page, RELEASE_NOTES_URI,
    RESIZE_ENCODER_ORDER, RESIZE_PNG_ORDER, RULER_MODE_ORDER, RULER_UNIT_ORDER, Shell, THEME_ORDER,
    installed_about_facts, page_copy,
};
use crate::always_on::MAX_INTERVAL_HOURS;
use crate::image_resizer::FallbackEncoder;
use windows_reactor::*;

const LABEL_ABOUT_NAME: &str = "PowerKit";
const GLYPH_SYNC: &str = "\u{E72C}";

impl Shell {
    pub(super) fn page_view(&self, context: &ViewContext<Self>) -> View {
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
            Page::General => self.general(context),
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
        vec![
            settings_ui::described_card(
                t.pin,
                settings_ui::caption(&s.top_status),
                Button::new()
                    .on_click(context.message(Message::Command(AppCommand::TogglePin)))
                    .content(t.pin_action),
            ),
            self.toggle_card(context, AppCommand::ToggleSound, t.sound),
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
            self.toggle_card(context, AppCommand::ToggleFrameAccent, t.frame_accent),
            settings_ui::card(
                t.frame_color,
                self.color_button(context, ColorTarget::Frame, s.frame_color),
            ),
            self.stepper_card(
                context,
                t.frame_opacity,
                format!("%{}", s.frame_opacity),
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
            settings_ui::caption(t.always_top_lead),
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
        vec![
            settings_ui::described_card(t.ruler_title, settings_ui::caption(status), measure),
            self.hotkey_card(
                context,
                AppCommand::CaptureRulerHotkey,
                t.hotkey_ruler,
                &s.ruler_hotkey,
            ),
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
            settings_ui::card(
                t.ruler_color,
                self.color_button(context, ColorTarget::Ruler, s.ruler_color),
            ),
            settings_ui::caption(t.ruler_lead),
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
            settings_ui::card(
                t.resize_open,
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(context.message(Message::Command(AppCommand::ResizeOpen)))
                    .content(t.resize_open),
            ),
            self.toggle_card(context, AppCommand::ToggleResizeShrink, t.resize_shrink),
            self.toggle_card(context, AppCommand::ToggleResizeReplace, t.resize_replace),
            self.toggle_card(
                context,
                AppCommand::ToggleResizeOrientation,
                t.resize_orientation,
            ),
            self.toggle_card(context, AppCommand::ToggleResizeMetadata, t.resize_metadata),
            self.toggle_card(
                context,
                AppCommand::ToggleResizeKeepDate,
                t.resize_keep_date,
            ),
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
                resize.tiff.label().to_string(),
                AppCommand::ResizeTiffDown,
                AppCommand::ResizeTiffUp,
            ),
            settings_ui::wide_card(
                t.resize_file,
                settings_ui::stack(
                    vec![filename_row, settings_ui::caption(t.resize_file_hint)],
                    6.0,
                ),
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

    fn general(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let update_label = if s.update_ready {
            t.update_ready
        } else {
            t.update_check
        };
        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        let notes: View = match HyperlinkButton::new().navigate_uri(RELEASE_NOTES_URI) {
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
        let update = settings_ui::surface(
            StackPanel::new().spacing(10.0).children((
                Grid::new()
                    .columns([GridLength::Auto, GridLength::STAR])
                    .column_spacing(12.0)
                    .children((
                        FontIcon::new()
                            .grid_column(0)
                            .glyph(GLYPH_SYNC)
                            .vertical_alignment(VerticalAlignment::Top)
                            .margin(Thickness::new(0.0, 4.0, 0.0, 0.0)),
                        StackPanel::new().grid_column(1).spacing(2.0).children((
                            TextBlock::new()
                                .text(version)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .text_wrapping(TextWrapping::Wrap),
                            notes,
                            TextBlock::new()
                                .text(s.update_status)
                                .opacity(0.8)
                                .text_wrapping(TextWrapping::WrapWholeWords),
                        )),
                    )),
                Button::new()
                    .style(ButtonStyle::Accent)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .on_click(context.message(Message::Command(AppCommand::Update)))
                    .content(update_label),
            )),
        );
        let mut about = vec![
            settings_ui::heading(LABEL_ABOUT_NAME, 20.0).into(),
            settings_ui::caption(t.about_sub),
        ];
        about.extend(installed_about_facts(t).map(|(name, value)| {
            Grid::new()
                .columns([GridLength::Pixel(96.0), GridLength::STAR])
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
        vec![
            settings_ui::section(t.general_updates),
            update,
            settings_ui::section(t.general_session),
            settings_ui::surface(settings_ui::stack(about, 6.0)),
            settings_ui::caption(&s.hint),
        ]
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
            settings_ui::section(t.general_updates),
            self.toggle_described(
                context,
                AppCommand::ToggleAutoUpdate,
                t.auto_update,
                t.auto_update_hint,
            ),
            self.toggle_card(context, AppCommand::ToggleUpdateNotify, t.update_notify),
            self.toggle_card(context, AppCommand::ToggleShowNotes, t.show_notes),
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
        let capturing = self.snapshot.capturing == Some(command);
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

    fn choices<const N: usize>(
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
