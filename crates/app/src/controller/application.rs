//! The application: the state every platform shell shares and the decisions
//! made on it.
//!
//! [`App`] owns the preferences, the PowerStore and MediaRenamer sessions and
//! the key-recording state. A platform shell hands it what the user asked for
//! and gets back a [`Step`]: what the shell must do next that only it can do
//! (store the preferences, ask for a file, copy text). `App` never calls the
//! platform itself. That keeps it free of re-entrancy (a file dialog pumps the
//! shell's messages while it is open) and lets every decision here be tested
//! without a window.
//!
//! Preferences change in one way: the shell stores a candidate, then calls
//! [`App::adopt`]. A step that needs something done after the store succeeds
//! names it in [`Then`], and the shell passes that back to [`App::saved`].
use crate::command::Command;
use crate::controller::hotkeys::{self, Problem, Rejected, Slot};
use crate::controller::media_renamer::{self, Action as RenameAction};
use crate::controller::power_store::{self, Action as StoreAction, Notice};
use crate::controller::settings::Edit;
use crate::intent::{ExpiryPick, Intent, MouseAction, MouseSlot};
use crate::view::{CaptureTarget, Page, Snapshot, mode_index};
use powerkit_core::always_on::{Choice, Session};
use powerkit_core::always_top::{OPACITY_STEP, Pin};
use powerkit_core::hotkey::Hotkey;
use powerkit_core::image_resizer::ResizeSettings;
use powerkit_core::locale::{Strings, UiLanguage};
use powerkit_core::power_mouse::{self, device};
use powerkit_core::power_store::{
    ExportPackage, ManagerKind, ManagerSettings, OperationKind, PowerStorePreferences,
    QueuedOperation,
};
use powerkit_core::primitives::{KeyCode, Rgb};
use powerkit_core::settings::Preferences;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The file manager starts one process per selected item for its classic
/// verbs, so selections that arrive this close together are one selection.
const RENAME_HANDOFF_BURST: Duration = Duration::from_secs(2);

/// What the next recorded key is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capturing {
    Hotkey(Slot),
    MouseCustom(MouseSlot),
    MouseAddFKey,
}

/// What the shell does next for a request.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// The application state is up to date; nothing else to do.
    Done,
    /// Store `prefs`. On success adopt them and call [`App::saved`] with
    /// `then`; on failure report the error and do neither.
    Save { prefs: Box<Preferences>, then: Then },
    /// Commit this edit through `settings::commit`.
    Commit(Edit),
    /// Ask the user, then give the answer to the matching `App` method.
    Ask(Ask),
    /// Open a place outside PowerKit.
    Open(Destination),
    /// Put the text on the clipboard; call [`App::copied`] if it got there.
    CopyText(String),
    /// Send the current snapshot again so the UI drops a draft the
    /// application refused.
    Refresh,
    /// Start recording a key; the state is already set.
    BeginCapture,
    /// Commit an edit the UI holds as a draft: if storing fails, report it
    /// and send the snapshot again so the draft is dropped.
    CommitDraft(Edit),
    /// Bring the settings window forward.
    Present,
    /// Show this page and bring the settings window forward.
    ShowPage(Page),
    /// Have a running tool do something.
    Tool(ToolRequest),
    /// Close PowerKit.
    Exit,
}

/// A place outside PowerKit a request leads to; each platform knows its own
/// address for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    /// Where the system store lists pending updates.
    StoreUpdates,
    /// The system's color settings.
    SystemColors,
}

/// Something only the platform's tools can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolRequest {
    /// Pin or unpin the last eligible window.
    TogglePin,
    /// Change the pinned window's opacity by this many percent.
    StepOpacity(i32),
    /// Pin or unpin the window in front.
    ToggleForegroundPin,
    /// Change the opacity of the window in front, if it is pinned.
    StepForegroundOpacity(i32),
    /// Stop a running session, or start the one a shortcut starts.
    ToggleAlwaysOn,
    /// Start or stop measuring.
    ToggleRuler,
    /// Ask for images and open the resize dialog.
    OpenResize,
    /// Keep the display on, or stop doing so, and store the choice.
    ToggleDisplay,
    ExtendAlwaysOn,
    /// Keep awake while the last eligible window's process runs.
    BindLastWindow,
    StartAlwaysOn(Choice),
    /// Store the custom interval and restart a running custom session.
    SetInterval(u32),
    StartExpiry(ExpiryPick),
    /// Show or hide the Actions Ring.
    ToggleRing,
    /// Switch the frame between the system accent and a fixed color; the
    /// platform knows the accent.
    ToggleFrameAccent,
}

/// The answer to an intent: what the shell does next, and whether the view
/// must be rebuilt afterwards even though no preference changed.
#[derive(Debug, PartialEq)]
pub struct Reply {
    pub step: Step,
    pub refresh: bool,
}
impl From<Step> for Reply {
    fn from(step: Step) -> Self {
        Self {
            step,
            refresh: false,
        }
    }
}

/// A question only the platform can put to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The program of a package manager; answer with [`App::program_picked`].
    ManagerProgram(ManagerKind),
    /// Where to write the package list; answer with [`App::export_to`].
    ExportFile,
    /// A package list to read; answer with [`App::import_from`].
    ImportFile,
    /// Media files to add; answer with [`App::media_picked`].
    MediaFiles,
    /// A folder of media to add; answer with [`App::media_picked`].
    MediaFolder,
}

/// What follows a successful store.
#[derive(Debug, PartialEq)]
pub enum Then {
    Nothing,
    StoreNotice(Notice),
    /// Reload the installed list and probe the manager again.
    RelistManager(ManagerKind),
    /// The check interval changed.
    Reschedule,
    /// Remote logos were switched on or off.
    RemoteIcons,
    LowercaseExt(bool),
    /// Queue the packages of an import with the options just stored.
    InstallImport(Vec<ExportPackage>),
}

/// What a recorded key leads to.
#[derive(Debug, PartialEq)]
pub enum Captured {
    /// No recording was running, or the key means nothing for it.
    Ignored,
    /// End the recording without a change.
    Cancel,
    /// Commit the edit. `cancel_if_unchanged`: when the edit changes nothing,
    /// end the recording as cancelled.
    Commit {
        edit: Edit,
        cancel_if_unchanged: bool,
    },
}

/// What an incoming selection for MediaRenamer leads to.
#[derive(Debug, PartialEq)]
pub enum Handoff {
    /// Nothing changed (a job is running); show the page.
    Show,
    /// Unapplied renames would be lost: ask, then call
    /// [`App::replace_rename`] on a yes.
    Confirm(Vec<PathBuf>),
}

/// What one turn of the shell's timer found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// Background work finished; the view must be rebuilt.
    pub changed: bool,
    /// A check found this many updates, more than the user was told about,
    /// and wants a notification.
    pub updates: Option<usize>,
}

/// What the platform reads from the running tools for a snapshot.
pub struct Readings<'a> {
    /// The tray caption, also the status line of the Home page.
    pub status: String,
    pub ui_language: UiLanguage,
    pub session: Session,
    pub bound_app: Option<String>,
    pub pin: &'a Pin,
    pub ruler_active: bool,
    /// The registered form of a shortcut, as the platform words it.
    pub hotkey_label: &'a dyn Fn(Slot) -> String,
    /// Saved shortcuts that could not be registered.
    pub hotkey_problems: Vec<(Slot, Problem)>,
    /// The system accent color, shown while the frame follows it.
    pub accent: Rgb,
    pub rail_accent: Option<Rgb>,
    pub tray_available: bool,
    pub mouse_elevated: bool,
    pub mouse_devices: Vec<device::Device>,
    pub mouse_adapters: BTreeMap<String, device::AdapterStatus>,
    pub mouse_assets_rev: u64,
    /// Tools the platform cannot offer.
    pub unavailable: &'a [crate::controller::settings::Tool],
    pub packaged: bool,
    pub version: &'static str,
    /// The platform's name for the meta key.
    pub meta_key: &'static str,
    pub hero_phase: u8,
}

pub struct App {
    pub prefs: Preferences,
    pub power_store: power_store::Session,
    pub media_renamer: media_renamer::Session,
    /// Shown as the Home hint until the next preference change.
    pub warning: Option<String>,
    capturing: Option<Capturing>,
    /// The last recorded chord refused because another slot holds it.
    hotkey_rejected: Option<Rejected>,
    rename_handoff_at: Option<Instant>,
}
impl App {
    pub fn new(
        prefs: Preferences,
        power_store: power_store::Session,
        media_renamer: media_renamer::Session,
    ) -> Self {
        Self {
            prefs,
            power_store,
            media_renamer,
            warning: None,
            capturing: None,
            hotkey_rejected: None,
            rename_handoff_at: None,
        }
    }

    /// Makes stored preferences the running ones. A recording in progress and
    /// the last warning end with the change.
    pub fn adopt(&mut self, prefs: Preferences) {
        self.prefs = prefs;
        self.warning = None;
        self.capturing = None;
        self.hotkey_rejected = None;
    }

    /// One turn of the shell's timer: starts the periodic PowerStore check
    /// when it is due and takes in what the workers of both workspaces
    /// finished. A tool that is switched off is left alone.
    pub fn tick(&mut self, now: Instant) -> Tick {
        let mut tick = Tick::default();
        if self.prefs.modules.power_store {
            self.power_store
                .tick(now, self.prefs.power_store.check_minutes);
            let (changed, found) = self.power_store.poll(&self.prefs.power_store.filters);
            tick.changed = changed;
            tick.updates = found.filter(|_| self.prefs.power_store.notify);
        }
        if self.prefs.modules.media_renamer {
            tick.changed |= self.media_renamer.poll();
        }
        tick
    }

    /// The session to start after the custom interval was stored: a running
    /// custom interval starts over with the new length, counted from now.
    /// Every other session, a preset interval among them, is left alone.
    pub fn interval_restart(&self, running: &Session, uptime_ms: u64) -> Option<Session> {
        matches!(running, Session::Interval { custom: true, .. })
            .then(|| Session::interval(uptime_ms, self.prefs.interval_minutes, true))
    }

    /// Carries out what a successful [`Step::Save`] asked for.
    pub fn saved(&mut self, then: Then) {
        match then {
            Then::Nothing => {}
            Then::StoreNotice(notice) => self.power_store.set_notice(notice),
            Then::RelistManager(kind) => {
                self.power_store.refresh_installed(true);
                // The drawer shows the program and version, which the edit may have changed.
                self.power_store.probe_manager(kind);
            }
            Then::Reschedule => self
                .power_store
                .reschedule(self.prefs.power_store.check_minutes),
            Then::RemoteIcons => self.apply_remote_icons(),
            Then::LowercaseExt(on) => self.media_renamer.set_lowercase_ext(on),
            Then::InstallImport(packages) => self.enqueue_import(packages),
        }
    }

    // ---- every request ---------------------------------------------------

    /// What a request from the UI, the tray or a shortcut means. A request
    /// for a tool that is switched off means nothing.
    pub fn intent(&mut self, intent: Intent) -> Reply {
        if intent.tool().is_some_and(|tool| !tool.enabled(&self.prefs)) {
            return Step::Done.into();
        }
        let refreshed = |step| Reply {
            step,
            refresh: true,
        };
        match intent {
            Intent::Command(command) => self.command(command),
            Intent::Shortcut(slot) => Step::Tool(match slot {
                Slot::Pin => ToolRequest::ToggleForegroundPin,
                Slot::OpacityUp => ToolRequest::StepForegroundOpacity(OPACITY_STEP),
                Slot::OpacityDown => ToolRequest::StepForegroundOpacity(-OPACITY_STEP),
                Slot::Ruler => ToolRequest::ToggleRuler,
                Slot::AlwaysOn => ToolRequest::ToggleAlwaysOn,
            })
            .into(),
            Intent::ShellOpacity(percent) => Step::Commit(Edit::ShellOpacity(percent)).into(),
            Intent::FrameColor(color) => Step::Commit(Edit::FrameColor(color)).into(),
            Intent::RulerColor(color) => Step::Commit(Edit::RulerColor(color)).into(),
            Intent::Interval(minutes) => Step::Tool(ToolRequest::SetInterval(minutes)).into(),
            Intent::Expiry(pick) => Step::Tool(ToolRequest::StartExpiry(pick)).into(),
            Intent::ResizeFilename(text) => match ResizeSettings::parse_filename(&text) {
                Some(filename) => Step::CommitDraft(Edit::ResizeFilename(filename)).into(),
                None => Step::Refresh.into(),
            },
            Intent::PowerStore(action) => refreshed(self.power_store(action)),
            Intent::MediaRenamer(action) => refreshed(self.media_renamer(action)),
            Intent::PowerMouse(action) => self.power_mouse(action).into(),
            Intent::Tray(edit) => Step::Commit(Edit::Tray(vec![edit])).into(),
            Intent::ShowPage(page) => Step::ShowPage(page).into(),
            Intent::RememberResize(last) => Step::Commit(Edit::RememberResize(last)).into(),
        }
    }
    fn command(&mut self, command: Command) -> Reply {
        if let Some(edit) = command.edit() {
            return Step::Commit(edit).into();
        }
        if let Some(slot) = command.hotkey_slot() {
            self.begin_capture(Capturing::Hotkey(slot));
            return Step::BeginCapture.into();
        }
        let tool = |request| Reply::from(Step::Tool(request));
        let changed = Reply {
            step: Step::Done,
            refresh: true,
        };
        let step = OPACITY_STEP;
        match command {
            Command::OpenSettings => Step::Present.into(),
            Command::Exit => Step::Exit.into(),
            Command::TogglePin => tool(ToolRequest::TogglePin),
            Command::OpacityUp => tool(ToolRequest::StepOpacity(step)),
            Command::OpacityDown => tool(ToolRequest::StepOpacity(-step)),
            Command::ToggleMeasure => tool(ToolRequest::ToggleRuler),
            Command::ResizeOpen => tool(ToolRequest::OpenResize),
            Command::ToggleDisplay => tool(ToolRequest::ToggleDisplay),
            Command::AlwaysOnExtend => tool(ToolRequest::ExtendAlwaysOn),
            Command::AlwaysOnBindProcess => tool(ToolRequest::BindLastWindow),
            Command::ToggleFrameAccent => tool(ToolRequest::ToggleFrameAccent),
            Command::PowerMouseRing => tool(ToolRequest::ToggleRing),
            Command::OpenColorSettings => Step::Open(Destination::SystemColors).into(),
            Command::MediaRenamerOpen => Step::ShowPage(Page::MediaRenamer).into(),
            Command::OpenAlwaysOnPage => Step::ShowPage(Page::AlwaysOn).into(),
            Command::OpenAlwaysTopPage => Step::ShowPage(Page::AlwaysTop).into(),
            Command::OpenScreenRulerPage => Step::ShowPage(Page::ScreenRuler).into(),
            Command::PowerStoreOpen => {
                self.power_store
                    .set_remote_icons(self.prefs.power_store.remote_icons);
                self.power_store.show_installed();
                Step::ShowPage(Page::PowerStore).into()
            }
            Command::PowerStoreRefresh => {
                self.power_store
                    .set_remote_icons(self.prefs.power_store.remote_icons);
                self.power_store.refresh_active();
                changed
            }
            Command::PowerStoreEnsureLoaded => {
                self.power_store
                    .set_remote_icons(self.prefs.power_store.remote_icons);
                self.power_store.ensure_loaded();
                changed
            }
            Command::PowerStoreCheckNow => {
                self.power_store.check_now();
                changed
            }
            Command::PowerStoreCancelPending => {
                self.power_store.clear_pending();
                changed
            }
            Command::PowerStoreUpdateAll => {
                self.power_store.enqueue_updates(&self.prefs.power_store);
                changed
            }
            Command::PowerStoreCancelRunning => {
                self.power_store.cancel_running();
                changed
            }
            command => match command.always_on_choice() {
                Some(choice) => tool(ToolRequest::StartAlwaysOn(choice)),
                None => Step::Done.into(),
            },
        }
    }
    /// What each slot of the Actions Ring shows: the action's name, the
    /// recorded keys, or the start of the text it types. `None` for an empty
    /// slot.
    pub fn ring_labels(&self, t: &Strings, meta: &str) -> Vec<Option<String>> {
        power_mouse::Trigger::RING
            .iter()
            .map(|trigger| match self.prefs.mouse.binding(*trigger) {
                power_mouse::Binding::Single(power_mouse::Action::None) => None,
                power_mouse::Binding::Single(action) => Some(t.mouse.action_label(&action, meta)),
                _ => None,
            })
            .collect()
    }

    // ---- key recording -------------------------------------------------

    pub fn capturing(&self) -> Option<Capturing> {
        self.capturing
    }
    pub fn begin_capture(&mut self, capturing: Capturing) {
        self.capturing = Some(capturing);
        self.hotkey_rejected = None;
    }
    /// Ends a recording; `false` when none was running.
    pub fn cancel_capture(&mut self) -> bool {
        self.capturing.take().is_some()
    }
    /// A chord was recorded.
    pub fn finish_capture(&mut self, key: Hotkey) -> Captured {
        match self.capturing {
            None => Captured::Ignored,
            Some(Capturing::Hotkey(slot)) => match hotkeys::record(&self.prefs, slot, key) {
                Ok(edit) => Captured::Commit {
                    edit,
                    cancel_if_unchanged: false,
                },
                Err(rejected) => {
                    self.hotkey_rejected = Some(rejected);
                    Captured::Cancel
                }
            },
            Some(Capturing::MouseCustom(target)) => {
                let binding = self.prefs.mouse.binding(target.trigger);
                Captured::Commit {
                    edit: Edit::MouseBinding {
                        trigger: target.trigger,
                        binding: target.replace(&binding, power_mouse::Action::Custom(key)),
                    },
                    cancel_if_unchanged: true,
                }
            }
            Some(Capturing::MouseAddFKey) => Captured::Cancel,
        }
    }
    /// A single key was pressed while a function key is being added.
    pub fn finish_function_key(&mut self, key: KeyCode) -> Captured {
        if self.capturing != Some(Capturing::MouseAddFKey) {
            return Captured::Ignored;
        }
        match power_mouse::Trigger::from_key(key) {
            Some(trigger) => Captured::Commit {
                edit: Edit::MouseAddFunctionKey(trigger),
                cancel_if_unchanged: true,
            },
            None => Captured::Ignored,
        }
    }
    /// The chord last refused, for the platform's shortcut diagnosis.
    pub fn hotkey_rejected(&self) -> Option<Rejected> {
        self.hotkey_rejected
    }
    /// Notes under shortcut cards, keyed by the card's capture command: a
    /// refused recording, and saved chords an enabled tool could not register.
    pub fn hotkey_notes(
        &self,
        problems: &[(Slot, Problem)],
        t: &Strings,
    ) -> Vec<(Command, String)> {
        problems
            .iter()
            .map(|&(slot, problem)| {
                let note = match problem {
                    Problem::Duplicate { holder } => t
                        .hotkey_duplicate
                        .replace("{name}", &hotkey_name(t, holder)),
                    Problem::Shared { holder } => {
                        t.hotkey_shared.replace("{name}", &hotkey_name(t, holder))
                    }
                    Problem::Taken => t.hotkey_taken.into(),
                };
                (Command::capture(slot), note)
            })
            .collect()
    }

    // ---- PowerMouse ----------------------------------------------------

    pub fn power_mouse(&mut self, action: MouseAction) -> Step {
        match action {
            MouseAction::BeginCustomCapture(target) => {
                self.begin_capture(Capturing::MouseCustom(target));
                Step::BeginCapture
            }
            MouseAction::BeginAddFKey => {
                self.begin_capture(Capturing::MouseAddFKey);
                Step::BeginCapture
            }
            action => match action.edit() {
                Some(edit) => Step::Commit(edit),
                None => Step::Done,
            },
        }
    }

    // ---- MediaRenamer --------------------------------------------------

    pub fn media_renamer(&mut self, action: RenameAction) -> Step {
        let save = |prefs: &Preferences,
                    edit: &dyn Fn(&mut powerkit_core::media_renamer::Preferences),
                    then: Then| {
            let mut prefs = prefs.clone();
            edit(&mut prefs.rename);
            Step::Save {
                prefs: Box::new(prefs),
                then,
            }
        };
        match action {
            RenameAction::AddFiles => Step::Ask(Ask::MediaFiles),
            RenameAction::AddFolder => Step::Ask(Ask::MediaFolder),
            RenameAction::SaveDefaultTemplate => {
                let template = self.media_renamer.template_text().to_string();
                save(
                    &self.prefs,
                    &|prefs| prefs.template = template.clone(),
                    Then::Nothing,
                )
            }
            RenameAction::SetDefaultRecurse(on) => {
                save(&self.prefs, &|prefs| prefs.recurse = on, Then::Nothing)
            }
            RenameAction::SetDefaultMtimeFallback(on) => save(
                &self.prefs,
                &|prefs| prefs.mtime_fallback = on,
                Then::Nothing,
            ),
            RenameAction::SetLowercaseExt(on) => save(
                &self.prefs,
                &|prefs| prefs.lowercase_ext = on,
                Then::LowercaseExt(on),
            ),
            action => {
                self.media_renamer.act(action);
                Step::Done
            }
        }
    }
    /// The answer to [`Ask::MediaFiles`] or [`Ask::MediaFolder`].
    pub fn media_picked(&mut self, paths: Vec<PathBuf>) {
        self.media_renamer.add(paths);
    }
    /// A selection arrived from the file manager or another launch.
    pub fn receive_rename(&mut self, paths: Vec<PathBuf>, now: Instant) -> Handoff {
        if self.media_renamer.busy() {
            return Handoff::Show;
        }
        let burst = self
            .rename_handoff_at
            .is_some_and(|at| now.saturating_duration_since(at) < RENAME_HANDOFF_BURST);
        if burst {
            self.rename_handoff_at = Some(now);
            self.media_renamer.add(paths);
            return Handoff::Show;
        }
        if self.media_renamer.has_unapplied() {
            return Handoff::Confirm(paths);
        }
        self.replace_rename(paths, now);
        Handoff::Show
    }
    /// Replaces the workspace with `paths`, dropping what was listed.
    pub fn replace_rename(&mut self, paths: Vec<PathBuf>, now: Instant) {
        self.rename_handoff_at = Some(now);
        self.media_renamer.replace(paths);
    }

    // ---- PowerStore ----------------------------------------------------

    /// A PowerStore preference edit; `edit` returns whether to store it.
    fn store_edit(
        &self,
        then: Then,
        edit: impl FnOnce(&mut PowerStorePreferences) -> bool,
    ) -> Step {
        let mut prefs = self.prefs.clone();
        if !edit(&mut prefs.power_store) {
            return Step::Done;
        }
        Step::Save {
            prefs: Box::new(prefs),
            then,
        }
    }
    /// An edit of one manager's settings. `relist` reloads the installed
    /// list, which a changed switch or program path alters; a default scope
    /// only affects later operations.
    fn manager_edit(
        &self,
        kind: ManagerKind,
        relist: bool,
        edit: impl FnOnce(&mut ManagerSettings),
    ) -> Step {
        let then = if relist {
            Then::RelistManager(kind)
        } else {
            Then::Nothing
        };
        self.store_edit(then, |prefs| {
            let before = prefs.manager(kind);
            let mut after = before.clone();
            edit(&mut after);
            after != before && prefs.set_manager(kind, after)
        })
    }
    fn apply_remote_icons(&mut self) {
        self.power_store
            .set_remote_icons(self.prefs.power_store.remote_icons);
        // Reload so database-assigned logos appear or disappear immediately.
        self.power_store.refresh_installed(true);
    }
    fn enqueue_import(&mut self, packages: Vec<ExportPackage>) {
        for package in packages {
            let options = self.prefs.power_store.operation_options_for(&package.key);
            self.power_store.enqueue(QueuedOperation {
                kind: OperationKind::Install,
                name: package.key.id.clone(),
                key: package.key,
                options,
            });
        }
    }

    pub fn power_store(&mut self, action: StoreAction) -> Step {
        let store = &mut self.power_store;
        match action {
            StoreAction::SetFilterText(text) => store.set_filter_text(text),
            StoreAction::SetSourceChecked(source, checked) => {
                store.set_source_checked(&source, checked)
            }
            StoreAction::ClearSourceFilter => store.clear_source_filter(),
            StoreAction::SetStatusFilter(status) => store.set_status_filter(status),
            StoreAction::SetScopeFilter(scope) => store.set_scope_filter(scope),
            StoreAction::SetSortKey(key) => store.set_sort_key(key),
            StoreAction::SetSort(key, ascending) => store.set_sort(key, ascending),
            StoreAction::StartDiscover { query, source } => store.start_discover(query, source),
            StoreAction::ExitDiscover => store.exit_discover(),
            StoreAction::Queue(kind, key, name) => {
                let options = self.prefs.power_store.operation_options_for(&key);
                store.enqueue(QueuedOperation {
                    kind,
                    key,
                    name,
                    options,
                });
            }
            StoreAction::QueueSelected(kind) => {
                store.enqueue_selected(kind, &self.prefs.power_store);
            }
            StoreAction::QueueVisibleUpdates => {
                store.enqueue_visible_updates(&self.prefs.power_store);
            }
            StoreAction::OpenStoreUpdates => {
                return Step::Open(Destination::StoreUpdates);
            }
            StoreAction::IgnoreSelected => {
                let keys = store.take_selected_non_own();
                return self.store_edit(Then::Nothing, |prefs| {
                    let mut changed = false;
                    for key in keys {
                        changed |= prefs.filters.ignore(key);
                    }
                    changed
                });
            }
            StoreAction::SetSelected(key, selected) => store.set_selected(key, selected),
            StoreAction::SetSelectAllVisible(select) => {
                store.set_select_all_visible(select, &self.prefs.power_store)
            }
            StoreAction::ClearSelection => store.clear_selection(),
            StoreAction::OpenInspector(key) => store.open_inspector(key),
            StoreAction::ToggleHistoryDrawer => store.toggle_history_drawer(),
            StoreAction::ToggleSettingsDrawer => store.toggle_settings_drawer(),
            StoreAction::CloseDrawer => store.close_drawer(),
            StoreAction::SaveOptions(key, options) => {
                let id = key.id.clone();
                let step = self.store_edit(Then::StoreNotice(Notice::OptionsSaved(id)), |prefs| {
                    prefs.set_options(key, options)
                });
                if step == Step::Done {
                    self.power_store.set_notice(Notice::OptionsRejected);
                    return Step::Refresh;
                }
                return step;
            }
            StoreAction::SkipVersion(key, version) => {
                return self.store_edit(Then::Nothing, |prefs| {
                    prefs.filters.skip_version(key, version)
                });
            }
            StoreAction::Ignore(key) => {
                return self.store_edit(Then::Nothing, |prefs| prefs.filters.ignore(key));
            }
            StoreAction::Unignore(key) => {
                return self.store_edit(Then::Nothing, |prefs| {
                    prefs.filters.clear_ignore(&key);
                    true
                });
            }
            StoreAction::Unskip(key) => {
                return self.store_edit(Then::Nothing, |prefs| {
                    prefs.filters.clear_skip(&key);
                    true
                });
            }
            StoreAction::CopyId(id) => return Step::CopyText(id),
            StoreAction::DismissNotice => store.dismiss_notice(),
            StoreAction::StepCheckMinutes(up) => {
                return self.store_edit(Then::Reschedule, |prefs| {
                    prefs.check_minutes =
                        powerkit_core::power_store::step_check_minutes(prefs.check_minutes, up);
                    true
                });
            }
            StoreAction::SetNotify(notify) => {
                return self.store_edit(Then::Nothing, |prefs| {
                    let changed = prefs.notify != notify;
                    prefs.notify = notify;
                    changed
                });
            }
            StoreAction::SetRemoteIcons(enabled) => {
                let step = self.store_edit(Then::RemoteIcons, |prefs| {
                    let changed = prefs.remote_icons != enabled;
                    prefs.remote_icons = enabled;
                    changed
                });
                if step == Step::Done {
                    // Nothing to store; the list is reloaded all the same.
                    self.apply_remote_icons();
                }
                return step;
            }
            StoreAction::SetManagerEnabled(kind, enabled) => {
                return self.manager_edit(kind, true, |settings| settings.enabled = enabled);
            }
            StoreAction::OpenManagerDrawer(kind) => store.open_manager_drawer(kind),
            StoreAction::ShowSettingsDrawer => store.show_settings_drawer(),
            StoreAction::PickManagerExecutable(kind) => {
                return Step::Ask(Ask::ManagerProgram(kind));
            }
            StoreAction::ResetManagerExecutable(kind) => {
                return self.manager_edit(kind, true, |settings| settings.executable = None);
            }
            StoreAction::SetManagerScope(kind, scope) => {
                return self.manager_edit(kind, false, |settings| settings.scope = scope);
            }
            StoreAction::SetManagerArchitecture(kind, architecture) => {
                return self
                    .manager_edit(kind, false, |settings| settings.architecture = architecture);
            }
            StoreAction::SetManagerTriplet(kind, triplet) => {
                return self.manager_edit(kind, false, |settings| settings.triplet = triplet);
            }
            StoreAction::Export => return Step::Ask(Ask::ExportFile),
            StoreAction::Import => return Step::Ask(Ask::ImportFile),
            StoreAction::SetImport(key, selected) => store.set_import(&key, selected),
            StoreAction::ClearImport => store.clear_import(),
            StoreAction::InstallImport => {
                let packages = store.take_import_selection();
                let mut prefs = self.prefs.clone();
                let mut changed = false;
                for package in packages
                    .iter()
                    .filter(|package| !package.options.is_empty())
                {
                    changed |= prefs
                        .power_store
                        .set_options(package.key.clone(), package.options.clone());
                }
                if changed {
                    return Step::Save {
                        prefs: Box::new(prefs),
                        then: Then::InstallImport(packages),
                    };
                }
                self.enqueue_import(packages);
            }
            StoreAction::ClearHistory => store.clear_history(),
            StoreAction::RemovePending(index) => {
                store.remove_pending(index);
            }
        }
        Step::Done
    }
    /// The clipboard took the package id.
    pub fn copied(&mut self, id: String) {
        self.power_store.set_notice(Notice::Copied(id));
    }
    /// The answer to [`Ask::ManagerProgram`].
    pub fn program_picked(&mut self, kind: ManagerKind, program: PathBuf) -> Step {
        let program = program.display().to_string();
        self.manager_edit(kind, true, |settings| settings.executable = Some(program))
    }
    /// The answer to [`Ask::ExportFile`].
    pub fn export_to(&mut self, path: PathBuf) {
        self.power_store.export(path, &self.prefs.power_store);
    }
    /// The answer to [`Ask::ImportFile`]: reads the list and opens the import
    /// selection, or says the file could not be used.
    pub fn import_from(&mut self, path: PathBuf) {
        let parsed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| powerkit_core::power_store::parse_import(&text));
        match parsed {
            Some(packages) => self.power_store.load_import(packages),
            None => self
                .power_store
                .set_notice(Notice::ImportFailed(path.display().to_string())),
        }
    }

    // ---- what the UI and the tray show ----------------------------------

    /// The tray caption: what is running, shortest form.
    pub fn caption(t: &Strings, always_on: Option<&str>, pin: &Pin, ruler_active: bool) -> String {
        let always_on = match always_on {
            Some(status) => format!("PowerKit — {status}"),
            None => t.status_off.into(),
        };
        let mut text = match t.always_top_status(pin) {
            Some(status) => format!("{always_on} · {}: {status}", t.tray_always_top_on),
            None => always_on,
        };
        if ruler_active {
            text.push_str(" · ");
            text.push_str(t.ruler_on);
        }
        text
    }

    /// Everything a settings UI renders.
    pub fn snapshot(&self, readings: Readings) -> Snapshot {
        let prefs = &self.prefs;
        let t = readings.ui_language.strings();
        let top_status = match t.always_top_status(readings.pin) {
            Some(status) => format!("{} ({status})", t.tray_always_top_on),
            None => t.tray_always_top_off.into(),
        };
        let hint = if let Some(warning) = &self.warning {
            warning.clone()
        } else if readings.tray_available && prefs.show_tray {
            t.hint_tray.into()
        } else if readings.tray_available {
            t.hint_tray_hidden.into()
        } else {
            t.hint_no_tray.into()
        };
        let label = readings.hotkey_label;

        Snapshot {
            status: readings.status,
            mode: mode_index(readings.session),
            display: prefs.display,
            interval_minutes: prefs.interval_minutes,
            hotkey_notes: self.hotkey_notes(&readings.hotkey_problems, t),
            always_on_hotkey: label(Slot::AlwaysOn),
            always_on_warn: prefs.always_on_warn,
            ac_only: prefs.always_on_battery.ac_only,
            battery_floor: prefs.always_on_battery.floor,
            bound_app: readings.bound_app,
            top_status,
            sound: prefs.sound,
            capturing: self.capturing.map(|capturing| match capturing {
                Capturing::Hotkey(slot) => CaptureTarget::Hotkey(Command::capture(slot)),
                Capturing::MouseCustom(target) => CaptureTarget::MouseCustom(target),
                Capturing::MouseAddFKey => CaptureTarget::MouseAddFKey,
            }),
            pin_hotkey: label(Slot::Pin),
            opacity_up_hotkey: label(Slot::OpacityUp),
            opacity_down_hotkey: label(Slot::OpacityDown),
            frame_accent: prefs.frame_color.is_none(),
            frame_color: prefs.frame_color.unwrap_or(readings.accent),
            frame_opacity: prefs.frame_opacity,
            frame_thickness: prefs.frame_thickness,
            ruler_active: readings.ruler_active,
            ruler_hotkey: label(Slot::Ruler),
            ruler_mode: prefs.ruler_mode,
            ruler_unit: prefs.ruler_unit,
            ruler_continuous: prefs.ruler_continuous,
            ruler_per_channel: prefs.ruler_per_channel,
            ruler_feet: prefs.ruler_feet,
            ruler_tolerance: prefs.ruler_tolerance,
            ruler_color: prefs.ruler_color,
            resize: prefs.resize.clone(),
            startup: prefs.startup,
            theme: prefs.theme,
            language: prefs.language,
            show_tray: prefs.show_tray,
            shell_opacity: prefs.shell_opacity,
            rail_accent: readings.rail_accent,
            ui_language: readings.ui_language,
            hint,
            power_store: self.power_store.snapshot(&prefs.power_store),
            media_renamer: self.media_renamer.snapshot(&prefs.rename),
            mouse: prefs.mouse.clone(),
            mouse_elevated: readings.mouse_elevated,
            mouse_devices: readings.mouse_devices,
            mouse_adapters: readings.mouse_adapters,
            mouse_assets_rev: readings.mouse_assets_rev,
            modules: prefs.modules,
            tray: prefs.tray,
            unavailable: readings.unavailable.to_vec(),
            packaged: readings.packaged,
            version: readings.version,
            meta_key: readings.meta_key,
            hero_phase: readings.hero_phase,
        }
    }
}

/// "Tool · shortcut" for naming the slot that holds a chord.
fn hotkey_name(t: &Strings, slot: Slot) -> String {
    let (tool, label) = match slot {
        Slot::Pin => (t.nav_always_top, t.hotkey_pin),
        Slot::OpacityUp => (t.nav_always_top, t.hotkey_op_up),
        Slot::OpacityDown => (t.nav_always_top, t.hotkey_op_down),
        Slot::Ruler => (t.nav_screen_ruler, t.hotkey_ruler),
        Slot::AlwaysOn => (t.nav_always_on, t.hotkey_always_on),
    };
    format!("{tool} · {label}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{FakeCatalog, FakeMedia};
    use powerkit_core::power_mouse::{Action, ActionSlot, Binding, Trigger};
    use std::sync::Arc;

    struct Bench {
        app: App,
        media: Arc<FakeMedia>,
    }
    fn bench() -> Bench {
        let prefs = Preferences::default();
        let media = Arc::new(FakeMedia::default());
        let renamer = media_renamer::Session::new(media.clone(), Arc::new(|| {}), &prefs.rename);
        let store = power_store::Session::new(Arc::new(Arc::new(FakeCatalog::default())));
        Bench {
            app: App::new(prefs, store, renamer),
            media,
        }
    }
    /// Stores what a step asked for, as a shell would.
    fn save(app: &mut App, step: Step) {
        let Step::Save { prefs, then } = step else {
            panic!("expected a save, got {step:?}");
        };
        app.adopt(*prefs);
        app.saved(then);
    }
    fn settle(app: &mut App) {
        let start = Instant::now();
        while app.media_renamer.working() {
            assert!(start.elapsed() < Duration::from_secs(20), "did not settle");
            app.media_renamer.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn chord(key: KeyCode) -> Hotkey {
        Hotkey::new(false, true, true, false, key).unwrap()
    }

    #[test]
    fn a_request_for_a_tool_that_is_off_means_nothing() {
        let mut app = bench().app;
        app.prefs.modules.always_top = false;
        assert_eq!(
            app.intent(Intent::Command(Command::TogglePin)),
            Step::Done.into()
        );
        assert_eq!(
            app.intent(Intent::FrameColor(Rgb::new(1, 2, 3))),
            Step::Done.into()
        );
        // Recording a shortcut of that tool does not start either.
        assert_eq!(
            app.intent(Intent::Command(Command::CapturePinHotkey)),
            Step::Done.into()
        );
        assert_eq!(app.capturing(), None);
        // The tool's own switch and app-wide requests still work.
        assert!(matches!(
            app.intent(Intent::Command(Command::ToggleModuleAlwaysTop))
                .step,
            Step::Commit(_)
        ));
        assert_eq!(
            app.intent(Intent::Command(Command::Exit)),
            Step::Exit.into()
        );
    }

    #[test]
    fn a_shortcut_acts_on_the_window_in_front_and_only_while_its_tool_is_on() {
        let mut app = bench().app;
        let press = |app: &mut App, slot| app.intent(Intent::Shortcut(slot)).step;
        assert_eq!(
            press(&mut app, Slot::Pin),
            Step::Tool(ToolRequest::ToggleForegroundPin)
        );
        assert_eq!(
            press(&mut app, Slot::OpacityUp),
            Step::Tool(ToolRequest::StepForegroundOpacity(OPACITY_STEP))
        );
        assert_eq!(
            press(&mut app, Slot::OpacityDown),
            Step::Tool(ToolRequest::StepForegroundOpacity(-OPACITY_STEP))
        );
        assert_eq!(
            press(&mut app, Slot::Ruler),
            Step::Tool(ToolRequest::ToggleRuler)
        );
        assert_eq!(
            press(&mut app, Slot::AlwaysOn),
            Step::Tool(ToolRequest::ToggleAlwaysOn)
        );
        // A shortcut that is still registered when its tool goes off does nothing.
        app.prefs.modules.always_top = false;
        app.prefs.modules.always_on = false;
        assert_eq!(press(&mut app, Slot::Pin), Step::Done);
        assert_eq!(press(&mut app, Slot::OpacityUp), Step::Done);
        assert_eq!(press(&mut app, Slot::AlwaysOn), Step::Done);
        assert_eq!(
            press(&mut app, Slot::Ruler),
            Step::Tool(ToolRequest::ToggleRuler)
        );
    }

    #[test]
    fn only_a_running_custom_interval_starts_over_with_the_new_length() {
        let mut app = bench().app;
        app.prefs.interval_minutes = 45;
        let custom = Session::interval(1_000, 20, true);
        assert_eq!(
            app.interval_restart(&custom, 9_000),
            Some(Session::interval(9_000, 45, true))
        );
        let preset = Session::interval(1_000, 30, false);
        assert_eq!(app.interval_restart(&preset, 9_000), None);
        assert_eq!(app.interval_restart(&Session::Off, 9_000), None);
        assert_eq!(app.interval_restart(&Session::Indefinite, 9_000), None);
    }

    #[test]
    fn the_timer_leaves_a_switched_off_workspace_alone() {
        let Bench { mut app, media } = bench();
        media.add(
            std::path::Path::new("fake-media/a.jpg"),
            Some((2024, 5, 1, 10, 0, 0)),
        );
        app.media_renamer.add(vec![PathBuf::from("fake-media")]);
        app.prefs.modules.media_renamer = false;
        app.prefs.modules.power_store = false;
        let start = Instant::now();
        while app.media_renamer.working() && start.elapsed() < Duration::from_millis(200) {
            assert_eq!(app.tick(Instant::now()), Tick::default());
            std::thread::sleep(Duration::from_millis(2));
        }
        // Switched on again, the same timer takes the scan in.
        app.prefs.modules.media_renamer = true;
        let start = Instant::now();
        let mut changed = false;
        while app.media_renamer.working() || !changed {
            assert!(start.elapsed() < Duration::from_secs(20), "did not settle");
            changed |= app.tick(Instant::now()).changed;
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(app.tick(Instant::now()), Tick::default());
    }

    #[test]
    fn commands_become_edits_tool_requests_or_places_to_go() {
        let mut app = bench().app;
        let step = |app: &mut App, command| app.intent(Intent::Command(command)).step;
        assert_eq!(
            step(&mut app, Command::ToggleSound),
            Step::Commit(Edit::ToggleSound)
        );
        assert_eq!(
            step(&mut app, Command::TogglePin),
            Step::Tool(ToolRequest::TogglePin)
        );
        assert_eq!(
            step(&mut app, Command::OpacityDown),
            Step::Tool(ToolRequest::StepOpacity(-OPACITY_STEP))
        );
        assert_eq!(
            step(&mut app, Command::AlwaysOnHour),
            Step::Tool(ToolRequest::StartAlwaysOn(Choice::Preset(60)))
        );
        // The expiry radio needs its date and time; alone it starts nothing.
        assert_eq!(step(&mut app, Command::AlwaysOnExpiry), Step::Done);
        assert_eq!(
            step(&mut app, Command::OpenColorSettings),
            Step::Open(Destination::SystemColors)
        );
        assert_eq!(
            step(&mut app, Command::OpenScreenRulerPage),
            Step::ShowPage(Page::ScreenRuler)
        );
        assert_eq!(step(&mut app, Command::OpenSettings), Step::Present);
        assert_eq!(
            step(&mut app, Command::CaptureRulerHotkey),
            Step::BeginCapture
        );
        assert_eq!(app.capturing(), Some(Capturing::Hotkey(Slot::Ruler)));
        // PowerStore commands change the session and ask for a redraw.
        assert_eq!(
            app.intent(Intent::Command(Command::PowerStoreCancelPending)),
            Reply {
                step: Step::Done,
                refresh: true
            }
        );
        assert_eq!(
            app.intent(Intent::Command(Command::PowerStoreOpen)).step,
            Step::ShowPage(Page::PowerStore)
        );
    }

    #[test]
    fn values_from_the_ui_are_checked_before_they_are_stored() {
        let mut app = bench().app;
        assert_eq!(
            app.intent(Intent::Interval(45)).step,
            Step::Tool(ToolRequest::SetInterval(45))
        );
        assert_eq!(
            app.intent(Intent::ShellOpacity(80)).step,
            Step::Commit(Edit::ShellOpacity(80))
        );
        // A filename pattern that does not parse drops the UI's draft.
        let valid = app.prefs.resize.filename.clone();
        assert!(matches!(
            app.intent(Intent::ResizeFilename(valid)).step,
            Step::CommitDraft(Edit::ResizeFilename(_))
        ));
        assert_eq!(
            app.intent(Intent::ResizeFilename(String::new())).step,
            Step::Refresh
        );
        // Page actions always redraw, whatever they led to.
        assert!(
            app.intent(Intent::PowerStore(StoreAction::ClearSelection))
                .refresh
        );
        assert!(
            app.intent(Intent::MediaRenamer(RenameAction::Clear))
                .refresh
        );
    }

    #[test]
    fn the_ring_shows_what_each_slot_does() {
        let mut app = bench().app;
        let t = UiLanguage::English.strings();
        assert!(app.ring_labels(t, "Meta").iter().all(Option::is_none));
        let key = chord(KeyCode::F13);
        assert!(
            app.prefs
                .mouse
                .set_binding(Trigger::RING[0], Binding::Single(Action::Custom(key)))
        );
        let labels = app.ring_labels(t, "Meta");
        assert_eq!(labels.len(), Trigger::RING.len());
        assert_eq!(labels[0], Some(key.label("Meta")));
        assert_eq!(labels[1], None);
    }

    #[test]
    fn a_recorded_shortcut_is_committed_and_a_held_one_is_refused() {
        let mut app = bench().app;
        assert_eq!(app.finish_capture(chord(KeyCode::F13)), Captured::Ignored);
        app.begin_capture(Capturing::Hotkey(Slot::Ruler));
        let fresh = chord(KeyCode::F13);
        assert_eq!(
            app.finish_capture(fresh),
            Captured::Commit {
                edit: Edit::Hotkey(Slot::Ruler, fresh),
                cancel_if_unchanged: false
            }
        );
        // The chord another slot holds ends the recording and is remembered
        // for the note under the card.
        let held = Slot::Pin.chord(&app.prefs);
        assert_eq!(app.finish_capture(held), Captured::Cancel);
        assert!(app.hotkey_rejected().is_some());
        assert!(app.cancel_capture());
        assert!(!app.cancel_capture());
        // A new recording forgets the refusal.
        app.begin_capture(Capturing::Hotkey(Slot::Ruler));
        assert_eq!(app.hotkey_rejected(), None);
    }

    #[test]
    fn mouse_recordings_become_edits_that_end_quietly_when_unchanged() {
        let mut app = bench().app;
        // A recording fills a slot that exists: the function key is listed first.
        assert!(
            app.prefs
                .mouse
                .set_binding(Trigger::F13, Binding::Single(Action::None))
        );
        let target = MouseSlot {
            trigger: Trigger::F13,
            slot: ActionSlot::Single,
            step: None,
        };
        assert_eq!(
            app.power_mouse(MouseAction::BeginCustomCapture(target)),
            Step::BeginCapture
        );
        assert_eq!(app.capturing(), Some(Capturing::MouseCustom(target)));
        let key = chord(KeyCode::F13);
        let Captured::Commit {
            edit: Edit::MouseBinding { trigger, binding },
            cancel_if_unchanged: true,
        } = app.finish_capture(key)
        else {
            panic!("a mouse recording commits a binding");
        };
        assert_eq!(trigger, target.trigger);
        assert_eq!(binding, Binding::Single(Action::Custom(key)));

        // Adding a function key takes one key, and only a function key.
        assert_eq!(
            app.power_mouse(MouseAction::BeginAddFKey),
            Step::BeginCapture
        );
        assert_eq!(
            app.finish_function_key(Trigger::F14.key().unwrap()),
            Captured::Commit {
                edit: Edit::MouseAddFunctionKey(Trigger::F14),
                cancel_if_unchanged: true
            }
        );
        // A chord means nothing while a single key is awaited.
        assert_eq!(app.finish_capture(key), Captured::Cancel);
        // Plain edits go to the one commit path.
        assert_eq!(
            app.power_mouse(MouseAction::SetDpi(Some(1600))),
            Step::Commit(Edit::MouseDpi(Some(1600)))
        );
    }

    #[test]
    fn adopting_preferences_ends_a_recording_and_clears_the_warning() {
        let mut app = bench().app;
        app.warning = Some("settings were reset".into());
        app.begin_capture(Capturing::MouseAddFKey);
        let mut prefs = app.prefs.clone();
        prefs.sound = !prefs.sound;
        app.adopt(prefs.clone());
        assert_eq!(app.prefs, prefs);
        assert_eq!(app.capturing(), None);
        assert_eq!(app.warning, None);
    }

    #[test]
    fn store_settings_are_saved_only_when_they_change() {
        let mut app = bench().app;
        let icons = app.prefs.power_store.remote_icons;
        assert_eq!(
            app.power_store(StoreAction::SetRemoteIcons(icons)),
            Step::Done
        );
        let step = app.power_store(StoreAction::SetRemoteIcons(!icons));
        assert!(matches!(
            &step,
            Step::Save { prefs, then: Then::RemoteIcons } if prefs.power_store.remote_icons != icons
        ));
        // Nothing changes until the shell has stored the candidate.
        assert_eq!(app.prefs.power_store.remote_icons, icons);
        save(&mut app, step);
        assert_eq!(app.prefs.power_store.remote_icons, !icons);

        let before = app.prefs.power_store.check_minutes;
        let step = app.power_store(StoreAction::StepCheckMinutes(true));
        assert!(matches!(
            &step,
            Step::Save {
                then: Then::Reschedule,
                ..
            }
        ));
        save(&mut app, step);
        assert_ne!(app.prefs.power_store.check_minutes, before);
    }

    #[test]
    fn a_manager_switch_relists_and_a_default_scope_does_not() {
        let mut app = bench().app;
        let kind = ManagerKind::Scoop;
        let enabled = app.prefs.power_store.manager(kind).enabled;
        assert_eq!(
            app.power_store(StoreAction::SetManagerEnabled(kind, enabled)),
            Step::Done
        );
        let step = app.power_store(StoreAction::SetManagerEnabled(kind, !enabled));
        assert!(matches!(
            &step,
            Step::Save {
                then: Then::RelistManager(ManagerKind::Scoop),
                ..
            }
        ));
        save(&mut app, step);
        assert_eq!(app.prefs.power_store.manager(kind).enabled, !enabled);
        // A picked program is stored the same way.
        let step = app.program_picked(kind, PathBuf::from("scoop.cmd"));
        assert!(matches!(
            &step,
            Step::Save { prefs, then: Then::RelistManager(_) }
                if prefs.power_store.manager(kind).executable.as_deref() == Some("scoop.cmd")
        ));
    }

    #[test]
    fn requests_that_need_the_platform_say_so() {
        let mut app = bench().app;
        assert_eq!(
            app.power_store(StoreAction::PickManagerExecutable(ManagerKind::Pip)),
            Step::Ask(Ask::ManagerProgram(ManagerKind::Pip))
        );
        assert_eq!(
            app.power_store(StoreAction::Export),
            Step::Ask(Ask::ExportFile)
        );
        assert_eq!(
            app.power_store(StoreAction::Import),
            Step::Ask(Ask::ImportFile)
        );
        assert!(matches!(
            app.power_store(StoreAction::OpenStoreUpdates),
            Step::Open(Destination::StoreUpdates)
        ));
        assert_eq!(
            app.power_store(StoreAction::CopyId("Git.Git".into())),
            Step::CopyText("Git.Git".into())
        );
        // "Copied" is claimed only once the shell says the clipboard took it.
        assert_eq!(
            app.power_store.snapshot(&app.prefs.power_store).notice,
            None
        );
        app.copied("Git.Git".into());
        assert_eq!(
            app.power_store.snapshot(&app.prefs.power_store).notice,
            Some(Notice::Copied("Git.Git".into()))
        );
        assert_eq!(
            app.media_renamer(RenameAction::AddFiles),
            Step::Ask(Ask::MediaFiles)
        );
        assert_eq!(
            app.media_renamer(RenameAction::AddFolder),
            Step::Ask(Ask::MediaFolder)
        );
    }

    #[test]
    fn an_unreadable_import_is_reported_and_opens_nothing() {
        let mut app = bench().app;
        app.import_from(PathBuf::from("no-such-powerkit-import.json"));
        assert!(matches!(
            app.power_store.snapshot(&app.prefs.power_store).notice,
            Some(Notice::ImportFailed(_))
        ));
    }

    #[test]
    fn renamer_defaults_are_stored_and_the_extension_switch_reaches_the_session() {
        let mut app = bench().app;
        let on = !app.prefs.rename.lowercase_ext;
        let step = app.media_renamer(RenameAction::SetLowercaseExt(on));
        assert!(matches!(
            &step,
            Step::Save { prefs, then: Then::LowercaseExt(_) } if prefs.rename.lowercase_ext == on
        ));
        save(&mut app, step);
        assert_eq!(app.prefs.rename.lowercase_ext, on);
        let recurse = !app.prefs.rename.recurse;
        let step = app.media_renamer(RenameAction::SetDefaultRecurse(recurse));
        save(&mut app, step);
        assert_eq!(app.prefs.rename.recurse, recurse);
    }

    #[test]
    fn a_new_selection_replaces_the_workspace_only_when_nothing_would_be_lost() {
        let Bench { mut app, media } = bench();
        let dir = PathBuf::from("fake-media");
        media.add(&dir.join("a.jpg"), Some((2024, 5, 1, 10, 0, 0)));
        media.add(&dir.join("b.jpg"), Some((2024, 5, 2, 10, 0, 0)));
        let start = Instant::now();
        // An empty workspace takes the selection.
        assert_eq!(
            app.receive_rename(vec![dir.join("a.jpg")], start),
            Handoff::Show
        );
        settle(&mut app);
        assert_eq!(
            app.media_renamer.snapshot(&app.prefs.rename).summary.groups,
            1
        );
        // One second later is the same selection arriving item by item.
        assert_eq!(
            app.receive_rename(vec![dir.join("b.jpg")], start + Duration::from_secs(1)),
            Handoff::Show
        );
        settle(&mut app);
        assert_eq!(
            app.media_renamer.snapshot(&app.prefs.rename).summary.groups,
            2
        );
        // Later, with renames still unapplied, the user is asked first.
        let later = start + Duration::from_secs(60);
        let Handoff::Confirm(paths) = app.receive_rename(vec![dir.join("b.jpg")], later) else {
            panic!("unapplied renames need a confirmation");
        };
        assert_eq!(
            app.media_renamer.snapshot(&app.prefs.rename).summary.groups,
            2
        );
        app.replace_rename(paths, later);
        settle(&mut app);
        assert_eq!(
            app.media_renamer.snapshot(&app.prefs.rename).summary.groups,
            1
        );
    }

    #[test]
    fn the_caption_lists_what_is_running() {
        let t = UiLanguage::English.strings();
        let idle = App::caption(t, None, &Pin::default(), false);
        assert_eq!(idle, t.status_off);
        let busy = App::caption(t, Some("30 min left"), &Pin::default(), true);
        assert!(busy.starts_with("PowerKit — 30 min left"));
        assert!(busy.ends_with(t.ruler_on));
    }

    #[test]
    fn the_snapshot_reflects_recording_problems_and_the_tray_hint() {
        let mut app = bench().app;
        let t = UiLanguage::English;
        let pin = Pin::default();
        let readings = |tray_available: bool, problems: Vec<(Slot, Problem)>| Readings {
            status: "status".into(),
            ui_language: t,
            session: Session::Off,
            bound_app: None,
            pin: &pin,
            ruler_active: false,
            hotkey_label: &|slot| format!("{slot:?}"),
            hotkey_problems: problems,
            accent: Rgb::new(1, 2, 3),
            rail_accent: None,
            tray_available,
            mouse_elevated: false,
            mouse_devices: Vec::new(),
            mouse_adapters: BTreeMap::new(),
            mouse_assets_rev: 0,
            unavailable: &[],
            packaged: false,
            version: "1.2.3",
            meta_key: "Meta",
            hero_phase: 0,
        };
        let view = app.snapshot(readings(true, Vec::new()));
        assert_eq!(view.mode, Some(0));
        assert_eq!(view.pin_hotkey, "Pin");
        assert_eq!(view.ruler_hotkey, "Ruler");
        assert_eq!(view.hint, t.strings().hint_tray);
        assert_eq!(view.version, "1.2.3");
        assert_eq!(view.meta_key, "Meta");
        // The frame follows the system accent until a color is chosen.
        assert!(view.frame_accent);
        assert_eq!(view.frame_color, Rgb::new(1, 2, 3));
        assert_eq!(view.capturing, None);
        assert!(view.hotkey_notes.is_empty());

        app.warning = Some("careful".into());
        app.begin_capture(Capturing::Hotkey(Slot::Pin));
        let view = app.snapshot(readings(
            false,
            vec![
                (Slot::Ruler, Problem::Taken),
                (Slot::OpacityUp, Problem::Duplicate { holder: Slot::Pin }),
            ],
        ));
        assert_eq!(view.hint, "careful");
        assert_eq!(
            view.capturing,
            Some(CaptureTarget::Hotkey(Command::CapturePinHotkey))
        );
        assert_eq!(view.hotkey_notes[0].0, Command::CaptureRulerHotkey);
        assert_eq!(view.hotkey_notes[0].1, t.strings().hotkey_taken);
        // The note names the tool and shortcut that hold the chord.
        assert!(view.hotkey_notes[1].1.contains(t.strings().hotkey_pin));
        app.warning = None;
        assert_eq!(
            app.snapshot(readings(false, Vec::new())).hint,
            t.strings().hint_no_tray
        );
    }
}
