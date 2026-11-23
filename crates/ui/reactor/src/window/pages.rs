//! Per-tool settings pages composed from shared `settings_ui` surfaces.
use super::super::settings_ui;
use super::view_helpers::{icon_button_width, sized_icon};
use super::{
    CaptureTarget, ColorTarget, Command, LANG_ORDER, MODE_ORDER, Message, Page, PageGlyph,
    RELEASE_NOTES_URI, RESIZE_ENCODER_ORDER, RESIZE_PNG_ORDER, RULER_MODE_ORDER, RULER_UNIT_ORDER,
    Shell, THEME_ORDER, installed_about_facts, page_copy, tray_page,
};
use powerkit_core::always_on::MAX_INTERVAL_HOURS;
use powerkit_core::image_resizer::FallbackEncoder;
use powerkit_core::tray_layout::TrayEdit;
use std::time::Duration;
use windows_reactor::*;

const LABEL_ABOUT_NAME: &str = "PowerKit";
const GLYPH_INFO: &str = "\u{E946}";
const GLYPH_HISTORY: &str = "\u{E81C}";
const GLYPH_UP: &str = "\u{E70E}";
const GLYPH_DOWN: &str = "\u{E70D}";
/// Same keyboard glyph as the tray Shortcuts submenu.
const GLYPH_KEYBOARD: &str = "\u{E765}";
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
            Page::PowerMouse | Page::PowerStore | Page::MediaRenamer => Vec::new(),
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
                        .on_click(context.message(Message::Command(Command::AlwaysOnBindProcess)))
                        .content(t.bind_action),
                ],
                8.0,
            )),
            self.toggle_card(context, Command::ToggleDisplay, t.display),
            settings_ui::section(t.section_controls),
            self.hotkey_card(
                context,
                Command::CaptureAlwaysOnHotkey,
                t.hotkey_always_on,
                &s.always_on_hotkey,
            ),
            self.toggle_described(
                context,
                Command::ToggleAlwaysOnWarn,
                t.warn_toggle,
                t.warn_hint,
            ),
            settings_ui::section(t.power_title),
            self.toggle_described(context, Command::ToggleAcOnly, t.ac_only, t.ac_only_hint),
            self.stepper_card(
                context,
                t.battery_floor,
                battery_floor,
                Command::BatteryFloorDown,
                Command::BatteryFloorUp,
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
                    .on_click(context.message(Message::Command(Command::TogglePin)))
                    .content(t.pin_action),
            ),
            settings_ui::section(t.section_shortcuts),
            self.hotkey_card(
                context,
                Command::CapturePinHotkey,
                t.hotkey_pin,
                &s.pin_hotkey,
            ),
            self.hotkey_card(
                context,
                Command::CaptureOpacityUpHotkey,
                t.hotkey_op_up,
                &s.opacity_up_hotkey,
            ),
            self.hotkey_card(
                context,
                Command::CaptureOpacityDownHotkey,
                t.hotkey_op_down,
                &s.opacity_down_hotkey,
            ),
            settings_ui::section(t.section_border),
            self.toggle_card(context, Command::ToggleFrameAccent, t.frame_accent),
            settings_ui::card(
                t.frame_color,
                self.color_button(context, ColorTarget::Frame, s.frame_color),
            ),
            self.stepper_card(
                context,
                t.frame_opacity,
                t.percent(s.frame_opacity),
                Command::FrameOpacityDown,
                Command::FrameOpacityUp,
            ),
            self.stepper_card(
                context,
                t.frame_thickness,
                format!("{} px", s.frame_thickness),
                Command::FrameThicknessDown,
                Command::FrameThicknessUp,
            ),
            settings_ui::section(t.section_behavior),
            self.toggle_card(context, Command::ToggleSound, t.sound),
        ]
    }

    fn screen_ruler(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let measure = Button::new()
            .style(ButtonStyle::Accent)
            .on_click(context.message(Message::Command(Command::ToggleMeasure)))
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
                Command::CaptureRulerHotkey,
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
            self.toggle_card(context, Command::ToggleRulerContinuous, t.ruler_continuous),
            self.toggle_card(context, Command::ToggleRulerChannel, t.ruler_channel),
            self.toggle_card(context, Command::ToggleRulerFeet, t.ruler_feet),
            self.stepper_card(
                context,
                t.ruler_tolerance,
                s.ruler_tolerance.to_string(),
                Command::RulerToleranceDown,
                Command::RulerToleranceUp,
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
                    .on_click(context.message(Message::Command(Command::ResizeOpen)))
                    .content(t.resize_open),
            ),
            settings_ui::section(t.resize_options),
            settings_ui::caption(t.resize_options_hint),
            self.toggle_card(context, Command::ToggleResizeShrink, t.resize_shrink),
            self.toggle_card(context, Command::ToggleResizeReplace, t.resize_replace),
            self.toggle_card(
                context,
                Command::ToggleResizeOrientation,
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
            self.toggle_card(context, Command::ToggleResizeKeepDate, t.resize_keep_date),
            self.toggle_card(context, Command::ToggleResizeMetadata, t.resize_metadata),
            settings_ui::section(t.resize_section_encoding),
            self.stepper_card(
                context,
                t.resize_jpeg,
                resize.jpeg_quality.to_string(),
                Command::ResizeJpegDown,
                Command::ResizeJpegUp,
            ),
            self.stepper_card(
                context,
                t.resize_tiff,
                t.resize_tiff_label(resize.tiff).to_string(),
                Command::ResizeTiffDown,
                Command::ResizeTiffUp,
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
        let t = self.t();
        let mut about_rows = vec![
            settings_ui::heading(LABEL_ABOUT_NAME, 20.0).into(),
            settings_ui::caption(t.about_sub),
        ];
        about_rows.extend(
            installed_about_facts(t, self.snapshot.version, self.snapshot.packaged).map(
                |(name, value)| {
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
                },
            ),
        );
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
        let changelog_body = powerkit_core::changelog::home_notes();
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
                        .text(format!("{} v{}", t.general_version, self.snapshot.version))
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
                Command::ToggleModulePowerStore,
            ),
            (
                Page::MediaRenamer,
                t.rename.nav,
                Command::ToggleModuleMediaRenamer,
            ),
            (
                Page::AlwaysOn,
                t.nav_always_on,
                Command::ToggleModuleAlwaysOn,
            ),
            (
                Page::AlwaysTop,
                t.nav_always_top,
                Command::ToggleModuleAlwaysTop,
            ),
            (
                Page::ScreenRuler,
                t.nav_screen_ruler,
                Command::ToggleModuleScreenRuler,
            ),
            (
                Page::ImageResizer,
                t.nav_image_resizer,
                Command::ToggleModuleImageResizer,
            ),
            (Page::PowerMouse, t.mouse.nav, Command::ToggleMouse),
        ];
        let mut left = Vec::new();
        let mut right = Vec::new();
        // A tool the platform does not offer has no tile.
        let offered = tools
            .into_iter()
            .filter(|(page, ..)| self.snapshot.offers(*page));
        for (index, (page, title, command)) in offered.enumerate() {
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
        command: Command,
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
            .on_click(context.message(Message::Command(Command::OpenColorSettings)))
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
            self.toggle_described(context, Command::ToggleStartup, t.startup, t.startup_hint),
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
                Command::ToggleShowTray,
                t.show_tray,
                t.show_tray_hint,
            ),
            settings_ui::section(t.tray_menu_title),
            settings_ui::caption(t.tray_menu_hint),
            self.tray_tools(context),
            settings_ui::section(t.general_session),
            settings_ui::described_card(
                t.reset_settings,
                settings_ui::caption(t.reset_settings_hint),
                Button::new()
                    .on_click(context.message(Message::OpenResetDialog))
                    .content(t.reset_settings_action),
            ),
            settings_ui::card(
                &s.hint,
                Button::new()
                    .on_click(context.message(Message::Command(Command::Exit)))
                    .content(t.exit),
            ),
            ContentDialog::new()
                .is_open(self.reset_dialog_open)
                .title(t.reset_settings)
                .primary_button_text(t.reset_settings_action)
                .close_button_text(t.cancel)
                .on_closed(context.callback(Message::ResetDialogClosed))
                .content(
                    TextBlock::new()
                        .text(t.reset_settings_confirm)
                        .text_wrapping(TextWrapping::Wrap),
                ),
        ]
    }

    /// One row per tray tool, in menu order: icon and name on the left, then the
    /// move buttons and the visibility switch. Rows are keyed by tool so a move
    /// reorders the existing rows instead of rewriting each switch in place.
    fn tray_tools(&self, context: &ViewContext<Self>) -> View {
        let t = self.t();
        let items = self.snapshot.tray.items();
        let last = items.len() - 1;
        let move_button = |glyph: &str, enabled: bool, edit: TrayEdit, tooltip: &str| {
            let side = icon_button_width(12.0, 32.0);
            Button::new()
                .width(side)
                .height(side)
                .style(ButtonStyle::Subtle)
                .is_enabled(enabled)
                .horizontal_content_alignment(HorizontalAlignment::Center)
                .vertical_content_alignment(VerticalAlignment::Center)
                .on_click(context.message(Message::Tray(edit)))
                .content(sized_icon(glyph, 12.0))
                .tooltip(tooltip)
        };
        let rows = items.iter().enumerate().map(|(index, item)| {
            let tool = item.tool;
            let page = tray_page(tool);
            let controls = StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(4.0)
                .children((
                    move_button(GLYPH_UP, index > 0, TrayEdit::MoveUp(tool), t.move_up),
                    move_button(
                        GLYPH_DOWN,
                        index < last,
                        TrayEdit::MoveDown(tool),
                        t.move_down,
                    ),
                    settings_ui::toggle_switch(
                        ToggleSwitch::new()
                            .is_on(item.shown)
                            .margin(Thickness::new(12.0, 0.0, 0.0, 0.0))
                            .on_toggled(context.callback(move |on| Message::TrayShown(tool, on))),
                    ),
                ));
            let label = StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(12.0)
                .children((
                    sized_icon(page.glyph(), 16.0),
                    TextBlock::new()
                        .text(page.label(self.snapshot.ui_language))
                        .vertical_alignment(VerticalAlignment::Center),
                ));
            KeyedView::new(tool.key(), settings_ui::row(label, controls))
        });
        // The Shortcuts submenu stays below the tools, so it has no move buttons.
        let shortcuts = settings_ui::row(
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(12.0)
                .children((
                    sized_icon(GLYPH_KEYBOARD, 16.0),
                    TextBlock::new()
                        .text(t.section_shortcuts)
                        .vertical_alignment(VerticalAlignment::Center),
                )),
            settings_ui::toggle_switch(
                ToggleSwitch::new()
                    .is_on(self.snapshot.tray.shortcuts())
                    .on_toggled(context.callback(Message::TrayShortcuts)),
            ),
        );
        let rows = rows.chain(std::iter::once(KeyedView::new("shortcuts", shortcuts)));
        StackPanel::new().spacing(8.0).keyed_children(rows)
    }

    fn toggle_switch(&self, context: &ViewContext<Self>, command: Command) -> View {
        settings_ui::toggle_switch(
            ToggleSwitch::new()
                .is_on(self.toggled(command))
                .on_toggled(context.callback(move |on| Message::Toggle(command, on))),
        )
    }

    fn toggle_card(&self, context: &ViewContext<Self>, command: Command, label: &str) -> View {
        settings_ui::card(label, self.toggle_switch(context, command))
    }

    fn toggle_described(
        &self,
        context: &ViewContext<Self>,
        command: Command,
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
        command: Command,
        label: &str,
        hotkey: &str,
    ) -> View {
        let capturing = self.snapshot.capturing == Some(CaptureTarget::Hotkey(command));
        let text: View = if capturing {
            TextBlock::new().text(self.t().hotkey_capture).into()
        } else {
            settings_ui::hotkey_keys(hotkey)
        };
        let button = settings_ui::hotkey_button(capturing)
            .on_click(context.message(Message::CaptureHotkey(command)))
            .content(text);
        let note = self
            .snapshot
            .hotkey_notes
            .iter()
            .find(|(slot, _)| *slot == command)
            .map(|(_, note)| note.as_str());
        settings_ui::hotkey_card(label, note, button)
    }

    pub(super) fn choices<const N: usize>(
        &self,
        context: &ViewContext<Self>,
        order: &'static [Command; N],
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
        down: Command,
        up: Command,
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
        color: powerkit_core::primitives::Rgb,
    ) -> View {
        let powerkit_core::primitives::Rgb { r, g, b } = color;
        Button::new()
            .content(settings_ui::color_swatch(color))
            .flyout_with(
                Flyout::rich(
                    ColorPicker::new()
                        .color(Color::rgb(r, g, b))
                        .is_alpha_enabled(false)
                        .on_color_changed(context.callback(move |color: Color| {
                            Message::ApplyColor(
                                target,
                                powerkit_core::primitives::Rgb::new(color.r, color.g, color.b),
                            )
                        })),
                )
                .placement(FlyoutPlacement::Bottom),
            )
    }
}
