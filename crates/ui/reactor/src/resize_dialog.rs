//! WinUI 3 Image Resizer dialog rendered by windows-reactor.
//!
//! The dialog owns its draft and job session. Preview/write still run on a worker
//! thread and post `resize::JOB` to the hidden app window; `on_job` then refreshes
//! this component. Closing the dialog destroys only this window; the settings shell
//! stays alive (often hidden) so the tray keeps working.
use super::settings_ui;
use powerkit_app::controller::image_resizer::{
    Job, Shared, parse_extent, quantity_text, ready_only,
};
use powerkit_app::intent::Intent;
use powerkit_app::ui::HostHandle;
use powerkit_app::view::Page;
use powerkit_core::image_resizer::{Fit, PlanItem, ResizeSettings, Unit};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use windows_reactor::*;
fn ui() -> &'static powerkit_core::locale::Strings {
    super::window::strings()
}
const CLIENT_WIDTH: f64 = 560.0;
const CLIENT_HEIGHT: f64 = 700.0;
const MIN_WIDTH: f64 = 420.0;
const MIN_HEIGHT: f64 = 480.0;
#[derive(Clone, PartialEq)]
pub struct Seed {
    pub host: HostHandle,
    pub files: Vec<PathBuf>,
    pub settings: ResizeSettings,
}
#[derive(Default)]
struct Bridge {
    sender: Option<LocalSender<Message>>,
    pending: Option<Seed>,
    open: bool,
}
thread_local! {
    static BRIDGE: RefCell<Bridge> = RefCell::new(Bridge::default());
}
/// Stores a seed until the settings shell opens the dialog window.
pub fn queue(seed: Seed) {
    BRIDGE.with(|bridge| bridge.borrow_mut().pending = Some(seed));
}
pub fn take_pending() -> Option<Seed> {
    BRIDGE.with(|bridge| bridge.borrow_mut().pending.take())
}
/// Reloads files/settings when the WinUI dialog is already open.
pub fn reload(seed: Seed) -> bool {
    BRIDGE.with(|bridge| {
        let bridge = bridge.borrow();
        if let Some(sender) = bridge.sender.clone() {
            sender.send(Message::Reload(Box::new(seed)));
            true
        } else {
            false
        }
    })
}
/// Applies a finished or in-progress worker update to the open dialog.
pub fn on_job() {
    let sender = BRIDGE.with(|bridge| bridge.borrow().sender.clone());
    if let Some(sender) = sender {
        sender.send(Message::JobTick);
    }
}
/// Closes the dialog so process exit is not blocked by a second WinUI window.
pub fn close_for_exit() {
    let sender = BRIDGE.with(|bridge| bridge.borrow().sender.clone());
    if let Some(sender) = sender {
        sender.send(Message::ForceClose);
    }
}
#[derive(Clone)]
pub enum Message {
    Reload(Box<Seed>),
    SelectSize(Option<usize>),
    Width(String),
    Height(String),
    Unit(Option<usize>),
    Fit(Option<usize>),
    Shrink(bool),
    Replace(bool),
    Orient(bool),
    Preview,
    Apply,
    Settings,
    Cancel,
    ForceClose,
    JobTick,
}
pub struct Dialog {
    host: HostHandle,
    files: Vec<PathBuf>,
    settings: ResizeSettings,
    width_text: String,
    height_text: String,
    plan: Option<Vec<PlanItem>>,
    lines: Vec<String>,
    status: String,
    generation: u64,
    cancel: Option<Arc<AtomicBool>>,
    close_when_idle: bool,
    shared: Arc<Mutex<Shared>>,
    apply_enabled: bool,
    busy: bool,
}
impl Component for Dialog {
    type Input = Seed;
    type Message = Message;
    fn create(input: &Seed, context: &ComponentContext<Self>) -> Self {
        BRIDGE.with(|bridge| {
            let mut bridge = bridge.borrow_mut();
            bridge.sender = Some(context.sender());
            bridge.open = true;
            bridge.pending = None;
        });
        Self::from_seed(input)
    }
    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::Reload(seed) => {
                if self.busy {
                    self.request_cancel(false);
                }
                *self = Self::from_seed(&seed);
                BRIDGE.with(|bridge| {
                    let mut bridge = bridge.borrow_mut();
                    bridge.sender = Some(context.sender());
                    bridge.open = true;
                });
            }
            Message::SelectSize(index) => {
                if let Some(index) = index
                    && self.settings.selected != index
                {
                    self.settings.selected = index;
                    self.invalidate();
                }
            }
            Message::Width(text) => {
                if self.width_text != text {
                    self.width_text = text;
                    self.invalidate();
                }
            }
            Message::Height(text) => {
                if self.height_text != text {
                    self.height_text = text;
                    self.invalidate();
                }
            }
            Message::Unit(index) => {
                if let Some(unit) = index.and_then(|index| Unit::from_index(index as i32))
                    && self.settings.custom.unit != unit
                {
                    self.settings.custom.unit = unit;
                    self.invalidate();
                }
            }
            Message::Fit(index) => {
                if let Some(fit) = index.and_then(|index| Fit::from_index(index as i32))
                    && self.settings.custom.fit != fit
                {
                    self.settings.custom.fit = fit;
                    self.invalidate();
                }
            }
            Message::Shrink(on) => {
                if self.settings.shrink_only != on {
                    self.settings.shrink_only = on;
                    self.invalidate();
                }
            }
            Message::Replace(on) => {
                if self.settings.replace != on {
                    self.settings.replace = on;
                    self.invalidate();
                }
            }
            Message::Orient(on) => {
                if self.settings.ignore_orientation != on {
                    self.settings.ignore_orientation = on;
                    self.invalidate();
                }
            }
            Message::Preview => self.preview(),
            Message::Apply => self.apply(),
            Message::Settings => self.host.send(Intent::ShowPage(Page::ImageResizer)),
            Message::Cancel => {
                if self.busy {
                    self.request_cancel(true);
                    self.status = ui().power_store_cancelling.into();
                } else {
                    let _ = context.window().request_close();
                }
            }
            Message::ForceClose => {
                self.request_cancel(false);
                let _ = context.window().request_close();
            }
            Message::JobTick => self.apply_job(context),
        }
    }
    fn view(&self, _: &Seed, context: &mut ViewContext<Self>) -> View {
        context.window_title(ui().nav_image_resizer);
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Acrylic)
                .theme(WindowTheme::System)
                .client_size(CLIENT_WIDTH, CLIENT_HEIGHT)
                .constraints(WindowConstraints {
                    min_width: Some(MIN_WIDTH),
                    min_height: Some(MIN_HEIGHT),
                    max_width: None,
                    max_height: None,
                }),
        );
        let custom = self.settings.selected == self.settings.sizes.len();
        let labels = ui().resize_choice_labels(&self.settings);
        let preview_rows = self
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                KeyedView::new(
                    index,
                    TextBlock::new()
                        .text(line.clone())
                        .text_wrapping(TextWrapping::Wrap)
                        .font_size(12.0),
                )
            })
            .collect::<Vec<_>>();
        let size = settings_ui::surface(settings_ui::stack(
            vec![
                TextBlock::new()
                    .text(ui().resize_size)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                settings_ui::stack(
                    labels
                        .into_iter()
                        .enumerate()
                        .map(|(index, label)| {
                            let on = self.settings.selected == index;
                            settings_ui::choice_button(on)
                                .on_click(context.message(Message::SelectSize(Some(index))))
                                .content(settings_ui::choice_content(&label, on))
                        })
                        .collect(),
                    6.0,
                ),
                Grid::new()
                    .columns([
                        GridLength::STAR,
                        GridLength::STAR,
                        GridLength::STAR,
                        GridLength::STAR,
                    ])
                    .column_spacing(8.0)
                    .children((
                        field(
                            ui().resize_width,
                            TextBox::new()
                                .text(self.width_text.clone())
                                .is_enabled(custom)
                                .on_text_changed(context.callback(Message::Width)),
                            0,
                        ),
                        field(
                            ui().resize_height,
                            TextBox::new()
                                .text(self.height_text.clone())
                                .is_enabled(custom)
                                .on_text_changed(context.callback(Message::Height)),
                            1,
                        ),
                        field(
                            ui().resize_unit,
                            ComboBox::new()
                                .items_source([
                                    ui().resize_unit_px,
                                    ui().resize_unit_percent,
                                    ui().resize_unit_in,
                                    ui().resize_unit_cm,
                                ])
                                .selected_index(Some(self.settings.custom.unit as usize))
                                .is_enabled(custom)
                                .on_selection_changed(context.callback(Message::Unit)),
                            2,
                        ),
                        field(
                            ui().resize_fit,
                            ComboBox::new()
                                .items_source([
                                    ui().resize_fit_fit,
                                    ui().resize_fit_fill,
                                    ui().resize_fit_stretch,
                                ])
                                .selected_index(Some(self.settings.custom.fit as usize))
                                .is_enabled(custom)
                                .on_selection_changed(context.callback(Message::Fit)),
                            3,
                        ),
                    )),
            ],
            10.0,
        ));
        let options = settings_ui::surface(settings_ui::stack(
            vec![
                toggle(
                    context,
                    ui().resize_shrink,
                    self.settings.shrink_only,
                    Message::Shrink,
                ),
                toggle(
                    context,
                    ui().resize_replace,
                    self.settings.replace,
                    Message::Replace,
                ),
                toggle(
                    context,
                    ui().resize_orientation,
                    self.settings.ignore_orientation,
                    Message::Orient,
                ),
            ],
            8.0,
        ));
        let preview = settings_ui::surface(settings_ui::stack(
            vec![
                TextBlock::new()
                    .text(ui().resize_preview)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
                Border::new()
                    .background(ThemeBrush::SolidBackground)
                    .border_brush(ThemeBrush::CardStroke)
                    .border_thickness(Thickness::uniform(1.0))
                    .corner_radius(CornerRadius::uniform(6.0))
                    .padding(Thickness::xy(12.0, 10.0))
                    .min_height(140.0)
                    .content(
                        ScrollViewer::new()
                            .content(StackPanel::new().spacing(4.0).keyed_children(preview_rows)),
                    ),
                TextBlock::new()
                    .text(self.status.clone())
                    .text_wrapping(TextWrapping::Wrap)
                    .opacity(0.8)
                    .into(),
            ],
            8.0,
        ));
        let actions = Grid::new()
            .columns([
                GridLength::STAR,
                GridLength::STAR,
                GridLength::STAR,
                GridLength::STAR,
            ])
            .column_spacing(8.0)
            .children((
                Button::new()
                    .grid_column(0)
                    .is_enabled(!self.busy)
                    .on_click(context.message(Message::Preview))
                    .content(ui().resize_preview_action),
                Button::new()
                    .grid_column(1)
                    .style(ButtonStyle::Accent)
                    .is_enabled(self.apply_enabled && !self.busy)
                    .on_click(context.message(Message::Apply))
                    .content(ui().resize_apply),
                Button::new()
                    .grid_column(2)
                    .on_click(context.message(Message::Settings))
                    .content(ui().nav_settings),
                Button::new()
                    .grid_column(3)
                    .on_click(context.message(Message::Cancel))
                    .content(ui().power_store_cancel),
            ));
        ScrollViewer::new().content(
            StackPanel::new()
                .margin(Thickness::new(20.0, 16.0, 20.0, 20.0))
                .spacing(12.0)
                .children((
                    TextBlock::new()
                        .text(ui().nav_image_resizer)
                        .font_size(28.0)
                        .font_weight(FontWeight::SEMI_BOLD),
                    size,
                    options,
                    preview,
                    actions,
                )),
        )
    }
}
impl Drop for Dialog {
    fn drop(&mut self) {
        self.request_cancel(false);
        BRIDGE.with(|bridge| {
            let mut bridge = bridge.borrow_mut();
            bridge.sender = None;
            bridge.open = false;
        });
    }
}
impl Dialog {
    fn from_seed(seed: &Seed) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            generation: 0,
            status: ui().resize_preview_hint.into(),
            lines: None,
            plan: None,
            stop: false,
        }));
        let status = ui().resize_files_selected(seed.files.len());
        if let Ok(mut shared) = shared.lock() {
            shared.status = status.clone();
        }
        Self {
            host: seed.host.clone(),
            files: seed.files.clone(),
            settings: seed.settings.clone(),
            width_text: quantity_text(seed.settings.custom.width),
            height_text: quantity_text(seed.settings.custom.height),
            plan: None,
            lines: Vec::new(),
            status,
            generation: 0,
            cancel: None,
            close_when_idle: false,
            shared,
            apply_enabled: false,
            busy: false,
        }
    }
    fn invalidate(&mut self) {
        self.plan = None;
        self.apply_enabled = false;
        self.generation = self.generation.wrapping_add(1);
        self.request_cancel(false);
        if let Ok(mut shared) = self.shared.lock() {
            shared.generation = self.generation;
            shared.stop = false;
            shared.status = ui().resize_preview_stale.into();
        }
        self.status = ui().resize_preview_stale.into();
    }
    fn request_cancel(&mut self, close_when_idle: bool) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.close_when_idle = close_when_idle;
        self.busy = false;
    }
    fn read_settings(&mut self) -> Option<ResizeSettings> {
        let mut settings = self.settings.clone();
        let latest = super::window::latest_resize();
        settings.jpeg_quality = latest.jpeg_quality;
        settings.png = latest.png;
        settings.tiff = latest.tiff;
        settings.filename = latest.filename;
        settings.keep_date = latest.keep_date;
        settings.remove_metadata = latest.remove_metadata;
        settings.fallback = latest.fallback;
        settings.sizes = latest.sizes;
        if settings.selected == settings.sizes.len() {
            settings.custom.width = parse_extent(&self.width_text)?;
            settings.custom.height = parse_extent(&self.height_text)?;
        }
        Some(settings)
    }
    fn preview(&mut self) {
        let Some(settings) = self.read_settings() else {
            self.status = ui().resize_custom_invalid.into();
            return;
        };
        // A failed save is reported by the host; the preview does not need it.
        self.host.send(Intent::RememberResize(settings.clone()));
        self.settings = settings;
        self.start_job(false);
    }
    fn apply(&mut self) {
        let Some(plan) = self.plan.clone() else {
            return;
        };
        if ready_only(&plan).is_none() {
            self.status = ui().resize_need_preview.into();
            return;
        }
        self.start_job(true);
    }
    fn start_job(&mut self, write: bool) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.generation = self.generation.wrapping_add(1);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));
        let generation = self.generation;
        if let Ok(mut shared) = self.shared.lock() {
            shared.generation = generation;
            shared.stop = false;
            shared.lines = None;
            shared.plan = None;
        }
        self.status = if write {
            ui().resize_writing.into()
        } else {
            ui().resize_previewing.into()
        };
        self.apply_enabled = false;
        self.busy = true;
        let job = Job {
            files: self.files.clone(),
            settings: self.settings.clone(),
            plan: self.plan.clone().unwrap_or_default(),
            shared: Arc::clone(&self.shared),
            generation,
            cancel,
            write,
            strings: ui(),
        };
        self.host.start_resize(job);
    }
    fn apply_job(&mut self, context: &ComponentContext<Self>) {
        let update = self.shared.lock().ok().map(|shared| shared.clone_update());
        let Some(update) = update else {
            return;
        };
        if update.generation != self.generation {
            return;
        }
        self.status = update.status;
        if let Some(lines) = update.lines {
            self.lines = lines;
        }
        if update.stop {
            self.cancel = None;
            self.busy = false;
            if let Some(plan) = update.plan {
                self.apply_enabled = ready_only(&plan).is_some();
                self.plan = Some(plan);
            }
            if self.close_when_idle {
                self.close_when_idle = false;
                let _ = context.window().request_close();
            }
        }
    }
}
fn field(label: &str, control: impl Into<View>, column: i32) -> View {
    StackPanel::new()
        .grid_column(column)
        .spacing(4.0)
        .children((
            TextBlock::new().text(label).opacity(0.8).font_size(12.0),
            control.into(),
        ))
}
fn toggle(context: &ViewContext<Dialog>, label: &str, on: bool, map: fn(bool) -> Message) -> View {
    Grid::new()
        .columns([GridLength::STAR, GridLength::Auto])
        .column_spacing(12.0)
        .children((
            TextBlock::new()
                .grid_column(0)
                .text(label)
                .text_wrapping(TextWrapping::Wrap)
                .vertical_alignment(VerticalAlignment::Center),
            StackPanel::new()
                .grid_column(1)
                .children((settings_ui::toggle_switch(
                    ToggleSwitch::new()
                        .is_on(on)
                        .on_toggled(context.callback(map)),
                ),)),
        ))
}
