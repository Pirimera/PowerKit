//! The redesigned PowerMouse workspace: a sticky device header with real
//! device imagery, a segmented tab bar, an OpenLogi-inspired interactive mouse
//! stage with routed leader lines and fixed right-side binding inspector, a
//! pointer & scrolling dashboard, and an interactive radial Actions Ring editor.
use super::super::settings_ui;
use super::view_helpers::{
    PillTone, accent_bar, drawer_icon_badge, hairline, icon_button, inset_notice, sized_icon,
    status_pill,
};
use super::{CaptureTarget, Command, Message, MouseSlot, PowerMouseAction, Shell};
use powerkit_app::ui::{DeviceImage, MouseImageKind as ImageKind};
use powerkit_core::locale::MouseStrings;
use powerkit_core::power_mouse::device::{AdapterLink, AdapterStatus, Device, Transport};
use powerkit_core::power_mouse::diagram::{
    self, CANVAS_H, CANVAS_W, CARD_H, CARD_W, CalloutId, DeviceArt, MouseRect, PlacedCallout, Side,
};
use powerkit_core::power_mouse::hidpp::WheelMode;
use powerkit_core::power_mouse::{
    Action, ActionSlot, Binding, PAGE_TRIGGERS, TILT_TRIGGERS, Trigger, WORKFLOW_MAX_STEPS,
};
use std::sync::Arc;
use windows_reactor::*;

/// Pause lengths a PowerMouse workflow step can be set to.
const WORKFLOW_WAITS_MS: [u16; 5] = [100, 250, 500, 1000, 2000];
/// Width of the fixed right-side inspector panel (matches OpenLogi's 328 DIP).
const INSPECTOR_WIDTH: f64 = 328.0;
/// Size of the radial Actions Ring preview canvas and each circular slot node.
const RING_CANVAS: f64 = 356.0;
const RING_NODE: f64 = 68.0;

// Segoe Fluent Icons used across the PowerMouse workspace.
const GLYPH_MOUSE: &str = "\u{E962}";
const GLYPH_SPEED: &str = "\u{EC4A}";
const GLYPH_RING: &str = "\u{E80A}";
const GLYPH_WHEEL: &str = "\u{E895}";
const GLYPH_RESET: &str = "\u{E7A7}";
const GLYPH_BLUETOOTH: &str = "\u{E702}";
const GLYPH_WIRELESS: &str = "\u{E701}";
const GLYPH_USB: &str = "\u{E88E}";
const GLYPH_BATTERY_FULL: &str = "\u{E83F}";
const GLYPH_BATTERY_MID: &str = "\u{E855}";
const GLYPH_BATTERY_LOW: &str = "\u{E851}";
const GLYPH_BATTERY_CHARGE: &str = "\u{E85A}";
const GLYPH_CHECK: &str = "\u{E73E}";
const GLYPH_WARNING: &str = "\u{E7BA}";
const GLYPH_INFO: &str = "\u{E946}";
const GLYPH_CHEVRON_RIGHT: &str = "\u{E76C}";
const GLYPH_CHEVRON_LEFT: &str = "\u{E76B}";
const GLYPH_UP: &str = "\u{E74A}";
const GLYPH_DOWN: &str = "\u{E74B}";
const GLYPH_LEFT: &str = "\u{E72B}";
const GLYPH_RIGHT: &str = "\u{E72A}";
const GLYPH_CLICK: &str = "\u{ECCC}";
const GLYPH_GESTURE: &str = "\u{E759}";
const GLYPH_FLASH: &str = "\u{E945}";
const GLYPH_CLOCK: &str = "\u{E823}";
const GLYPH_KEYBOARD: &str = "\u{E765}";
const GLYPH_ADD: &str = "\u{E710}";
const GLYPH_DELETE: &str = "\u{E74D}";
const GLYPH_CLEAR: &str = "\u{E711}";

/// Which part of the PowerMouse workspace is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MouseTab {
    #[default]
    Buttons,
    Scrolling,
    Ring,
}

/// Contextual Segoe Fluent icon for an action, used in callout cards,
/// gesture direction rows and the Actions Ring preview.
fn action_glyph(action: &Action) -> &'static str {
    match action {
        Action::None => "\u{E711}",
        Action::LeftClick
        | Action::RightClick
        | Action::MiddleClick
        | Action::MouseBack
        | Action::MouseForward => GLYPH_MOUSE,
        Action::BrowserBack | Action::PrevTab => GLYPH_LEFT,
        Action::BrowserForward | Action::NextTab => GLYPH_RIGHT,
        Action::TaskView => "\u{E7C4}",
        Action::PreviousDesktop => GLYPH_CHEVRON_LEFT,
        Action::NextDesktop => GLYPH_CHEVRON_RIGHT,
        Action::ShowDesktop => "\u{E7F4}",
        Action::Copy => "\u{E8C8}",
        Action::Paste => "\u{E77F}",
        Action::Cut => "\u{E8C6}",
        Action::Undo => "\u{E7A7}",
        Action::Redo => "\u{E7A6}",
        Action::SelectAll => "\u{E8B3}",
        Action::Find => "\u{E721}",
        Action::Save => "\u{E74E}",
        Action::NewTab => GLYPH_ADD,
        Action::CloseTab => GLYPH_CLEAR,
        Action::ReopenTab | Action::ReloadPage => "\u{E72C}",
        Action::PlayPause => "\u{E768}",
        Action::NextTrack => "\u{E893}",
        Action::PrevTrack => "\u{E892}",
        Action::VolumeUp => "\u{E995}",
        Action::VolumeDown => "\u{E993}",
        Action::MuteVolume => "\u{E74F}",
        Action::CaptureRegion => "\u{E722}",
        Action::LockScreen => "\u{E72E}",
        Action::PinLastWindow => "\u{E718}",
        Action::StartScreenRuler => "\u{ED5E}",
        Action::OpenPowerKit => "\u{E80F}",
        Action::ActionsRing => GLYPH_RING,
        Action::SwitchHost(_) => "\u{E7F8}",
        Action::HapticPulse => "\u{E877}",
        Action::DpiCycle => GLYPH_SPEED,
        Action::Custom(_) => GLYPH_KEYBOARD,
        Action::Text(_) => "\u{E8D2}",
        Action::Workflow(_) => GLYPH_FLASH,
        Action::Delay(_) => GLYPH_CLOCK,
    }
}

/// Contextual glyph for a binding summary in a callout card.
fn binding_glyph(binding: &Binding) -> &'static str {
    match binding {
        Binding::PassThrough => GLYPH_MOUSE,
        Binding::Single(action) | Binding::Hold(action) => action_glyph(action),
        Binding::LongPress { short, .. } => action_glyph(short),
        Binding::Gesture { .. } => GLYPH_GESTURE,
    }
}

/// Contextual glyph for a trigger in the inspector header.
fn trigger_glyph(trigger: Trigger) -> &'static str {
    if trigger.is_function_key() {
        GLYPH_KEYBOARD
    } else if trigger.is_ring_slot() {
        GLYPH_RING
    } else if trigger.is_thumb_wheel() || TILT_TRIGGERS.contains(&trigger) {
        GLYPH_WHEEL
    } else if trigger == Trigger::HapticPanel {
        "\u{E877}"
    } else if trigger == Trigger::GestureButton {
        GLYPH_GESTURE
    } else {
        GLYPH_MOUSE
    }
}

fn transport_glyph(transport: Transport) -> &'static str {
    match transport {
        Transport::Bluetooth => GLYPH_BLUETOOTH,
        Transport::BoltReceiver | Transport::UnifyingReceiver => GLYPH_WIRELESS,
        Transport::Usb => GLYPH_USB,
    }
}

fn battery_glyph(percent: u8, charging: bool) -> &'static str {
    if charging {
        GLYPH_BATTERY_CHARGE
    } else if percent <= 20 {
        GLYPH_BATTERY_LOW
    } else if percent <= 65 {
        GLYPH_BATTERY_MID
    } else {
        GLYPH_BATTERY_FULL
    }
}

/// One editor field: concise muted label above a stretched control.
fn field_block(label: &str, control: impl Into<View>) -> View {
    settings_ui::surface(settings_ui::stack(
        vec![
            TextBlock::new()
                .text(label)
                .font_size(12.0)
                .opacity(0.75)
                .into(),
            control.into(),
        ],
        6.0,
    ))
}

impl Shell {
    /// Full-height PowerMouse workspace pane: sticky device header, segmented
    /// tab bar and the active workspace body.
    pub(super) fn power_mouse_pane(&self, context: &ViewContext<Self>) -> View {
        let header = self.mouse_header(context);
        let tab_bar = self.mouse_tab_bar(context);
        let body: View = match self.mouse_tab {
            MouseTab::Buttons => self.mouse_buttons_workspace(context),
            MouseTab::Scrolling => self.mouse_scrolling_workspace(context),
            MouseTab::Ring => self.mouse_ring_workspace(context),
        };
        Grid::new()
            .rows([GridLength::Auto, GridLength::Auto, GridLength::STAR])
            .keyed_children([
                KeyedView::new("header", StackPanel::new().grid_row(0).children((header,))),
                KeyedView::new(
                    "tabs",
                    StackPanel::new()
                        .grid_row(1)
                        .children((tab_bar, hairline())),
                ),
                KeyedView::new(
                    match self.mouse_tab {
                        MouseTab::Buttons => "tab-buttons",
                        MouseTab::Scrolling => "tab-scrolling",
                        MouseTab::Ring => "tab-ring",
                    },
                    Grid::new().grid_row(2).children((body,)),
                ),
            ])
    }

    /// Lead device driving the page header and artwork: prefers an active
    /// adapter device, then any detected Logitech mouse.
    fn mouse_lead_device(&self) -> Option<&Device> {
        let s = &self.snapshot;
        s.mouse_devices
            .iter()
            .find(|device| s.mouse_adapters.contains_key(&device.key))
            .or_else(|| s.mouse_devices.first())
    }

    /// Artwork specification for the connected device, if recognized.
    fn mouse_lead_art(&self) -> Option<&'static DeviceArt> {
        let device = self.mouse_lead_device()?;
        let adapter_name = self
            .snapshot
            .mouse_adapters
            .get(&device.key)
            .map(|status| status.name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(device.name.as_str());
        diagram::device_art(device.product_id, adapter_name)
    }

    /// Sticky top device bar: device render thumbnail, title, live transport /
    /// battery / adapter status pills, master enable switch and reset action.
    fn mouse_header(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot;
        let m = self.t().mouse;
        let lead = self.mouse_lead_device();
        let art = self.mouse_lead_art();

        let title = lead.map_or_else(
            || m.hero_any_mouse.to_string(),
            |device| {
                s.mouse_adapters
                    .get(&device.key)
                    .map(|status| status.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| device.name.clone())
            },
        );

        // Real studio front render in the badge when cached; clean mouse icon otherwise.
        let thumb_content: View = art
            .and_then(|spec| {
                self.app
                    .mouse_image(spec.depot, ImageKind::Front)
                    .or_else(|| self.app.mouse_image(spec.depot, ImageKind::Side))
            })
            .map_or_else(
                || sized_icon(GLYPH_MOUSE, 22.0),
                |img| {
                    Image::new()
                        .source_data(EncodedImage::new(Arc::clone(&img.bytes)))
                        .stretch(Stretch::Uniform)
                        .margin(Thickness::uniform(4.0))
                        .into()
                },
            );
        let badge = Border::new()
            .grid_column(0)
            .width(48.0)
            .height(48.0)
            .corner_radius(CornerRadius::uniform(10.0))
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .vertical_alignment(VerticalAlignment::Center)
            .content(thumb_content);

        // Inline status pills beside / beneath the device title.
        let mut pills: Vec<View> = Vec::new();
        if let Some(device) = lead {
            let transport_label = match device.transport {
                Transport::Bluetooth => m.transport_bluetooth,
                Transport::BoltReceiver => m.transport_bolt,
                Transport::UnifyingReceiver => m.transport_unifying,
                Transport::Usb => m.transport_usb,
            };
            pills.push(status_pill(
                Some(transport_glyph(device.transport)),
                transport_label,
                PillTone::Neutral,
            ));
            match s.mouse_adapters.get(&device.key) {
                Some(status) => {
                    match status.link {
                        AdapterLink::Active => {
                            pills.push(status_pill(
                                Some(GLYPH_CHECK),
                                m.status_connected,
                                PillTone::Success,
                            ));
                        }
                        AdapterLink::Connecting => {
                            pills.push(status_pill(None, m.device_connecting, PillTone::Neutral));
                        }
                        AdapterLink::Unreachable => {
                            pills.push(status_pill(
                                Some(GLYPH_WARNING),
                                m.status_off,
                                PillTone::Warning,
                            ));
                        }
                        AdapterLink::Conflict(app) => {
                            pills.push(status_pill(Some(GLYPH_WARNING), app, PillTone::Warning));
                        }
                    }
                    if let Some(battery) = status.battery {
                        let text = if battery.charging {
                            format!("{}% · {}", battery.percent, m.battery_charging)
                        } else {
                            format!("{}%", battery.percent)
                        };
                        let tone = if battery.charging {
                            PillTone::Success
                        } else if battery.percent <= 15 {
                            PillTone::Critical
                        } else if battery.percent <= 25 {
                            PillTone::Warning
                        } else {
                            PillTone::Neutral
                        };
                        pills.push(status_pill(
                            Some(battery_glyph(battery.percent, battery.charging)),
                            &text,
                            tone,
                        ));
                    }
                }
                None if device.transport.is_receiver() => {
                    pills.push(status_pill(None, m.transport_bolt, PillTone::Muted));
                }
                None => {
                    pills.push(status_pill(None, m.kind_default_short, PillTone::Muted));
                }
            }
        }
        if s.mouse.enabled && s.mouse_elevated {
            pills.push(status_pill(
                Some(GLYPH_WARNING),
                m.status_elevated,
                PillTone::Warning,
            ));
        }

        let subtitle: View = if pills.is_empty() {
            TextBlock::new()
                .text(m.hero_generic)
                .font_size(12.0)
                .opacity(0.72)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .into()
        } else {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .keyed_children(
                    pills
                        .into_iter()
                        .enumerate()
                        .map(|(index, pill)| KeyedView::new(index, pill)),
                )
        };

        let title_col = StackPanel::new()
            .grid_column(1)
            .spacing(4.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                TextBlock::new()
                    .text(title)
                    .font_size(20.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_trimming(TextTrimming::CharacterEllipsis),
                subtitle,
            ));

        // Master switch: state label sits to the LEFT of the ToggleSwitch.
        let state_label = if !s.mouse.enabled {
            m.status_off
        } else if s.mouse_elevated {
            m.status_elevated
        } else {
            m.status_on
        };
        let controls = StackPanel::new()
            .grid_column(2)
            .orientation(Orientation::Horizontal)
            .spacing(12.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                TextBlock::new()
                    .text(state_label)
                    .font_size(13.0)
                    .opacity(0.85)
                    .vertical_alignment(VerticalAlignment::Center),
                settings_ui::toggle_switch(
                    ToggleSwitch::new()
                        .min_width(0.0)
                        .is_on(s.mouse.enabled)
                        .on_toggled(
                            context.callback(|on| Message::Toggle(Command::ToggleMouse, on)),
                        ),
                ),
                icon_button(
                    GLYPH_RESET,
                    14.0,
                    34.0,
                    context.message(Message::PowerMouse(PowerMouseAction::ResetDefaults)),
                    m.reset_defaults,
                ),
            ));

        Border::new()
            .padding(Thickness::new(24.0, 16.0, 24.0, 10.0))
            .content(
                Grid::new()
                    .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                    .column_spacing(14.0)
                    .children((badge, title_col, controls)),
            )
    }

    /// Segmented tab strip + live binding counter badge.
    fn mouse_tab_bar(&self, context: &ViewContext<Self>) -> View {
        let m = self.t().mouse;
        let tabs = [
            (MouseTab::Buttons, GLYPH_MOUSE, m.tab_buttons),
            (MouseTab::Scrolling, GLYPH_SPEED, m.tab_scrolling),
            (MouseTab::Ring, GLYPH_RING, m.tab_ring),
        ];
        let tab_buttons: Vec<KeyedView> = tabs
            .into_iter()
            .enumerate()
            .map(|(index, (tab, glyph, label))| {
                let active = self.mouse_tab == tab;
                let btn = Button::new()
                    .style(if active {
                        ButtonStyle::Accent
                    } else {
                        ButtonStyle::Subtle
                    })
                    .on_click(context.message(Message::MouseTab(tab)))
                    .content(
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(7.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                sized_icon(glyph, 13.0),
                                TextBlock::new()
                                    .text(label)
                                    .font_size(12.5)
                                    .font_weight(if active {
                                        FontWeight::SEMI_BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .vertical_alignment(VerticalAlignment::Center),
                            )),
                    );
                KeyedView::new(index, btn)
            })
            .collect();

        let switcher = Border::new()
            .grid_column(0)
            .horizontal_alignment(HorizontalAlignment::Left)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .padding(Thickness::uniform(3.0))
            .content(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(3.0)
                    .keyed_children(tab_buttons),
            );

        let right_pill: Option<View> = match self.mouse_tab {
            MouseTab::Buttons => {
                let custom_bindings = self
                    .snapshot
                    .mouse
                    .bindings
                    .values()
                    .filter(|b| !b.is_pass_through())
                    .count();
                if custom_bindings > 0 {
                    Some(status_pill(
                        None,
                        &m.custom_count
                            .replace("{count}", &custom_bindings.to_string()),
                        PillTone::Neutral,
                    ))
                } else {
                    Some(
                        TextBlock::new()
                            .text(m.diagram_hint)
                            .font_size(12.0)
                            .opacity(0.62)
                            .vertical_alignment(VerticalAlignment::Center)
                            .into(),
                    )
                }
            }
            MouseTab::Scrolling => {
                let dpi = self
                    .snapshot
                    .mouse_devices
                    .first()
                    .and_then(|d| self.snapshot.mouse_adapters.get(&d.key))
                    .and_then(|st| st.dpi);
                dpi.map(|v| status_pill(Some(GLYPH_SPEED), &format!("{v} DPI"), PillTone::Neutral))
            }
            MouseTab::Ring => {
                let filled = Trigger::RING
                    .into_iter()
                    .filter(|slot| !self.snapshot.mouse.binding(*slot).is_pass_through())
                    .count();
                Some(status_pill(
                    Some(GLYPH_RING),
                    &m.ring_slots_filled.replace("{count}", &filled.to_string()),
                    if filled > 0 {
                        PillTone::Success
                    } else {
                        PillTone::Neutral
                    },
                ))
            }
        };

        let mut bar_children: Vec<KeyedView> = vec![KeyedView::new("tabs", switcher)];
        if let Some(pill) = right_pill {
            bar_children.push(KeyedView::new(
                "right",
                StackPanel::new()
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((pill,)),
            ));
        }

        Border::new()
            .padding(Thickness::new(24.0, 2.0, 24.0, 12.0))
            .content(
                Grid::new()
                    .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                    .column_spacing(12.0)
                    .keyed_children(bar_children),
            )
    }

    /// Whether a connected device offers the trigger, or it is already bound.
    fn mouse_offers(&self, trigger: Trigger) -> bool {
        let s = &self.snapshot;
        s.mouse.bindings.contains_key(&trigger)
            || s.mouse_adapters.values().any(|status| {
                status.controls.contains(&trigger)
                    || (trigger.is_thumb_wheel() && status.has_thumb_wheel)
            })
    }

    /// Callouts placed around the mouse render on the Buttons canvas.
    fn mouse_canvas_callouts(&self, has_real_art: bool) -> Vec<CalloutId> {
        let has_thumb = Trigger::THUMB_WHEEL
            .into_iter()
            .any(|trigger| self.mouse_offers(trigger));
        let mut items: Vec<CalloutId> = PAGE_TRIGGERS
            .iter()
            .copied()
            .map(CalloutId::Trigger)
            .collect();
        for trigger in Trigger::DEVICE {
            if self.mouse_offers(trigger) {
                items.push(CalloutId::Trigger(trigger));
            }
        }
        if has_thumb {
            items.push(CalloutId::ThumbWheel);
        } else if !has_real_art {
            // In generic mouse mode (no device thumb wheel), place horizontal
            // scroll right beside the scroll wheel on the diagram.
            items.push(CalloutId::Trigger(Trigger::TiltLeft));
            items.push(CalloutId::Trigger(Trigger::TiltRight));
        }
        items
    }

    /// Tab 1: OpenLogi-style split workspace — interactive mouse stage on the
    /// left, fixed scrollable binding inspector on the right.
    fn mouse_buttons_workspace(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot;
        let m = self.t().mouse;
        let art = self.mouse_lead_art();
        let cached_side = art.and_then(|spec| self.app.mouse_image(spec.depot, ImageKind::Side));
        let has_real_art = art.is_some() && cached_side.is_some();

        let callouts = self.mouse_canvas_callouts(has_real_art);
        let laid_out = diagram::layout(
            &callouts,
            has_real_art.then_some(art).flatten(),
            cached_side.as_ref().map(|img| (img.width, img.height)),
        );

        let mut stage_rows: Vec<View> = Vec::new();
        stage_rows.extend(self.mouse_stage_notices());

        // Interactive mouse canvas centered inside the stage.
        stage_rows.push(
            Border::new()
                .horizontal_alignment(HorizontalAlignment::Center)
                .margin(Thickness::new(0.0, 4.0, 0.0, 4.0))
                .content(self.mouse_stage_canvas(context, &laid_out, cached_side.as_ref())),
        );

        // Secondary triggers bar (horizontal scroll when not on canvas + F13–F24 keys).
        stage_rows.push(self.mouse_extra_triggers_bar(context, &callouts));

        let left_stage = ScrollViewer::new()
            .grid_column(0)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .margin(Thickness::new(16.0, 14.0, 16.0, 24.0))
                    .spacing(12.0)
                    .keyed_children(
                        stage_rows
                            .into_iter()
                            .enumerate()
                            .map(|(index, row)| KeyedView::new(index, row)),
                    ),
            );

        let divider = Border::new()
            .grid_column(1)
            .width(1.0)
            .background(ThemeBrush::CardStroke)
            .opacity(0.70);

        let inspector = ScrollViewer::new()
            .grid_column(2)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .margin(Thickness::new(18.0, 16.0, 18.0, 24.0))
                    .spacing(10.0)
                    .keyed_children(
                        self.mouse_button_inspector(context, self.mouse_trigger)
                            .into_iter()
                            .enumerate()
                            .map(|(index, row)| KeyedView::new(index, row)),
                    ),
            );

        // Subtle surface distinction for the right inspector panel.
        let inspector_shell = Grid::new().grid_column(2).children((
            Border::new()
                .background(ThemeBrush::CardBackground)
                .opacity(0.42),
            inspector,
        ));

        let _ = (s, m);
        Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::Pixel(1.0),
                GridLength::Pixel(INSPECTOR_WIDTH),
            ])
            .children((left_stage, divider, inspector_shell))
    }

    /// Non-intrusive contextual notices shown above the mouse canvas only when
    /// relevant (conflict app, elevated foreground, unverified device).
    fn mouse_stage_notices(&self) -> Vec<View> {
        let s = &self.snapshot;
        let m = self.t().mouse;
        let mut notices = Vec::new();
        if s.mouse.enabled && s.mouse_elevated {
            notices.push(inset_notice(GLYPH_WARNING, m.elevated_hint));
        }
        for device in &s.mouse_devices {
            match s.mouse_adapters.get(&device.key) {
                Some(status) => match status.link {
                    AdapterLink::Conflict(app) => {
                        notices.push(inset_notice(
                            GLYPH_WARNING,
                            &m.device_conflict.replace("{app}", app),
                        ));
                    }
                    AdapterLink::Unreachable => {
                        notices.push(inset_notice(GLYPH_INFO, m.device_unreachable));
                    }
                    AdapterLink::Active | AdapterLink::Connecting => {}
                },
                None if device.transport.is_receiver() => {
                    notices.push(inset_notice(
                        GLYPH_INFO,
                        &format!("{} — {}", device.name, m.device_receiver),
                    ));
                }
                None => {
                    notices.push(inset_notice(
                        GLYPH_INFO,
                        &format!("{} — {}", device.name, m.device_unverified),
                    ));
                }
            }
        }
        notices
    }

    /// The interactive mouse canvas: real device render (or sculpted fallback),
    /// routed 45° leader lines, physical anchor dots and floating callout cards.
    fn mouse_stage_canvas(
        &self,
        context: &ViewContext<Self>,
        laid_out: &diagram::CanvasLayout,
        real_image: Option<&DeviceImage>,
    ) -> View {
        let mut elements: Vec<View> = Vec::new();

        // 1. Center mouse visual: real device PNG or sculpted vector fallback.
        if let Some(img) = real_image {
            elements.push(
                Image::new()
                    .source_data(EncodedImage::new(Arc::clone(&img.bytes)))
                    .stretch(Stretch::Uniform)
                    .width(laid_out.mouse.w)
                    .height(laid_out.mouse.h)
                    .canvas_left(laid_out.mouse.x)
                    .canvas_top(laid_out.mouse.y)
                    .into(),
            );
        } else {
            elements.extend(self.generic_mouse_vector(laid_out.mouse));
        }

        // Partition callouts so the selected leader & anchor paint on top.
        let mut unselected: Vec<&PlacedCallout> = Vec::new();
        let mut selected_callout: Option<&PlacedCallout> = None;
        for callout in &laid_out.callouts {
            if callout.id.matches(self.mouse_trigger) {
                selected_callout = Some(callout);
            } else {
                unselected.push(callout);
            }
        }

        // 2. Unselected leader lines.
        for callout in &unselected {
            let customized = self.callout_is_customized(callout.id);
            elements.extend(leader_segments(&callout.leader, false, customized));
        }
        // 3. Unselected anchor dots.
        for callout in &unselected {
            let customized = self.callout_is_customized(callout.id);
            elements.extend(anchor_dot(callout.anchor, false, customized));
        }
        // 4. Selected leader line + anchor halo on top of all other traces.
        if let Some(callout) = selected_callout {
            elements.extend(leader_segments(&callout.leader, true, true));
            elements.extend(anchor_dot(callout.anchor, true, true));
        }
        // 5. Floating callout cards on both sides.
        for callout in &laid_out.callouts {
            let is_selected = callout.id.matches(self.mouse_trigger);
            elements.push(self.mouse_callout_card(context, callout, is_selected));
        }

        Canvas::new()
            .width(CANVAS_W)
            .height(CANVAS_H)
            .keyed_children(
                elements
                    .into_iter()
                    .enumerate()
                    .map(|(index, view)| KeyedView::new(index, view)),
            )
    }

    /// Sculpted top-down mouse illustration used when no real device render is
    /// available (generic mouse mode or offline before first cache).
    fn generic_mouse_vector(&self, r: MouseRect) -> Vec<View> {
        let mut shapes: Vec<View> = Vec::new();
        let cx = r.x + r.w / 2.0;
        let body_r = r.w * 0.48;

        // Soft ambient base shadow.
        shapes.push(
            Rectangle::new()
                .width(r.w)
                .height(r.h)
                .radius_x(body_r)
                .radius_y(body_r)
                .fill(ThemeBrush::PrimaryText)
                .opacity(0.06)
                .canvas_left(r.x)
                .canvas_top(r.y + 4.0)
                .into(),
        );
        // Sculpted main shell.
        shapes.push(
            Rectangle::new()
                .width(r.w)
                .height(r.h)
                .radius_x(body_r)
                .radius_y(body_r)
                .fill(ThemeBrush::SolidBackground)
                .stroke(ThemeBrush::CardStroke)
                .stroke_thickness(1.5)
                .canvas_left(r.x)
                .canvas_top(r.y)
                .into(),
        );
        // Subtle inner palm contour highlight.
        let inset = 8.0;
        shapes.push(
            Rectangle::new()
                .width(r.w - inset * 2.0)
                .height(r.h - inset * 2.0)
                .radius_x(body_r - inset)
                .radius_y(body_r - inset)
                .fill(ThemeBrush::CardBackground)
                .opacity(0.45)
                .canvas_left(r.x + inset)
                .canvas_top(r.y + inset)
                .into(),
        );
        // Left/right button split and deck seam.
        let deck_y = r.y + r.h * 0.43;
        shapes.push(
            Line::new()
                .x1(cx)
                .y1(r.y + 2.0)
                .x2(cx)
                .y2(deck_y)
                .stroke(ThemeBrush::PrimaryText)
                .stroke_thickness(1.0)
                .opacity(0.26)
                .into(),
        );
        shapes.push(
            Line::new()
                .x1(r.x + 6.0)
                .y1(deck_y)
                .x2(r.x + r.w - 6.0)
                .y2(deck_y)
                .stroke(ThemeBrush::PrimaryText)
                .stroke_thickness(1.0)
                .opacity(0.22)
                .into(),
        );
        // Recessed wheel well + knurled scroll wheel.
        let well_w = 28.0;
        let well_h = 62.0;
        let well_y = r.y + r.h * 0.135;
        shapes.push(
            Rectangle::new()
                .width(well_w)
                .height(well_h)
                .radius_x(14.0)
                .radius_y(14.0)
                .fill(ThemeBrush::PrimaryText)
                .opacity(0.10)
                .canvas_left(cx - well_w / 2.0)
                .canvas_top(well_y)
                .into(),
        );
        let wheel_w = 18.0;
        let wheel_h = 48.0;
        let wheel_y = well_y + (well_h - wheel_h) / 2.0;
        shapes.push(
            Rectangle::new()
                .width(wheel_w)
                .height(wheel_h)
                .radius_x(9.0)
                .radius_y(9.0)
                .fill(ThemeBrush::CardBackground)
                .stroke(ThemeBrush::PrimaryText)
                .stroke_thickness(1.2)
                .opacity(0.85)
                .canvas_left(cx - wheel_w / 2.0)
                .canvas_top(wheel_y)
                .into(),
        );
        for tick in 0..5 {
            let ty = wheel_y + 11.0 + f64::from(tick) * 6.5;
            shapes.push(
                Line::new()
                    .x1(cx - 5.0)
                    .y1(ty)
                    .x2(cx + 5.0)
                    .y2(ty)
                    .stroke(ThemeBrush::PrimaryText)
                    .stroke_thickness(1.0)
                    .opacity(0.32)
                    .into(),
            );
        }
        // Sculpted left-flank Forward and Back buttons.
        for (y_frac, h) in [(0.34, 30.0), (0.49, 30.0)] {
            shapes.push(
                Rectangle::new()
                    .width(7.0)
                    .height(h)
                    .radius_x(3.5)
                    .radius_y(3.5)
                    .fill(ThemeBrush::CardBackground)
                    .stroke(ThemeBrush::PrimaryText)
                    .stroke_thickness(1.0)
                    .opacity(0.55)
                    .canvas_left(r.x - 1.0)
                    .canvas_top(r.y + r.h * y_frac)
                    .into(),
            );
        }
        shapes
    }

    fn callout_is_customized(&self, id: CalloutId) -> bool {
        let mouse = &self.snapshot.mouse;
        match id {
            CalloutId::Trigger(trigger) => !mouse.binding(trigger).is_pass_through(),
            CalloutId::ThumbWheel => {
                !mouse.binding(Trigger::ThumbLeft).is_pass_through()
                    || !mouse.binding(Trigger::ThumbRight).is_pass_through()
            }
        }
    }

    fn callout_label_and_summary(&self, id: CalloutId) -> (&'static str, String, &'static str) {
        let m = self.t().mouse;
        let mouse = &self.snapshot.mouse;
        match id {
            CalloutId::Trigger(trigger) => {
                let binding = mouse.binding(trigger);
                let summary = match &binding {
                    Binding::Gesture { .. } => m.directions_count.to_string(),
                    _ => m.binding_summary(&binding, self.snapshot.meta_key),
                };
                (m.trigger(trigger), summary, binding_glyph(&binding))
            }
            CalloutId::ThumbWheel => {
                let left = mouse.binding(Trigger::ThumbLeft);
                let right = mouse.binding(Trigger::ThumbRight);
                let (summary, glyph) = match (left.is_pass_through(), right.is_pass_through()) {
                    (true, true) => (m.pass_through.to_string(), GLYPH_WHEEL),
                    (false, true) => (
                        m.binding_summary(&left, self.snapshot.meta_key),
                        binding_glyph(&left),
                    ),
                    (true, false) => (
                        m.binding_summary(&right, self.snapshot.meta_key),
                        binding_glyph(&right),
                    ),
                    (false, false) => (
                        format!(
                            "{} / {}",
                            m.binding_summary(&left, self.snapshot.meta_key),
                            m.binding_summary(&right, self.snapshot.meta_key)
                        ),
                        GLYPH_WHEEL,
                    ),
                };
                (m.section_thumb_wheel, summary, glyph)
            }
        }
    }

    /// One OpenLogi-style floating callout card positioned on the Canvas.
    fn mouse_callout_card(
        &self,
        context: &ViewContext<Self>,
        callout: &PlacedCallout,
        selected: bool,
    ) -> View {
        let (title, summary, glyph) = self.callout_label_and_summary(callout.id);
        let customized = self.callout_is_customized(callout.id);
        let target_trigger = callout.id.primary_trigger(self.mouse_trigger);

        let title_row: View = if customized {
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(4.0)
                .children((
                    TextBlock::new()
                        .grid_column(0)
                        .text(title)
                        .font_size(11.0)
                        .opacity(if selected { 0.90 } else { 0.65 })
                        .text_trimming(TextTrimming::CharacterEllipsis),
                    Ellipse::new()
                        .grid_column(1)
                        .width(6.0)
                        .height(6.0)
                        .fill(ThemeBrush::Accent)
                        .vertical_alignment(VerticalAlignment::Center),
                ))
        } else {
            TextBlock::new()
                .text(title)
                .font_size(11.0)
                .opacity(if selected { 0.90 } else { 0.65 })
                .text_trimming(TextTrimming::CharacterEllipsis)
                .into()
        };

        let value_row = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(6.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .opacity(if selected || customized { 0.92 } else { 0.60 })
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((sized_icon(glyph, 12.0),)),
                TextBlock::new()
                    .grid_column(1)
                    .text(summary.clone())
                    .font_size(12.0)
                    .font_weight(if selected || customized {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .opacity(if selected || customized { 0.96 } else { 0.72 })
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    .vertical_alignment(VerticalAlignment::Center),
                StackPanel::new()
                    .grid_column(2)
                    .opacity(if selected { 0.80 } else { 0.38 })
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((sized_icon(GLYPH_CHEVRON_RIGHT, 9.0),)),
            ));

        let card_content = StackPanel::new()
            .spacing(2.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((title_row, value_row));

        let frame = Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(if selected {
                ThemeBrush::Accent
            } else {
                ThemeBrush::CardStroke
            })
            .border_thickness(Thickness::uniform(if selected { 1.5 } else { 1.0 }))
            .corner_radius(CornerRadius::uniform(8.0));

        let wash = Border::new()
            .background(ThemeBrush::Accent)
            .opacity(if selected { 0.11 } else { 0.0 })
            .corner_radius(CornerRadius::uniform(8.0));

        let hit = Button::new()
            .style(ButtonStyle::Subtle)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Stretch)
            .vertical_content_alignment(VerticalAlignment::Center)
            .on_click(context.message(Message::MouseSelect(target_trigger)))
            .content(card_content)
            .tooltip(format!("{title}: {summary}"));

        Grid::new()
            .width(CARD_W)
            .height(CARD_H)
            .canvas_left(callout.card_x)
            .canvas_top(callout.card_y)
            .children((frame, wash, hit))
    }

    /// Compact bottom bar under the mouse canvas for triggers not drawn on the
    /// mouse silhouette (e.g. horizontal scroll when MX Master 4 shows thumb
    /// wheel instead, plus F13–F24 extra keys).
    fn mouse_extra_triggers_bar(
        &self,
        context: &ViewContext<Self>,
        on_canvas: &[CalloutId],
    ) -> View {
        let m = self.t().mouse;
        let mut chips: Vec<View> = Vec::new();

        for &trigger in TILT_TRIGGERS {
            if !on_canvas.contains(&CalloutId::Trigger(trigger)) {
                chips.push(self.mouse_trigger_chip(context, trigger));
            }
        }
        let mut fkeys: Vec<Trigger> = self
            .snapshot
            .mouse
            .bindings
            .keys()
            .copied()
            .filter(|t| t.is_function_key())
            .collect();
        fkeys.sort();
        for trigger in fkeys {
            chips.push(self.mouse_trigger_chip(context, trigger));
        }

        let capturing = self.snapshot.capturing == Some(CaptureTarget::MouseAddFKey);
        chips.push(
            Button::new()
                .style(if capturing {
                    ButtonStyle::Accent
                } else {
                    ButtonStyle::Subtle
                })
                .on_click(context.message(Message::PowerMouse(PowerMouseAction::BeginAddFKey)))
                .content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(6.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            sized_icon(GLYPH_ADD, 11.0),
                            TextBlock::new()
                                .text(if capturing {
                                    m.capturing_fkey
                                } else {
                                    m.add_extra_key
                                })
                                .font_size(12.0)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                ),
        );

        let header = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(8.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .opacity(0.65)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((sized_icon(GLYPH_KEYBOARD, 13.0),)),
                TextBlock::new()
                    .grid_column(1)
                    .text(m.other_triggers)
                    .font_size(12.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .opacity(0.85)
                    .vertical_alignment(VerticalAlignment::Center),
            ));

        // Wrap chips cleanly into rows of at most 3 so nothing overflows horizontally.
        let mut rows: Vec<View> = vec![header];
        for chunk in chips.chunks(3) {
            rows.push(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .keyed_children(
                        chunk
                            .iter()
                            .cloned()
                            .enumerate()
                            .map(|(index, chip)| KeyedView::new(index, chip)),
                    ),
            );
        }

        settings_ui::surface(settings_ui::stack(rows, 8.0))
    }

    fn mouse_trigger_chip(&self, context: &ViewContext<Self>, trigger: Trigger) -> View {
        let m = self.t().mouse;
        let selected = self.mouse_trigger == trigger;
        let binding = self.snapshot.mouse.binding(trigger);
        let customized = !binding.is_pass_through();
        let label = if customized {
            format!(
                "{} · {}",
                m.trigger(trigger),
                m.binding_summary(&binding, self.snapshot.meta_key)
            )
        } else {
            m.trigger(trigger).to_string()
        };
        Button::new()
            .style(if selected {
                ButtonStyle::Accent
            } else {
                ButtonStyle::Subtle
            })
            .on_click(context.message(Message::MouseSelect(trigger)))
            .content(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        sized_icon(trigger_glyph(trigger), 11.5),
                        TextBlock::new()
                            .text(label)
                            .font_size(12.0)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .vertical_alignment(VerticalAlignment::Center),
                    )),
            )
    }

    /// Fixed right-side binding inspector for the selected trigger.
    fn mouse_button_inspector(&self, context: &ViewContext<Self>, trigger: Trigger) -> Vec<View> {
        let m = self.t().mouse;
        let current = self.snapshot.mouse.binding(trigger);
        let is_bound = !current.is_pass_through();

        let subtitle = match &current {
            Binding::Gesture { .. } => m.directions_count.to_string(),
            _ => m.binding_summary(&current, self.snapshot.meta_key),
        };

        // Header: icon badge + title + subtitle + optional quick Clear button.
        let title_col = StackPanel::new()
            .grid_column(1)
            .spacing(2.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                TextBlock::new()
                    .text(if trigger.is_thumb_wheel() {
                        m.section_thumb_wheel
                    } else {
                        m.trigger(trigger)
                    })
                    .font_size(17.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(subtitle)
                    .font_size(12.0)
                    .opacity(0.68)
                    .text_trimming(TextTrimming::CharacterEllipsis),
            ));

        let mut header_children: Vec<KeyedView> = vec![
            KeyedView::new(
                "badge",
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((drawer_icon_badge(trigger_glyph(trigger)),)),
            ),
            KeyedView::new("title", title_col),
        ];
        if is_bound {
            header_children.push(KeyedView::new(
                "clear",
                Button::new()
                    .grid_column(2)
                    .style(ButtonStyle::Subtle)
                    .vertical_alignment(VerticalAlignment::Center)
                    .on_click(
                        context.message(Message::PowerMouse(PowerMouseAction::SetBinding {
                            trigger,
                            binding: Binding::PassThrough,
                        })),
                    )
                    .content(TextBlock::new().text(m.clear_binding).font_size(12.0)),
            ));
        }

        let mut body: Vec<View> = vec![
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(10.0)
                .keyed_children(header_children),
            hairline(),
        ];

        // Dual-direction switcher when inspecting the physical thumb wheel.
        if trigger.is_thumb_wheel() {
            body.push(self.mouse_thumb_direction_switcher(context, trigger));
        }

        // OpenLogi-inspired mode pill selector.
        body.push(self.mouse_kind_selector(context, trigger, &current));

        // Mode-specific editor surface.
        match &current {
            Binding::PassThrough => {
                body.push(inset_notice(GLYPH_INFO, m.pass_through_note));
            }
            Binding::Single(action) | Binding::Hold(action) => {
                body.push(self.mouse_action_slot(
                    context,
                    trigger,
                    current.clone(),
                    ActionSlot::Single,
                    m.slot_action,
                    action,
                ));
                if matches!(current, Binding::Hold(_)) {
                    body.push(inset_notice(GLYPH_INFO, m.hold_hint));
                }
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
                body.push(inset_notice(GLYPH_CLOCK, m.long_press_hint));
            }
            Binding::Gesture {
                click,
                up,
                down,
                left,
                right,
            } => {
                body.extend(self.mouse_gesture_inspector(
                    context, trigger, &current, click, up, down, left, right,
                ));
            }
        }

        // Trigger-specific note or remove button.
        if trigger.is_device_control() {
            body.push(inset_notice(GLYPH_INFO, m.device_buttons_hint));
        } else if trigger.is_thumb_wheel() {
            body.push(inset_notice(GLYPH_INFO, m.thumb_wheel_hint));
        } else if TILT_TRIGGERS.contains(&trigger) {
            body.push(inset_notice(GLYPH_INFO, m.tilt_hint));
        } else if trigger.is_function_key() {
            body.push(
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .on_click(
                        context.message(Message::PowerMouse(PowerMouseAction::RemoveFKey {
                            trigger,
                        })),
                    )
                    .content(
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(6.0)
                            .children((
                                sized_icon(GLYPH_DELETE, 12.0),
                                TextBlock::new().text(m.remove_key).font_size(12.5),
                            )),
                    ),
            );
            body.push(inset_notice(GLYPH_KEYBOARD, m.fkey_hint));
        }

        body
    }

    /// 2-segment switcher between Left and Right rotation of the Thumb Wheel.
    fn mouse_thumb_direction_switcher(
        &self,
        context: &ViewContext<Self>,
        selected: Trigger,
    ) -> View {
        let m = self.t().mouse;
        let side_btn = |col: i32, trigger: Trigger, glyph: &str, label: &str| {
            let active = selected == trigger;
            let bound = !self.snapshot.mouse.binding(trigger).is_pass_through();
            let text = if bound {
                format!("{label} •")
            } else {
                label.to_string()
            };
            Button::new()
                .grid_column(col)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .style(if active {
                    ButtonStyle::Accent
                } else {
                    ButtonStyle::Subtle
                })
                .on_click(context.message(Message::MouseSelect(trigger)))
                .content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(6.0)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            sized_icon(glyph, 11.5),
                            TextBlock::new()
                                .text(text)
                                .font_size(12.0)
                                .font_weight(if active {
                                    FontWeight::SEMI_BOLD
                                } else {
                                    FontWeight::NORMAL
                                }),
                        )),
                )
        };
        Border::new()
            .background(ThemeBrush::SolidBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(CornerRadius::uniform(8.0))
            .padding(Thickness::uniform(3.0))
            .content(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::STAR])
                    .column_spacing(3.0)
                    .children((
                        side_btn(0, Trigger::ThumbLeft, GLYPH_LEFT, m.thumb_left_short),
                        side_btn(1, Trigger::ThumbRight, GLYPH_RIGHT, m.thumb_right_short),
                    )),
            )
    }

    /// Clean segmented binding-type selector inside the right inspector.
    fn mouse_kind_selector(
        &self,
        context: &ViewContext<Self>,
        trigger: Trigger,
        current: &Binding,
    ) -> View {
        let m = self.t().mouse;
        let kinds = available_kinds(trigger);
        let active_kind = binding_kind(current);

        let make_pill = |kind: BindingKind| -> View {
            let active = kind == active_kind;
            let next_binding = coerce_binding(current, kind);
            Button::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .style(if active {
                    ButtonStyle::Accent
                } else {
                    ButtonStyle::Subtle
                })
                .on_click(context.message(if active {
                    Message::Noop
                } else {
                    Message::PowerMouse(PowerMouseAction::SetBinding {
                        trigger,
                        binding: next_binding,
                    })
                }))
                .content(
                    TextBlock::new()
                        .text(kind.short_label(m))
                        .font_size(12.0)
                        .font_weight(if active {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .text_trimming(TextTrimming::CharacterEllipsis),
                )
        };

        // 2-column or 3-column grid so labels stay readable inside 328 DIP.
        let grid: View = if kinds.len() <= 3 {
            let cols: Vec<GridLength> = (0..kinds.len()).map(|_| GridLength::STAR).collect();
            let cells: Vec<KeyedView> = kinds
                .iter()
                .enumerate()
                .map(|(i, &k)| {
                    KeyedView::new(
                        i,
                        StackPanel::new()
                            .grid_column(i as i32)
                            .children((make_pill(k),)),
                    )
                })
                .collect();
            Grid::new()
                .columns(cols)
                .column_spacing(3.0)
                .keyed_children(cells)
        } else {
            // Top row: 3 pills (Default, Single, Hold); Bottom row: Long press, Gesture.
            let split = 3.min(kinds.len());
            let row_view = |slice: &[BindingKind]| -> View {
                let cols: Vec<GridLength> = (0..slice.len()).map(|_| GridLength::STAR).collect();
                let cells: Vec<KeyedView> = slice
                    .iter()
                    .enumerate()
                    .map(|(i, &k)| {
                        KeyedView::new(
                            i,
                            StackPanel::new()
                                .grid_column(i as i32)
                                .children((make_pill(k),)),
                        )
                    })
                    .collect();
                Grid::new()
                    .columns(cols)
                    .column_spacing(3.0)
                    .keyed_children(cells)
            };
            settings_ui::stack(
                vec![row_view(&kinds[..split]), row_view(&kinds[split..])],
                3.0,
            )
        };

        settings_ui::stack(
            vec![
                TextBlock::new()
                    .text(m.binding_kind)
                    .font_size(12.0)
                    .opacity(0.70)
                    .into(),
                Border::new()
                    .background(ThemeBrush::SolidBackground)
                    .border_brush(ThemeBrush::CardStroke)
                    .border_thickness(Thickness::uniform(1.0))
                    .corner_radius(CornerRadius::uniform(8.0))
                    .padding(Thickness::uniform(3.0))
                    .content(grid),
            ],
            5.0,
        )
    }

    /// OpenLogi-style 5-direction gesture list + focused action editor for the
    /// currently chosen direction.
    #[allow(clippy::too_many_arguments)]
    fn mouse_gesture_inspector(
        &self,
        context: &ViewContext<Self>,
        trigger: Trigger,
        current: &Binding,
        click: &Action,
        up: &Action,
        down: &Action,
        left: &Action,
        right: &Action,
    ) -> Vec<View> {
        let m = self.t().mouse;
        let active_slot = match self.mouse_gesture_slot {
            ActionSlot::Up
            | ActionSlot::Down
            | ActionSlot::Left
            | ActionSlot::Right
            | ActionSlot::Click => self.mouse_gesture_slot,
            _ => ActionSlot::Click,
        };
        let directions: [(ActionSlot, &str, &str, &Action); 5] = [
            (ActionSlot::Up, GLYPH_UP, m.slot_up, up),
            (ActionSlot::Down, GLYPH_DOWN, m.slot_down, down),
            (ActionSlot::Left, GLYPH_LEFT, m.slot_left, left),
            (ActionSlot::Right, GLYPH_RIGHT, m.slot_right, right),
            (ActionSlot::Click, GLYPH_CLICK, m.slot_click, click),
        ];

        let mut dir_rows: Vec<View> = Vec::with_capacity(5);
        for (slot, dir_glyph, dir_label, action) in directions {
            let selected = slot == active_slot;
            let summary = m.action_label(action, self.snapshot.meta_key);

            let row_content = Grid::new()
                .columns([
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::STAR,
                    GridLength::Auto,
                ])
                .column_spacing(10.0)
                .children((
                    accent_bar(
                        24.0,
                        if selected {
                            ThemeBrush::Accent.into()
                        } else {
                            Color::argb(0, 0, 0, 0).into()
                        },
                    )
                    .grid_column(0),
                    StackPanel::new()
                        .grid_column(1)
                        .opacity(if selected { 0.95 } else { 0.65 })
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((sized_icon(dir_glyph, 14.0),)),
                    StackPanel::new()
                        .grid_column(2)
                        .spacing(1.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            TextBlock::new()
                                .text(dir_label)
                                .font_size(12.5)
                                .font_weight(if selected {
                                    FontWeight::SEMI_BOLD
                                } else {
                                    FontWeight::NORMAL
                                }),
                            TextBlock::new()
                                .text(summary)
                                .font_size(11.5)
                                .opacity(0.68)
                                .text_trimming(TextTrimming::CharacterEllipsis),
                        )),
                    StackPanel::new()
                        .grid_column(3)
                        .opacity(if selected { 0.90 } else { 0.0 })
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((sized_icon(GLYPH_CHECK, 12.0),)),
                ));

            let wash = Border::new()
                .background(ThemeBrush::Accent)
                .opacity(if selected { 0.12 } else { 0.0 })
                .corner_radius(CornerRadius::uniform(6.0));
            let hit = Button::new()
                .style(ButtonStyle::Subtle)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .horizontal_content_alignment(HorizontalAlignment::Stretch)
                .on_click(context.message(Message::MouseGestureSlot(slot)))
                .content(row_content);

            dir_rows.push(Grid::new().children((wash, hit)));
        }

        let active_action = match active_slot {
            ActionSlot::Up => up,
            ActionSlot::Down => down,
            ActionSlot::Left => left,
            ActionSlot::Right => right,
            _ => click,
        };

        vec![
            TextBlock::new()
                .text(m.direction_heading)
                .font_size(12.0)
                .opacity(0.70)
                .into(),
            Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::uniform(4.0))
                .content(settings_ui::stack(dir_rows, 2.0)),
            self.mouse_action_slot(
                context,
                trigger,
                current.clone(),
                active_slot,
                m.current_action,
                active_action,
            ),
            inset_notice(GLYPH_GESTURE, m.gesture_hint),
        ]
    }

    /// Tab 2: OpenLogi-inspired Pointer & Scrolling dashboard.
    fn mouse_scrolling_workspace(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot;
        let m = self.t().mouse;
        let mut rows: Vec<View> = Vec::new();

        let active = s
            .mouse_adapters
            .values()
            .find(|status| status.link == AdapterLink::Active);

        if let Some(status) = active {
            let dpi_card =
                (!status.dpi_presets.is_empty()).then(|| self.mouse_dpi_card(context, status));
            let wheel_card = (status.has_wheel_mode || status.has_hires_wheel)
                .then(|| self.mouse_wheel_card(context, status));
            match (dpi_card, wheel_card) {
                (Some(left), Some(right)) => {
                    rows.push(
                        Grid::new()
                            .columns([GridLength::STAR, GridLength::STAR])
                            .column_spacing(14.0)
                            .children((
                                StackPanel::new().grid_column(0).children((left,)),
                                StackPanel::new().grid_column(1).children((right,)),
                            )),
                    );
                }
                (Some(one), None) | (None, Some(one)) => rows.push(one),
                (None, None) => {}
            }
        }

        if active.is_none() {
            rows.push(inset_notice(GLYPH_SPEED, m.device_settings_absent));
        }
        // PowerMouse offers only what Windows Settings lacks; scroll
        // direction, pointer speed and lines per notch are set there.
        rows.push(inset_notice(GLYPH_MOUSE, m.windows_mouse_settings));
        if let Ok(link) = HyperlinkButton::new().navigate_uri("ms-settings:mousetouchpad") {
            rows.push(
                link.horizontal_alignment(HorizontalAlignment::Left)
                    .content(m.open_windows_mouse_settings),
            );
        }

        ScrollViewer::new()
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .margin(Thickness::new(24.0, 18.0, 24.0, 28.0))
                    .spacing(14.0)
                    .keyed_children(
                        rows.into_iter()
                            .enumerate()
                            .map(|(index, row)| KeyedView::new(index, row)),
                    ),
            )
    }

    /// OpenLogi-inspired Pointer Sensitivity (DPI) card with large live readout,
    /// interactive slider and one-click preset chips.
    fn mouse_dpi_card(&self, context: &ViewContext<Self>, status: &AdapterStatus) -> View {
        let m = self.t().mouse;
        let prefs = &self.snapshot.mouse;
        let presets = status.dpi_presets.clone();
        let displayed_dpi = prefs.dpi.or(status.dpi).unwrap_or(presets[0]);
        let slider_index = presets
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.abs_diff(displayed_dpi))
            .map_or(0, |(idx, _)| idx);

        let header = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((sized_icon(GLYPH_SPEED, 15.0),)),
                TextBlock::new()
                    .grid_column(1)
                    .text(m.pointer_card_title)
                    .font_size(14.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center),
                TextBlock::new()
                    .grid_column(2)
                    .text(format!("{displayed_dpi} DPI"))
                    .font_size(18.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center),
            ));

        let slider_presets = presets.clone();
        let current_managed = prefs.dpi;
        let slider = Slider::new()
            .minimum(0.0)
            .maximum((presets.len().saturating_sub(1)) as f64)
            .step_frequency(1.0)
            .value(slider_index as f64)
            .on_value_changed(context.callback(move |val: f64| {
                let idx = val
                    .round()
                    .clamp(0.0, (slider_presets.len().saturating_sub(1)) as f64)
                    as usize;
                let Some(&dpi) = slider_presets.get(idx) else {
                    return Message::Noop;
                };
                // Ignore programmatic initial sync if the value matches what is
                // already displayed, so an unmanaged device is never written to.
                if dpi == displayed_dpi && current_managed.is_none() {
                    return Message::Noop;
                }
                if current_managed == Some(dpi) {
                    return Message::Noop;
                }
                Message::PowerMouse(PowerMouseAction::SetDpi(Some(dpi)))
            }));

        let min_dpi = presets.first().copied().unwrap_or(400);
        let max_dpi = presets.last().copied().unwrap_or(4000);
        let range_line = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                TextBlock::new()
                    .grid_column(0)
                    .text(format!("{min_dpi} – {max_dpi} DPI"))
                    .font_size(11.5)
                    .opacity(0.65),
                TextBlock::new()
                    .grid_column(1)
                    .text(if prefs.dpi.is_none() {
                        m.dpi_device_managed
                    } else {
                        ""
                    })
                    .font_size(11.5)
                    .opacity(0.65),
            ));

        // Preset chips in clean 4-column rows.
        let mut preset_rows: Vec<View> = Vec::new();
        for chunk in presets.chunks(4) {
            let cols: Vec<GridLength> = (0..chunk.len()).map(|_| GridLength::STAR).collect();
            let cells: Vec<KeyedView> =
                chunk
                    .iter()
                    .enumerate()
                    .map(|(i, &dpi)| {
                        let active = prefs.dpi == Some(dpi);
                        KeyedView::new(
                            i,
                            Button::new()
                                .grid_column(i as i32)
                                .horizontal_alignment(HorizontalAlignment::Stretch)
                                .style(if active {
                                    ButtonStyle::Accent
                                } else {
                                    ButtonStyle::Subtle
                                })
                                .on_click(context.message(Message::PowerMouse(
                                    PowerMouseAction::SetDpi(Some(dpi)),
                                )))
                                .content(
                                    TextBlock::new()
                                        .text(dpi.to_string())
                                        .font_size(12.0)
                                        .font_weight(if active {
                                            FontWeight::SEMI_BOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .horizontal_alignment(HorizontalAlignment::Center),
                                ),
                        )
                    })
                    .collect();
            preset_rows.push(
                Grid::new()
                    .columns(cols)
                    .column_spacing(6.0)
                    .keyed_children(cells),
            );
        }

        let mut body = vec![
            header,
            slider.into(),
            range_line,
            TextBlock::new()
                .text(m.dpi_presets_label)
                .font_size(12.0)
                .opacity(0.70)
                .margin(Thickness::new(0.0, 4.0, 0.0, 0.0))
                .into(),
        ];
        body.extend(preset_rows);
        if prefs.dpi.is_some() {
            body.push(
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .on_click(context.message(Message::PowerMouse(PowerMouseAction::SetDpi(None))))
                    .content(TextBlock::new().text(m.device_keep).font_size(12.0)),
            );
        }

        settings_ui::surface(settings_ui::stack(body, 8.0))
    }

    /// OpenLogi-inspired SmartShift & Wheel Resolution card.
    fn mouse_wheel_card(&self, context: &ViewContext<Self>, status: &AdapterStatus) -> View {
        let m = self.t().mouse;
        let prefs = &self.snapshot.mouse;
        let mut body: Vec<View> = vec![
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    sized_icon(GLYPH_WHEEL, 15.0),
                    TextBlock::new()
                        .text(m.smartshift_card_title)
                        .font_size(14.5)
                        .font_weight(FontWeight::SEMI_BOLD),
                )),
        ];

        if status.has_wheel_mode {
            let modes: [(Option<WheelMode>, &str); 4] = [
                (None, m.wheel_mode_device_short),
                (Some(WheelMode::Ratchet), m.wheel_ratchet),
                (Some(WheelMode::FreeSpin), m.wheel_free_spin),
                (Some(WheelMode::SmartShift), m.wheel_mode_smart_short),
            ];
            let cells: Vec<KeyedView> =
                modes
                    .into_iter()
                    .enumerate()
                    .map(|(i, (mode, label))| {
                        let active = prefs.wheel_mode == mode;
                        KeyedView::new(
                            i,
                            Button::new()
                                .grid_column(i as i32)
                                .horizontal_alignment(HorizontalAlignment::Stretch)
                                .style(if active {
                                    ButtonStyle::Accent
                                } else {
                                    ButtonStyle::Subtle
                                })
                                .on_click(context.message(Message::PowerMouse(
                                    PowerMouseAction::SetWheelMode(mode),
                                )))
                                .content(
                                    TextBlock::new()
                                        .text(label)
                                        .font_size(11.5)
                                        .font_weight(if active {
                                            FontWeight::SEMI_BOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .horizontal_alignment(HorizontalAlignment::Center)
                                        .text_trimming(TextTrimming::CharacterEllipsis),
                                ),
                        )
                    })
                    .collect();

            let segmented = Border::new()
                .background(ThemeBrush::SolidBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(CornerRadius::uniform(8.0))
                .padding(Thickness::uniform(3.0))
                .content(
                    Grid::new()
                        .columns([
                            GridLength::STAR,
                            GridLength::STAR,
                            GridLength::STAR,
                            GridLength::STAR,
                        ])
                        .column_spacing(3.0)
                        .keyed_children(cells),
                );

            let mode_note = match prefs.wheel_mode {
                None => m.device_keep,
                Some(WheelMode::Ratchet) => m.wheel_ratchet,
                Some(WheelMode::FreeSpin) => m.wheel_free_spin,
                Some(WheelMode::SmartShift) => m.wheel_smart_shift,
            };

            body.push(
                TextBlock::new()
                    .text(m.wheel_mode_label)
                    .font_size(12.0)
                    .opacity(0.70)
                    .into(),
            );
            body.push(segmented);
            body.push(
                TextBlock::new()
                    .text(mode_note)
                    .font_size(11.5)
                    .opacity(0.65)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            );
        }

        if status.has_hires_wheel {
            if status.has_wheel_mode {
                body.push(hairline());
            }
            body.push(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto])
                    .column_spacing(12.0)
                    .children((
                        StackPanel::new()
                            .grid_column(0)
                            .spacing(2.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                TextBlock::new()
                                    .text(m.hires_wheel)
                                    .font_size(13.5)
                                    .text_wrapping(TextWrapping::Wrap),
                                TextBlock::new()
                                    .text(m.hires_wheel_hint)
                                    .font_size(11.5)
                                    .opacity(0.68)
                                    .text_wrapping(TextWrapping::Wrap),
                            )),
                        StackPanel::new()
                            .grid_column(1)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((settings_ui::toggle_switch(
                                ToggleSwitch::new()
                                    .min_width(0.0)
                                    .is_on(prefs.hires_wheel.unwrap_or(false))
                                    .on_toggled(context.callback(|on| {
                                        Message::PowerMouse(PowerMouseAction::SetHiresWheel(on))
                                    })),
                            ),)),
                    )),
            );
        }

        settings_ui::surface(settings_ui::stack(body, 8.0))
    }

    /// Tab 3: OpenLogi-inspired radial Actions Ring canvas + direct slot inspector.
    fn mouse_ring_workspace(&self, context: &ViewContext<Self>) -> View {
        let s = &self.snapshot;
        let m = self.t().mouse;

        let openers: Vec<&str> = s
            .mouse
            .bindings
            .iter()
            .filter(|(_, binding)| {
                [
                    ActionSlot::Single,
                    ActionSlot::Short,
                    ActionSlot::Long,
                    ActionSlot::Click,
                    ActionSlot::Up,
                    ActionSlot::Down,
                    ActionSlot::Left,
                    ActionSlot::Right,
                ]
                .iter()
                .any(|slot| binding.slot(*slot).is_some_and(Action::opens_ring))
            })
            .map(|(trigger, _)| m.trigger(*trigger))
            .collect();

        let filled_count = Trigger::RING
            .into_iter()
            .filter(|slot| !s.mouse.binding(*slot).is_pass_through())
            .count();

        let centre = RING_CANVAS / 2.0;
        let node_radius = 122.0;
        let outer_ring = 328.0;
        let inner_guide = 210.0;
        let hub_size = 78.0;

        let mut shapes: Vec<View> = vec![
            // Outer circular disc backdrop.
            Ellipse::new()
                .width(outer_ring)
                .height(outer_ring)
                .fill(ThemeBrush::CardBackground)
                .stroke(ThemeBrush::CardStroke)
                .stroke_thickness(1.2)
                .canvas_left(centre - outer_ring / 2.0)
                .canvas_top(centre - outer_ring / 2.0)
                .into(),
            // Subtle inner guide ring.
            Ellipse::new()
                .width(inner_guide)
                .height(inner_guide)
                .stroke(ThemeBrush::CardStroke)
                .stroke_thickness(1.0)
                .opacity(0.45)
                .canvas_left(centre - inner_guide / 2.0)
                .canvas_top(centre - inner_guide / 2.0)
                .into(),
        ];

        // Radial guide spokes from center hub to each slot.
        for (index, slot) in Trigger::RING.into_iter().enumerate() {
            let angle = (index as f64 * 45.0).to_radians();
            let selected = self.mouse_ring_slot == slot;
            let r0 = hub_size / 2.0 + 4.0;
            let r1 = node_radius - RING_NODE / 2.0 - 4.0;
            shapes.push(
                Line::new()
                    .x1(centre + r0 * angle.sin())
                    .y1(centre - r0 * angle.cos())
                    .x2(centre + r1 * angle.sin())
                    .y2(centre - r1 * angle.cos())
                    .stroke(if selected {
                        ThemeBrush::Accent
                    } else {
                        ThemeBrush::CardStroke
                    })
                    .stroke_thickness(if selected { 2.0 } else { 1.0 })
                    .opacity(if selected { 0.90 } else { 0.45 })
                    .into(),
            );
        }

        // Center hub displaying filled slot counter.
        shapes.push(
            Border::new()
                .width(hub_size)
                .height(hub_size)
                .corner_radius(CornerRadius::uniform(hub_size / 2.0))
                .background(ThemeBrush::SolidBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .canvas_left(centre - hub_size / 2.0)
                .canvas_top(centre - hub_size / 2.0)
                .content(
                    StackPanel::new()
                        .spacing(3.0)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            StackPanel::new()
                                .opacity(0.75)
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .children((sized_icon(GLYPH_RING, 16.0),)),
                            TextBlock::new()
                                .text(format!("{filled_count}/8"))
                                .font_size(11.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .opacity(0.80)
                                .horizontal_alignment(HorizontalAlignment::Center),
                        )),
                ),
        );

        // 8 circular slot nodes around the ring.
        for (index, slot) in Trigger::RING.into_iter().enumerate() {
            let angle = (index as f64 * 45.0).to_radians();
            let nx = centre + node_radius * angle.sin() - RING_NODE / 2.0;
            let ny = centre - node_radius * angle.cos() - RING_NODE / 2.0;
            let binding = s.mouse.binding(slot);
            let filled = !binding.is_pass_through();
            let selected = self.mouse_ring_slot == slot;

            if selected {
                let halo = RING_NODE + 10.0;
                shapes.push(
                    Ellipse::new()
                        .width(halo)
                        .height(halo)
                        .fill(ThemeBrush::Accent)
                        .opacity(0.18)
                        .canvas_left(nx - 5.0)
                        .canvas_top(ny - 5.0)
                        .into(),
                );
            }

            let short_label = if filled {
                m.binding_summary(&binding, self.snapshot.meta_key)
            } else {
                format!("{}", index + 1)
            };

            let node_face: View = if filled {
                StackPanel::new()
                    .spacing(3.0)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        sized_icon(binding_glyph(&binding), 17.0),
                        TextBlock::new()
                            .text(short_label.clone())
                            .font_size(9.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .opacity(0.82)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .horizontal_alignment(HorizontalAlignment::Center),
                    ))
            } else {
                StackPanel::new()
                    .spacing(1.0)
                    .opacity(if selected { 0.90 } else { 0.48 })
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        sized_icon(GLYPH_ADD, 13.0),
                        TextBlock::new()
                            .text(short_label.clone())
                            .font_size(10.0)
                            .horizontal_alignment(HorizontalAlignment::Center),
                    ))
            };

            let node_bg = Border::new()
                .background(ThemeBrush::SolidBackground)
                .border_brush(if selected {
                    ThemeBrush::Accent
                } else {
                    ThemeBrush::CardStroke
                })
                .border_thickness(Thickness::uniform(if selected { 2.0 } else { 1.0 }))
                .corner_radius(CornerRadius::uniform(RING_NODE / 2.0));
            let node_wash = Border::new()
                .background(ThemeBrush::Accent)
                .opacity(if selected { 0.14 } else { 0.0 })
                .corner_radius(CornerRadius::uniform(RING_NODE / 2.0));
            let node_btn = Button::new()
                .style(ButtonStyle::Subtle)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .vertical_alignment(VerticalAlignment::Stretch)
                .horizontal_content_alignment(HorizontalAlignment::Center)
                .vertical_content_alignment(VerticalAlignment::Center)
                .on_click(context.message(Message::MouseSelect(slot)))
                .content(node_face)
                .tooltip(format!(
                    "{}: {}",
                    m.trigger(slot),
                    if filled {
                        m.binding_summary(&binding, self.snapshot.meta_key)
                    } else {
                        m.ring_empty_slot.to_string()
                    }
                ));

            shapes.push(
                Grid::new()
                    .width(RING_NODE)
                    .height(RING_NODE)
                    .canvas_left(nx)
                    .canvas_top(ny)
                    .children((node_bg, node_wash, node_btn)),
            );
        }

        let opener_banner = if openers.is_empty() {
            inset_notice(GLYPH_WARNING, m.ring_no_opener)
        } else {
            inset_notice(
                GLYPH_CHECK,
                &m.ring_opened_by.replace("{list}", &openers.join(", ")),
            )
        };

        let left_stage = ScrollViewer::new()
            .grid_column(0)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .margin(Thickness::new(20.0, 14.0, 20.0, 24.0))
                    .spacing(12.0)
                    .children((
                        Canvas::new()
                            .width(RING_CANVAS)
                            .height(RING_CANVAS)
                            .horizontal_alignment(HorizontalAlignment::Center)
                            .keyed_children(
                                shapes
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, view)| KeyedView::new(index, view)),
                            ),
                        opener_banner,
                        inset_notice(GLYPH_INFO, m.ring_hint),
                    )),
            );

        let divider = Border::new()
            .grid_column(1)
            .width(1.0)
            .background(ThemeBrush::CardStroke)
            .opacity(0.70);

        let inspector = ScrollViewer::new()
            .grid_column(2)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(
                StackPanel::new()
                    .margin(Thickness::new(18.0, 16.0, 18.0, 24.0))
                    .spacing(10.0)
                    .keyed_children(
                        self.mouse_ring_inspector(context, self.mouse_ring_slot)
                            .into_iter()
                            .enumerate()
                            .map(|(index, row)| KeyedView::new(index, row)),
                    ),
            );

        let inspector_shell = Grid::new().grid_column(2).children((
            Border::new()
                .background(ThemeBrush::CardBackground)
                .opacity(0.42),
            inspector,
        ));

        Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::Pixel(1.0),
                GridLength::Pixel(INSPECTOR_WIDTH),
            ])
            .children((left_stage, divider, inspector_shell))
    }

    /// Direct right-side inspector for one Actions Ring slot: picking an action
    /// immediately assigns it, and "Clear slot" resets it to empty.
    fn mouse_ring_inspector(&self, context: &ViewContext<Self>, slot: Trigger) -> Vec<View> {
        let m = self.t().mouse;
        let current = self.snapshot.mouse.binding(slot);
        let filled = !current.is_pass_through();
        let slot_number = Trigger::RING
            .iter()
            .position(|each| *each == slot)
            .map_or(1, |idx| idx + 1);

        let badge = Border::new()
            .width(36.0)
            .height(36.0)
            .corner_radius(CornerRadius::uniform(18.0))
            .background(if filled {
                ThemeBrush::Accent
            } else {
                ThemeBrush::CardBackground
            })
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .content(
                TextBlock::new()
                    .text(slot_number.to_string())
                    .font_size(14.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center),
            );

        let title_col = StackPanel::new()
            .grid_column(1)
            .spacing(2.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                TextBlock::new()
                    .text(m.trigger(slot))
                    .font_size(16.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(if filled {
                        m.binding_summary(&current, self.snapshot.meta_key)
                    } else {
                        m.ring_empty_slot.to_string()
                    })
                    .font_size(12.0)
                    .opacity(0.68)
                    .text_trimming(TextTrimming::CharacterEllipsis),
            ));

        let mut header_children: Vec<KeyedView> = vec![
            KeyedView::new(
                "badge",
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((badge,)),
            ),
            KeyedView::new("title", title_col),
        ];
        if filled {
            header_children.push(KeyedView::new(
                "clear",
                Button::new()
                    .grid_column(2)
                    .style(ButtonStyle::Subtle)
                    .vertical_alignment(VerticalAlignment::Center)
                    .on_click(
                        context.message(Message::PowerMouse(PowerMouseAction::SetBinding {
                            trigger: slot,
                            binding: Binding::PassThrough,
                        })),
                    )
                    .content(TextBlock::new().text(m.clear_slot).font_size(12.0)),
            ));
        }

        // For an empty ring slot, picking any action creates `Binding::Single(action)`.
        let effective_binding = match &current {
            Binding::PassThrough => Binding::Single(Action::None),
            other => other.clone(),
        };
        let shown_action = match &current {
            Binding::Single(action) => Some(action),
            _ => None,
        };
        let target = MouseSlot {
            trigger: slot,
            slot: ActionSlot::Single,
            step: None,
        };
        let picker =
            self.mouse_ring_action_picker(context, target, effective_binding, shown_action);

        let mut body: Vec<View> = vec![
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(10.0)
                .keyed_children(header_children),
            hairline(),
            field_block(m.slot_action, picker),
        ];
        body.extend(self.mouse_capture_note(target));
        if let Some(action) = shown_action {
            match action {
                Action::Text(text) => body.push(self.mouse_text_editor(context, target, text)),
                Action::Workflow(steps) => {
                    body.push(self.mouse_workflow_editor(context, target, &current, steps));
                }
                _ => {}
            }
        }
        body
    }

    /// Action picker specialized for Actions Ring slots: filters out
    /// `Action::ActionsRing` (a ring slot cannot reopen the ring) and allows
    /// starting from an empty selection.
    fn mouse_ring_action_picker(
        &self,
        context: &ViewContext<Self>,
        target: MouseSlot,
        current: Binding,
        action: Option<&Action>,
    ) -> ComboBox {
        let m = self.t().mouse;
        let catalog: Vec<Action> = Action::CATALOG
            .iter()
            .filter(|a| !a.opens_ring())
            .cloned()
            .collect();
        let catalog_len = catalog.len();
        let record = catalog_len;
        let text = catalog_len + 1;
        let workflow = catalog_len + 2;

        let mut labels: Vec<String> = catalog.iter().map(|a| m.action(a).to_string()).collect();
        labels.push(match action {
            Some(Action::Custom(hotkey)) => hotkey.label(self.snapshot.meta_key),
            _ => m.record_shortcut.to_string(),
        });
        labels.push(m.type_text.to_string());
        labels.push(m.workflow.to_string());

        let selected = match action {
            None => None,
            Some(Action::Custom(_)) => Some(record),
            Some(Action::Text(_)) => Some(text),
            Some(Action::Workflow(_)) => Some(workflow),
            Some(other) => catalog.iter().position(|each| each == other),
        };
        let keeps_content = matches!(action, Some(Action::Text(_) | Action::Workflow(_)));

        ComboBox::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .placeholder_text(m.ring_empty_slot)
            .items_source(labels)
            .selected_index(selected)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                let Some(index) = index else {
                    return Message::Noop;
                };
                if Some(index) == selected && keeps_content {
                    return Message::Noop;
                }
                let next = if let Some(found) = catalog.get(index) {
                    found.clone()
                } else if index == record {
                    return Message::PowerMouse(PowerMouseAction::BeginCustomCapture(target));
                } else if index == text {
                    Action::Text(String::new())
                } else if index == workflow {
                    Action::Workflow(Vec::new())
                } else {
                    return Message::Noop;
                };
                let binding = target.replace(&current, next);
                Message::PowerMouse(PowerMouseAction::SetBinding {
                    trigger: target.trigger,
                    binding,
                })
            }))
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
        let target = MouseSlot {
            trigger,
            slot,
            step: None,
        };
        let picker = self.mouse_action_picker(context, target, current.clone(), action);
        let mut rows = vec![field_block(label, picker)];
        rows.extend(self.mouse_capture_note(target));
        match action {
            Action::Text(text) => rows.push(self.mouse_text_editor(context, target, text)),
            Action::Workflow(steps) => {
                rows.push(self.mouse_workflow_editor(context, target, &current, steps));
            }
            _ => {}
        }
        settings_ui::stack(rows, 6.0)
    }

    /// "Press the keys" under the picker that is recording a shortcut.
    fn mouse_capture_note(&self, target: MouseSlot) -> Option<View> {
        (self.snapshot.capturing == Some(CaptureTarget::MouseCustom(target)))
            .then(|| inset_notice(GLYPH_KEYBOARD, self.t().hotkey_capture))
    }

    /// Action choice for a binding slot or a workflow step: the catalog, then
    /// "Record shortcut…" and "Type text…", then "Workflow…" for a slot or the
    /// pause lengths for a step.
    fn mouse_action_picker(
        &self,
        context: &ViewContext<Self>,
        target: MouseSlot,
        current: Binding,
        action: &Action,
    ) -> ComboBox {
        let m = self.t().mouse;
        let catalog_len = Action::CATALOG.len();
        let record = catalog_len;
        let text = catalog_len + 1;
        let extra = catalog_len + 2;
        let is_step = target.step.is_some();
        let mut labels: Vec<String> = m.action_labels().into_iter().map(str::to_string).collect();
        labels.push(match action {
            Action::Custom(hotkey) => hotkey.label(self.snapshot.meta_key),
            _ => m.record_shortcut.to_string(),
        });
        labels.push(m.type_text.to_string());
        if is_step {
            labels.extend(
                WORKFLOW_WAITS_MS
                    .iter()
                    .map(|ms| format!("{} {ms} ms", m.wait)),
            );
        } else {
            labels.push(m.workflow.to_string());
        }
        let selected = match action {
            Action::Custom(_) => Some(record),
            Action::Text(_) => Some(text),
            Action::Workflow(_) => Some(extra),
            Action::Delay(ms) => WORKFLOW_WAITS_MS
                .iter()
                .position(|wait| wait == ms)
                .map(|index| extra + index),
            other => other.catalog_index(),
        };
        let keeps_content = matches!(action, Action::Text(_) | Action::Workflow(_));
        ComboBox::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .items_source(labels)
            .selected_index(selected)
            .on_selection_changed(context.callback(move |index: Option<usize>| {
                let Some(index) = index else {
                    return Message::Noop;
                };
                if Some(index) == selected && keeps_content {
                    return Message::Noop;
                }
                let next = if index < catalog_len {
                    Action::CATALOG[index].clone()
                } else if index == record {
                    return Message::PowerMouse(PowerMouseAction::BeginCustomCapture(target));
                } else if index == text {
                    Action::Text(String::new())
                } else if !is_step {
                    Action::Workflow(Vec::new())
                } else if let Some(ms) = WORKFLOW_WAITS_MS.get(index - extra) {
                    Action::Delay(*ms)
                } else {
                    return Message::Noop;
                };
                let binding = target.replace(&current, next);
                if binding == current {
                    return Message::Noop;
                }
                Message::PowerMouse(PowerMouseAction::SetBinding {
                    trigger: target.trigger,
                    binding,
                })
            }))
    }

    /// Text box and Apply button for a "Type text" action.
    fn mouse_text_editor(
        &self,
        context: &ViewContext<Self>,
        target: MouseSlot,
        saved: &str,
    ) -> View {
        let m = self.t().mouse;
        let draft = self
            .mouse_text_draft
            .as_ref()
            .filter(|(at, _)| *at == target)
            .map(|(_, text)| text.clone());
        let dirty = draft.is_some();
        let row = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(draft.unwrap_or_else(|| saved.to_string()))
                    .on_text_changed(
                        context.callback(move |text| Message::EditMouseText(target, text)),
                    ),
                Button::new()
                    .grid_column(1)
                    .style(dirty.then_some(ButtonStyle::Accent))
                    .is_enabled(dirty)
                    .on_click(context.message(Message::ApplyMouseText(target)))
                    .content(m.apply),
            ));
        settings_ui::surface(settings_ui::stack(
            vec![row, settings_ui::caption(m.text_hint)],
            6.0,
        ))
    }

    /// Numbered steps of a workflow with icon remove buttons and "Add step".
    fn mouse_workflow_editor(
        &self,
        context: &ViewContext<Self>,
        target: MouseSlot,
        current: &Binding,
        steps: &[Action],
    ) -> View {
        let m = self.t().mouse;
        let trigger = target.trigger;
        let replace_steps = |steps: Vec<Action>| {
            Message::PowerMouse(PowerMouseAction::SetBinding {
                trigger,
                binding: current.with_slot(target.slot, Action::Workflow(steps)),
            })
        };
        let mut rows = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let at = MouseSlot {
                step: u8::try_from(index).ok(),
                ..target
            };
            let mut rest = steps.to_vec();
            rest.remove(index);
            let step_badge = Border::new()
                .grid_column(0)
                .width(22.0)
                .height(22.0)
                .corner_radius(CornerRadius::uniform(11.0))
                .background(ThemeBrush::SolidBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .vertical_alignment(VerticalAlignment::Center)
                .content(
                    TextBlock::new()
                        .text((index + 1).to_string())
                        .font_size(11.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .opacity(0.78)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .vertical_alignment(VerticalAlignment::Center),
                );
            let row = Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    step_badge,
                    self.mouse_action_picker(context, at, current.clone(), step)
                        .grid_column(1),
                    StackPanel::new()
                        .grid_column(2)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((icon_button(
                            GLYPH_DELETE,
                            12.0,
                            30.0,
                            context.message(replace_steps(rest)),
                            m.remove_step,
                        ),)),
                ));
            rows.push(row);
            rows.extend(self.mouse_capture_note(at));
            if let Action::Text(text) = step {
                rows.push(self.mouse_text_editor(context, at, text));
            }
        }
        let mut longer = steps.to_vec();
        longer.push(Action::None);
        rows.push(
            Button::new()
                .style(ButtonStyle::Subtle)
                .is_enabled(steps.len() < WORKFLOW_MAX_STEPS)
                .on_click(context.message(replace_steps(longer)))
                .content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(6.0)
                        .children((
                            sized_icon(GLYPH_ADD, 11.0),
                            TextBlock::new().text(m.add_step).font_size(12.0),
                        )),
                ),
        );
        rows.push(settings_ui::caption(m.workflow_hint));
        settings_ui::surface(settings_ui::stack(rows, 6.0))
    }
}

/// Converts a leader-line vertex list into WinUI `Line` segments.
fn leader_segments(points: &[(f64, f64)], selected: bool, customized: bool) -> Vec<View> {
    let mut lines = Vec::new();
    for pair in points.windows(2) {
        let (x1, y1) = pair[0];
        let (x2, y2) = pair[1];
        let line = Line::new()
            .x1(x1)
            .y1(y1)
            .x2(x2)
            .y2(y2)
            .stroke(if selected {
                ThemeBrush::Accent
            } else {
                ThemeBrush::PrimaryText
            })
            .stroke_thickness(if selected { 2.0 } else { 1.15 })
            .opacity(if selected {
                0.95
            } else if customized {
                0.42
            } else {
                0.24
            });
        lines.push(line.into());
    }
    lines
}

/// Physical button anchor dot over the mouse render (with halo when selected).
fn anchor_dot(center: (f64, f64), selected: bool, customized: bool) -> Vec<View> {
    let (cx, cy) = center;
    let mut dots = Vec::new();
    if selected {
        let halo = 18.0;
        dots.push(
            Ellipse::new()
                .width(halo)
                .height(halo)
                .fill(ThemeBrush::Accent)
                .opacity(0.28)
                .canvas_left(cx - halo / 2.0)
                .canvas_top(cy - halo / 2.0)
                .into(),
        );
        let ring = 11.0;
        dots.push(
            Ellipse::new()
                .width(ring)
                .height(ring)
                .fill(ThemeBrush::SolidBackground)
                .canvas_left(cx - ring / 2.0)
                .canvas_top(cy - ring / 2.0)
                .into(),
        );
        let core = 7.0;
        dots.push(
            Ellipse::new()
                .width(core)
                .height(core)
                .fill(ThemeBrush::Accent)
                .canvas_left(cx - core / 2.0)
                .canvas_top(cy - core / 2.0)
                .into(),
        );
    } else {
        let ring = 9.0;
        dots.push(
            Ellipse::new()
                .width(ring)
                .height(ring)
                .fill(ThemeBrush::SolidBackground)
                .opacity(0.90)
                .canvas_left(cx - ring / 2.0)
                .canvas_top(cy - ring / 2.0)
                .into(),
        );
        let core = 5.5;
        dots.push(
            Ellipse::new()
                .width(core)
                .height(core)
                .fill(if customized {
                    ThemeBrush::Accent
                } else {
                    ThemeBrush::PrimaryText
                })
                .opacity(if customized { 0.85 } else { 0.50 })
                .canvas_left(cx - core / 2.0)
                .canvas_top(cy - core / 2.0)
                .into(),
        );
    }
    dots
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindingKind {
    PassThrough,
    Single,
    Hold,
    LongPress,
    Gesture,
}

impl BindingKind {
    fn short_label(self, m: &MouseStrings) -> &'static str {
        match self {
            Self::PassThrough => m.kind_default_short,
            Self::Single => m.kind_single,
            Self::Hold => m.kind_hold,
            Self::LongPress => m.kind_long_press,
            Self::Gesture => m.kind_gesture_short,
        }
    }
}

fn available_kinds(trigger: Trigger) -> &'static [BindingKind] {
    if trigger.is_burst() || trigger.is_ring_slot() {
        &[BindingKind::PassThrough, BindingKind::Single]
    } else if trigger.is_function_key() {
        &[
            BindingKind::PassThrough,
            BindingKind::Single,
            BindingKind::Hold,
            BindingKind::LongPress,
        ]
    } else {
        &[
            BindingKind::PassThrough,
            BindingKind::Single,
            BindingKind::Hold,
            BindingKind::LongPress,
            BindingKind::Gesture,
        ]
    }
}

fn binding_kind(binding: &Binding) -> BindingKind {
    match binding {
        Binding::PassThrough => BindingKind::PassThrough,
        Binding::Single(_) => BindingKind::Single,
        Binding::Hold(_) => BindingKind::Hold,
        Binding::LongPress { .. } => BindingKind::LongPress,
        Binding::Gesture { .. } => BindingKind::Gesture,
    }
}

fn primary_action(binding: &Binding) -> Action {
    match binding {
        Binding::PassThrough => Action::Copy,
        Binding::Single(action) | Binding::Hold(action) => action.clone(),
        Binding::LongPress { short, .. } => short.clone(),
        Binding::Gesture { click, .. } => click.clone(),
    }
}

fn coerce_binding(current: &Binding, kind: BindingKind) -> Binding {
    let primary = primary_action(current);
    match kind {
        BindingKind::PassThrough => Binding::PassThrough,
        BindingKind::Single => Binding::Single(primary),
        BindingKind::Hold => Binding::Hold(primary),
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

// Keep `Side` imported cleanly for future side-aware callout alignment if needed.
const _: fn(Side) = |_| {};
