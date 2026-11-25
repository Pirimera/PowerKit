//! The application shell: the composition root on Windows.
//!
//! It owns the hidden app window, the tray icon, the preferences and the
//! message loop, turns intents and window messages into calls on the
//! `powerkit_app` controllers and the `powerkit_platform_windows` ports, and
//! publishes the snapshot the `powerkit_ui_reactor` UI renders. The platform
//! crate and the UI crate meet only here. All UI and power calls run on the
//! message-loop thread.
mod smoke;

use command::{Command, EXIT, MenuId, OPEN};
use icons::IconSet;
use power::PowerGuard;
use powerkit_app::controller::application::{
    App, Ask, Captured, Capturing, Destination, Handoff, Readings, Step, Tick, ToolRequest,
};
use powerkit_app::controller::hotkeys::Slot;
use powerkit_app::controller::settings::Edit;
use powerkit_app::event::Event;
use powerkit_app::intent::Intent;
use powerkit_app::ports::{Autostart, SettingsStore};
use powerkit_app::view::tray;
use powerkit_core::always_on::{Request, Session};
use powerkit_platform_windows::{
    Balloon, awake, clipboard, clock, color, command, events, fault, frame, hotkeys, icons,
    instance, intents, keys, language, media_renamer, menu, menu_hint, package, power, power_mouse,
    power_store, recovery, resize, ruler, settings_file, startup, theme, top, ui_strings, wide,
};
pub use powerkit_platform_windows::{report, report_error};
use powerkit_ui_reactor::{resize_dialog, window, winui_host};
use std::{
    cell::{Cell, RefCell},
    io,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        LibraryLoader::*,
        Power::*,
        RemoteDesktop::{
            NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
            WTSUnRegisterSessionNotification,
        },
        SystemInformation::*,
        Threading::*,
    },
    UI::{Input::KeyboardAndMouse::VK_ESCAPE, Shell::*, WindowsAndMessaging::*},
};
const CALLBACK: u32 = WM_APP + 1;
const SMOKE: usize = 2;
const POLL_TIMER: usize = 3;
/// Beside settings.txt so history shares its location without bloating settings writes.
const POWER_STORE_HISTORY_FILE: &str = "power-store-history.txt";
const POWER_STORE_INSTALLED_FILE: &str = "power-store-installed.txt";
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    /// Set while a logon/tray start waits for the first settings open to load WinUI.
    static SHELL_REQUESTED: Cell<bool> = const { Cell::new(false) };
}
struct State {
    taskbar_message: u32,
    balloon: Balloon,
    tray_available: bool,
    smoke: bool,
    fault: Option<String>,
    settings_unavailable: bool,
    /// True after a logon/tray start until the WinUI settings shell is started.
    shell_deferred: bool,
    last_caption: String,
    smoke_stage: u8,
    icons: IconSet,
    /// Preferences, the PowerStore and MediaRenamer sessions and the
    /// key-recording state, with the decisions made on them.
    app: App,
    /// Explorer selection received by WM_COPYDATA, confirmed on `handoff::HANDOFF`.
    rename_handoff: Option<Vec<PathBuf>>,
}
impl Default for State {
    /// Until the app window exists the MediaRenamer workspace is idle, with
    /// nobody to wake.
    fn default() -> Self {
        let prefs = powerkit_core::settings::Preferences::default();
        let renamer = media_renamer::session(null_mut(), &prefs.rename);
        Self {
            taskbar_message: 0,
            balloon: Balloon::default(),
            tray_available: false,
            smoke: false,
            fault: None,
            settings_unavailable: false,
            shell_deferred: false,
            last_caption: String::new(),
            smoke_stage: 0,
            icons: IconSet::default(),
            app: App::new(prefs, power_store::session(), renamer),
            rename_handoff: None,
        }
    }
}
fn with_state<R>(f: impl FnOnce(&State) -> R) -> R {
    STATE.with(|s| f(&s.borrow()))
}
fn with_state_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}
fn shell_is_deferred() -> bool {
    with_state(|s| s.shell_deferred)
}
fn settings_path() -> io::Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or(fault::Fault::SettingsLocation)?)
            .join("PowerKit")
            .join("settings.txt"),
    )
}
fn load_prefs() -> io::Result<powerkit_core::settings::Loaded> {
    settings_file::load(&settings_path()?)
}
fn save_prefs(prefs: powerkit_core::settings::Preferences) -> io::Result<()> {
    settings_file::save(&settings_path()?, &prefs)
}
/// The settings file and the startup registration as the `powerkit_app` ports.
/// The smoke test never writes the user's file.
struct Disk {
    smoke: bool,
}
impl Disk {
    fn current() -> Self {
        Self {
            smoke: with_state(|s| s.smoke),
        }
    }
}
impl SettingsStore for Disk {
    fn save(&mut self, prefs: &powerkit_core::settings::Preferences) -> io::Result<()> {
        if self.smoke {
            return Ok(());
        }
        save_prefs(prefs.clone())
    }
}
impl Autostart for Disk {
    fn set_autostart(&mut self, enabled: bool) -> io::Result<()> {
        startup::set_enabled(enabled)
    }
}
/// Saves and applies preferences.
fn persist(hwnd: HWND, prefs: powerkit_core::settings::Preferences) -> io::Result<()> {
    Disk::current().save(&prefs)?;
    adopt(hwnd, prefs);
    Ok(())
}
/// Makes saved preferences the running ones: state, tools, shell tint and view.
fn adopt(hwnd: HWND, prefs: powerkit_core::settings::Preferences) {
    with_state_mut(|s| {
        s.app.adopt(prefs.clone());
        powerkit_platform_windows::set_language(prefs.language);
    });
    winui_host::end_key_capture();
    apply_feature_prefs(hwnd, &prefs);
    winui_host::apply_shell_opacity(prefs.shell_opacity);
    update_view(hwnd, true);
}
/// Commits one preference edit and carries out what follows from it: the
/// Explorer command labels after a language change, the tray icon after its
/// switch, and leaving the page of a tool that was just switched off.
fn apply_edit(hwnd: HWND, edit: Edit) -> io::Result<()> {
    commit_edit(hwnd, edit).map(|_| ())
}
/// [`apply_edit`], reporting whether the edit changed and wrote anything.
fn commit_edit(hwnd: HWND, edit: Edit) -> io::Result<bool> {
    let tool_switch = matches!(edit, Edit::ToggleTool(_));
    let current = with_state(|s| s.app.prefs.clone());
    let Some(change) =
        powerkit_app::controller::settings::commit(&mut Disk::current(), &current, edit)?
    else {
        return Ok(false);
    };
    let page = window::current_page();
    let leave =
        tool_switch && !page.module_enabled(change.prefs.modules, change.prefs.mouse.enabled);
    adopt(hwnd, change.prefs);
    if leave {
        window::show_page(window::Page::General);
    }
    if change.language {
        resize::register_context_menu();
    }
    if change.show_tray {
        apply_tray_preference(hwnd)?;
        update_view(hwnd, true);
    }
    Ok(true)
}
/// Commits an edit made by recording a key; a recording that changes nothing
/// just ends.
fn commit_capture(hwnd: HWND, edit: Edit) {
    match commit_edit(hwnd, edit) {
        Ok(true) => {}
        Ok(false) => cancel_capture(hwnd),
        Err(error) => report_error(&error),
    }
}
fn apply_feature_prefs(hwnd: HWND, prefs: &powerkit_core::settings::Preferences) {
    // Workers read the manager switches and program paths, so publish them first.
    power_store::managers().configure(&prefs.power_store);
    if prefs.modules.always_top {
        top::apply_prefs(prefs);
    } else {
        let _ = top::release();
    }
    if prefs.modules.screen_ruler {
        ruler::apply_prefs(prefs);
    } else {
        ruler::suspend();
    }
    if prefs.modules.always_on {
        awake::apply_prefs(hwnd, prefs);
    } else {
        let _ = awake::start(hwnd, powerkit_core::always_on::Session::Off);
    }
    power_mouse::apply_prefs(prefs);
    // A tool that is off holds no chord; one switched back on gets it again.
    hotkeys::sync(prefs);
}

/// Starts listening for the key the application is now waiting for.
fn start_capture(hwnd: HWND) {
    hotkeys::pause();
    winui_host::begin_key_capture();
    update_view(hwnd, true);
}
fn cancel_capture(hwnd: HWND) {
    if !with_state_mut(|s| s.app.cancel_capture()) {
        return;
    }
    winui_host::end_key_capture();
    let prefs = with_state(|s| s.app.prefs.clone());
    apply_feature_prefs(hwnd, &prefs);
    update_view(hwnd, true);
}
/// Carries out what the application decided about a recorded key.
fn captured(hwnd: HWND, outcome: Captured) {
    match outcome {
        Captured::Ignored => {}
        Captured::Cancel => cancel_capture(hwnd),
        Captured::Commit {
            edit,
            cancel_if_unchanged: true,
        } => commit_capture(hwnd, edit),
        Captured::Commit { edit, .. } => {
            let _ = apply_edit(hwnd, edit).map_err(|error| report_error(&error));
        }
    }
}
fn finish_capture(hwnd: HWND, key: powerkit_core::hotkey::Hotkey) {
    let outcome = with_state_mut(|s| s.app.finish_capture(key));
    captured(hwnd, outcome);
}
fn finish_f_key_capture(hwnd: HWND, vk: u16) {
    let Some(key) = keys::from_vk(vk) else {
        return;
    };
    let outcome = with_state_mut(|s| s.app.finish_function_key(key));
    captured(hwnd, outcome);
}
fn read_capture(vk: u16, mods: isize) -> Option<powerkit_core::hotkey::Hotkey> {
    powerkit_core::hotkey::Hotkey::new(
        mods & winui_host::MOD_WIN != 0,
        mods & winui_host::MOD_CTRL != 0,
        mods & winui_host::MOD_ALT != 0,
        mods & winui_host::MOD_SHIFT != 0,
        keys::from_vk(vk)?,
    )
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // A restarted PowerKit empties the handle while it reopens the name.
        if self.0.is_null() {
            return;
        }
        // SAFETY: Handle owns the non-null mutex handle returned by CreateMutexW.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn tooltip() -> String {
    let t = ui_strings();
    let always_on = awake::session().active().then(|| awake::status(t));
    App::caption(t, always_on.as_deref(), &top::session(), ruler::active())
}
fn current_resize() -> powerkit_core::image_resizer::ResizeSettings {
    with_state(|s| s.app.prefs.resize.clone())
}
/// The app window as the settings UI's host.
struct ShellHost(HWND);
impl powerkit_app::ui::Host for ShellHost {
    fn send(&self, intent: Intent) {
        intents::send(self.0, intent);
    }
    fn start_resize(&self, job: powerkit_app::controller::image_resizer::Job) {
        resize::spawn(self.0, job);
    }
    fn mouse_image(
        &self,
        depot: &'static str,
        kind: powerkit_app::ui::MouseImageKind,
    ) -> Option<powerkit_app::ui::DeviceImage> {
        power_mouse::assets::cached(depot, kind)
    }
}
fn shell_host(hwnd: HWND) -> powerkit_app::ui::HostHandle {
    powerkit_app::ui::HostHandle::new(ShellHost(hwnd))
}
/// Shows the settings window, or explains why it cannot exist on this machine.
/// After a tray/logon start the WinUI shell is not loaded yet: the first open
/// wakes the Win32 pump so `window::run` can start.
/// Opens the Image Resizer dialog for files another launch handed over.
fn open_pending_resize(hwnd: HWND) {
    if let Some(files) = resize::take_pending() {
        open_resize(hwnd, files, current_resize());
    }
}
/// Opens the WinUI dialog, asking for files first when none were given. Without the
/// settings shell (missing Windows App Runtime) there is no dialog to open.
fn open_resize(
    hwnd: HWND,
    mut files: Vec<PathBuf>,
    settings: powerkit_core::image_resizer::ResizeSettings,
) {
    if files.is_empty() {
        files = resize::pick(hwnd);
        if files.is_empty() {
            return;
        }
    }
    if window::open_resize(shell_host(hwnd), files.clone(), settings.clone()) {
        return;
    }
    // Tray/logon starts defer WinUI; queue the dialog and wake the shell.
    if shell_is_deferred() {
        resize_dialog::queue(resize_dialog::Seed {
            host: shell_host(hwnd),
            files,
            settings,
        });
        request_shell(hwnd);
        return;
    }
    report(ui_strings().settings_unavailable);
}
fn present(hwnd: HWND) {
    if winui_host::present() {
        return;
    }
    if with_state(|s| s.shell_deferred) {
        request_shell(hwnd);
        return;
    }
    if with_state(|s| s.settings_unavailable && !s.smoke) {
        report(ui_strings().settings_unavailable);
    }
}

/// Ends a deferred tray pump so the WinUI settings shell can start.
fn request_shell(hwnd: HWND) {
    SHELL_REQUESTED.set(true);
    // SAFETY: hwnd is the live app window on this thread; WM_NULL only wakes GetMessage.
    unsafe {
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
}
fn mark_tray_unavailable(hwnd: HWND) {
    let smoke = with_state_mut(|s| {
        s.tray_available = false;
        s.smoke
    });
    if !smoke {
        present(hwnd);
    }
}
fn open_media_renamer(hwnd: HWND) {
    open_page(hwnd, window::Page::MediaRenamer);
}
fn open_page(hwnd: HWND, page: window::Page) {
    window::show_page(page);
    update_view(hwnd, true);
    present(hwnd);
}
/// Explorer input replaces the preview; unapplied renames are only dropped after a yes.
fn receive_rename_handoff(hwnd: HWND) {
    let Some(paths) = with_state_mut(|s| s.rename_handoff.take()) else {
        return;
    };
    let now = std::time::Instant::now();
    if let Handoff::Confirm(paths) = with_state_mut(|s| s.app.receive_rename(paths, now)) {
        let t = ui_strings();
        let title = wide(t.rename.replace_title);
        let body = wide(t.rename.replace_body);
        let owner = winui_host::settings().unwrap_or(hwnd);
        // SAFETY: both UTF-16 buffers outlive the modal call; no STATE borrow is held.
        let answer = unsafe {
            MessageBoxW(
                owner,
                body.as_ptr(),
                title.as_ptr(),
                MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND,
            )
        };
        if answer == IDYES {
            with_state_mut(|s| s.app.replace_rename(paths, std::time::Instant::now()));
        }
    }
    open_media_renamer(hwnd);
}
/// Does what only the shell can do for a step the application decided on. No
/// STATE borrow is held here: a dialog pumps this thread's messages.
fn run_step(hwnd: HWND, step: Step) -> io::Result<()> {
    match step {
        Step::Done => Ok(()),
        Step::Save { prefs, then } => {
            persist(hwnd, *prefs)?;
            with_state_mut(|s| s.app.saved(then));
            Ok(())
        }
        Step::Commit(edit) => apply_edit(hwnd, edit),
        Step::Ask(ask) => answer(hwnd, ask),
        Step::Open(destination) => {
            open_shell_uri(match destination {
                Destination::StoreUpdates => "ms-windows-store://downloadsandupdates",
                Destination::SystemColors => "ms-settings:colors",
            });
            Ok(())
        }
        Step::CommitDraft(edit) => {
            if let Err(error) = apply_edit(hwnd, edit) {
                report_error(&error);
                window::refresh();
            }
            Ok(())
        }
        Step::Present => {
            present(hwnd);
            Ok(())
        }
        Step::ShowPage(page) => {
            open_page(hwnd, page);
            Ok(())
        }
        Step::Tool(request) => run_tool(hwnd, request),
        Step::Exit => {
            // SAFETY: hwnd is this thread's app window; destroying it ends the loop.
            unsafe {
                DestroyWindow(hwnd);
            }
            Ok(())
        }
        Step::CopyText(text) => {
            // Claim "Copied" only when the clipboard really holds the text.
            if clipboard::set_text(hwnd, &text).is_ok() {
                with_state_mut(|s| s.app.copied(text));
            }
            Ok(())
        }
        Step::Refresh => {
            window::refresh();
            Ok(())
        }
        Step::BeginCapture => {
            start_capture(hwnd);
            Ok(())
        }
    }
}
/// Puts the application's question to the user and hands back the answer.
fn answer(hwnd: HWND, ask: Ask) -> io::Result<()> {
    let owner = winui_host::settings().unwrap_or(null_mut());
    let t = ui_strings();
    match ask {
        Ask::ManagerProgram(kind) => {
            let picked = power_store::file_dialog::open_program(owner, t.power_store_manager_pick);
            match picked {
                Some(program) => {
                    let step = with_state_mut(|s| s.app.program_picked(kind, program));
                    run_step(hwnd, step)
                }
                None => Ok(()),
            }
        }
        Ask::ExportFile => {
            let picked = power_store::file_dialog::save_json(
                owner,
                t.power_store_export,
                "powerkit-packages.json",
            );
            if let Some(path) = picked {
                with_state_mut(|s| s.app.export_to(path));
            }
            Ok(())
        }
        Ask::ImportFile => {
            if let Some(path) = power_store::file_dialog::open_json(owner, t.power_store_import) {
                with_state_mut(|s| s.app.import_from(path));
            }
            Ok(())
        }
        Ask::MediaFiles => {
            let paths =
                media_renamer::pick::files(owner, t.rename.pick_files, t.rename.pick_filter);
            with_state_mut(|s| s.app.media_picked(paths));
            Ok(())
        }
        Ask::MediaFolder => {
            if let Some(path) = media_renamer::pick::folder(owner, t.rename.pick_folder) {
                with_state_mut(|s| s.app.media_picked(vec![path]));
            }
            Ok(())
        }
    }
}
fn update_view(hwnd: HWND, force: bool) {
    let caption = tooltip();
    let (changed, available) = with_state_mut(|s| {
        let changed = force || s.last_caption != caption;
        s.last_caption = caption.clone();
        (changed, s.tray_available)
    });
    if !changed {
        return;
    }
    if available && tray(hwnd, NIM_MODIFY, force).is_err() {
        mark_tray_unavailable(hwnd);
    }
    window::publish(snapshot(caption));
}
fn snapshot(status: String) -> window::Snapshot {
    let pin = top::session();
    with_state(|s| {
        let prefs = &s.app.prefs;
        // Smoke runs never register, and a recording pauses every shortcut.
        let quiet = s.smoke || s.app.capturing().is_some();
        s.app.snapshot(Readings {
            status,
            ui_language: language::resolve(prefs.language),
            session: awake::session(),
            bound_app: awake::bound_app(),
            pin: &pin,
            ruler_active: ruler::active(),
            hotkey_label: &|slot| hotkeys::label(slot, prefs),
            hotkey_problems: hotkeys::problems(prefs, s.app.hotkey_rejected(), quiet),
            accent: color::from_colorref(frame::accent_color()),
            rail_accent: theme::shell_accent(prefs.theme).map(color::from_colorref),
            tray_available: s.tray_available,
            mouse_elevated: power_mouse::elevated_foreground(),
            mouse_devices: power_mouse::connected_devices(),
            mouse_adapters: power_mouse::adapter_statuses(),
            mouse_assets_rev: power_mouse::assets::revision(),
            unavailable: powerkit_platform_windows::UNAVAILABLE_TOOLS,
            packaged: package::is_packaged(),
            version: display_version(),
            meta_key: keys::META_KEY,
            hero_phase: window::hero_phase(),
        })
    })
}
/// The product version; a local test package appends its MSIX revision.
fn display_version() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| match package::revision() {
        0 => env!("CARGO_PKG_VERSION").to_string(),
        revision => format!("{}.{revision}", env!("CARGO_PKG_VERSION")),
    })
}
fn fail_closed(hwnd: HWND, error: io::Error) {
    let message = error.to_string();
    with_state_mut(|s| {
        s.fault = Some(message);
        s.app.power_store.shutdown();
        s.app.media_renamer.shutdown();
    });
    // Release BEFORE displaying any modal error dialog.
    hotkeys::shutdown();
    let _ = awake::shutdown(hwnd);
    top::shutdown();
    ruler::shutdown();
    power_mouse::shutdown();
    unsafe {
        DestroyWindow(hwnd);
    }
    // main displays the saved error after cleanup.
}
/// Applies a command-line AlwaysOn request from this or a later instance.
fn apply_request(hwnd: HWND, request: Request) {
    if let Err(error) = awake::request(hwnd, request) {
        report_error(&error);
    }
}
fn start_expiry(hwnd: HWND, pick: window::ExpiryPick) -> io::Result<()> {
    let now = clock::now_utc();
    let session = clock::minute_of_day(pick.time_of_day)
        .and_then(|minute| clock::expiry_target(pick.date, minute, now))
        .and_then(|target| Session::expiry(awake::uptime_ms(), now, target));
    match session {
        Some(session) => awake::start(hwnd, session),
        None => Err(io::Error::other(ui_strings().expiry_past)),
    }
}
fn apply_interval(hwnd: HWND, minutes: u32) -> io::Result<()> {
    if !commit_edit(hwnd, Edit::Interval(minutes))? {
        return Ok(());
    }
    let running = awake::session();
    match with_state(|s| s.app.interval_restart(&running, awake::uptime_ms())) {
        Some(session) => awake::start(hwnd, session),
        None => Ok(()),
    }
}
/// Toggles the display preference: power first, then the file, undoing power if saving fails.
fn toggle_display(hwnd: HWND) -> io::Result<()> {
    let mut prefs = with_state(|s| s.app.prefs.clone());
    let previous = prefs.display;
    awake::set_display(hwnd, !previous)?;
    prefs.display = !previous;
    persist(hwnd, prefs).inspect_err(|_| {
        if let Err(error) = awake::set_display(hwnd, previous) {
            let t = ui_strings();
            report(&format!(
                "{} ({})",
                t.err_display_rollback,
                fault::describe(&error, t)
            ));
        }
    })
}
fn tray_icon(hwnd: HWND, refresh: bool) -> isize {
    if !refresh {
        let existing = with_state(|s| s.icons.tray);
        if existing != 0 {
            return existing;
        }
    }
    let mode = awake::session().mode();
    let previous = with_state(|s| s.icons);
    let (icon, icons) = icons::apply_tray(hwnd, mode, previous);
    with_state_mut(|s| s.icons = icons);
    icon
}
fn tray(hwnd: HWND, operation: u32, refresh_icon: bool) -> io::Result<()> {
    if operation != NIM_DELETE && !with_state(|s| s.app.prefs.show_tray) {
        return Ok(());
    }
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = CALLBACK;
    data.hIcon = if operation == NIM_DELETE {
        null_mut()
    } else {
        tray_icon(hwnd, refresh_icon) as _
    };
    for (dst, src) in data
        .szTip
        .iter_mut()
        .take(127)
        .zip(tooltip().encode_utf16())
    {
        *dst = src;
    }
    if unsafe { Shell_NotifyIconW(operation, &data) } == 0 {
        Err(fault::Fault::TrayIcon.into())
    } else {
        Ok(())
    }
}
/// Shows a tray notification; callers apply their own opt-in preference.
fn tray_balloon(hwnd: HWND, title: &str, body: &str, kind: Balloon) {
    if !with_state(|s| s.tray_available && s.app.prefs.show_tray) {
        return;
    }
    with_state_mut(|s| s.balloon = kind);
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_INFO;
    for (dst, src) in data
        .szInfoTitle
        .iter_mut()
        .take(63)
        .zip(title.encode_utf16())
    {
        *dst = src;
    }
    for (dst, src) in data.szInfo.iter_mut().take(255).zip(body.encode_utf16()) {
        *dst = src;
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}
fn open_shell_uri(uri: &str) {
    let operation = wide("open");
    let target = wide(uri);
    // SAFETY: NUL-terminated verb and URI; no window is required for default browser/settings.
    unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}
fn apply_tray_preference(hwnd: HWND) -> io::Result<()> {
    let show = with_state(|s| s.app.prefs.show_tray);
    if show {
        match tray(hwnd, NIM_ADD, true) {
            Ok(()) => {
                with_state_mut(|s| s.tray_available = true);
                Ok(())
            }
            Err(error) => {
                with_state_mut(|s| s.tray_available = false);
                Err(error)
            }
        }
    } else {
        let _ = tray(hwnd, NIM_DELETE, false);
        Ok(())
    }
}
fn menu(hwnd: HWND) {
    top::remember_foreground(hwnd);
    let _ = top::reconcile();
    let (display, interval_minutes, sound, lang, power_store, renamer, modules, layout) =
        with_state(|s| {
            (
                s.app.prefs.display,
                s.app.prefs.interval_minutes,
                s.app.prefs.sound,
                language::resolve(s.app.prefs.language),
                tray::PowerStoreMenu {
                    updates: s
                        .app
                        .power_store
                        .update_count(&s.app.prefs.power_store.filters),
                    checking: s.app.power_store.checking(),
                },
                s.app.media_renamer.tray_status(),
                s.app.prefs.modules,
                s.app.prefs.tray,
            )
        });
    let session = awake::session();
    let status = awake::status(lang.strings());
    let expiry = awake::expiry_label();
    let bound_app = awake::bound_app();
    // Only chords that are registered: the tray never lists an inactive one.
    let pin_hotkey = hotkeys::active_label(Slot::Pin);
    let opacity_up = hotkeys::active_label(Slot::OpacityUp);
    let opacity_down = hotkeys::active_label(Slot::OpacityDown);
    let always_on_hotkey = hotkeys::active_label(Slot::AlwaysOn);
    let ruler_hotkey = hotkeys::active_label(Slot::Ruler);
    let mouse_enabled = with_state(|s| s.app.prefs.mouse.enabled);
    if let Some(command) = menu::track(
        hwnd,
        &tray::entries(tray::Menus {
            mouse: mouse_enabled,
            always_on: tray::AlwaysOnMenu {
                session,
                display,
                interval_minutes,
                status: &status,
                expiry: expiry.as_deref(),
                bound_app: bound_app.as_deref(),
            },
            always_top: tray::AlwaysTopMenu {
                pin: &top::session(),
                sound,
            },
            ruler: tray::ScreenRulerMenu {
                active: ruler::active(),
            },
            power_store,
            media_renamer: renamer,
            shortcuts: tray::Shortcuts {
                always_on: always_on_hotkey.as_deref(),
                pin: pin_hotkey.as_deref(),
                opacity_up: opacity_up.as_deref(),
                opacity_down: opacity_down.as_deref(),
                ruler: ruler_hotkey.as_deref(),
            },
            modules,
            layout,
            lang,
        }),
    ) {
        unsafe {
            PostMessageW(hwnd, WM_COMMAND, command, 0);
        }
    }
}
/// A tray menu choice or a worker thread's notification, as a `WM_COMMAND` id.
fn on_command(hwnd: HWND, id: usize) -> LRESULT {
    if let Some(command) = Command::from_id(id) {
        on_intent(hwnd, Intent::Command(command));
    }
    0
}
/// Runs the intents the settings shell queued, oldest first.
fn drain_intents(hwnd: HWND) {
    while let Some(intent) = intents::next() {
        // An Exit intent destroys the window; what was queued behind it is
        // dropped, as a posted message to a destroyed window would be.
        // SAFETY: IsWindow accepts any handle value.
        if unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd) } == 0 {
            continue;
        }
        on_intent(hwnd, intent);
    }
}
/// What the platform's background work reports, on the UI thread.
fn on_event(hwnd: HWND, event: Event) {
    if !event
        .tool()
        .is_none_or(|tool| with_state(|s| tool.enabled(&s.app.prefs)))
    {
        return;
    }
    match event {
        Event::MouseDevicesChanged => {
            let strings = ui_strings();
            for (name, percent) in power_mouse::on_devices_published() {
                tray_balloon(
                    hwnd,
                    strings.mouse.title,
                    &strings
                        .mouse
                        .battery_low
                        .replace("{name}", &name)
                        .replace("{percent}", &percent.to_string()),
                    Balloon::Info,
                );
            }
            update_view(hwnd, true);
        }
        Event::RenamerProgress => {
            if with_state_mut(|s| s.app.media_renamer.poll()) {
                update_view(hwnd, true);
            }
        }
        Event::ResizeProgress => resize_dialog::on_job(),
        Event::Suspending => power_mouse::on_suspend(),
        Event::Resumed => {
            power_mouse::on_resume();
            if let Err(e) = awake::reconcile(hwnd) {
                fail_closed(hwnd, e);
            }
        }
        Event::PowerSourceChanged => awake::enforce_policy(hwnd),
        Event::ClockChanged => {
            if let Err(e) = awake::reconcile(hwnd) {
                fail_closed(hwnd, e);
            }
        }
        Event::SessionLocked
        | Event::SessionUnlocked
        | Event::SessionDisconnected
        | Event::SessionConnected => {
            power_mouse::on_session_change(match event {
                Event::SessionLocked => power_mouse::SessionChange::Locked,
                Event::SessionUnlocked => power_mouse::SessionChange::Unlocked,
                Event::SessionDisconnected => power_mouse::SessionChange::Disconnected,
                _ => power_mouse::SessionChange::Connected,
            });
            update_view(hwnd, true);
        }
        // The scan itself runs on the discovery thread.
        Event::HardwareChanged => power_mouse::on_devices_changed(),
        // Refreshes the tray icon and the shell rail accent together.
        Event::ThemeChanged => update_view(hwnd, true),
    }
}
/// The one entry point for everything a user asks for, whichever surface
/// asked: the settings shell, the tray menu or a shortcut.
fn on_intent(hwnd: HWND, intent: Intent) {
    let reply = with_state_mut(|s| s.app.intent(intent));
    let result = run_step(hwnd, reply.step);
    if reply.refresh {
        update_view(hwnd, true);
    }
    settle(hwnd, result);
}
/// Has a tool do what the application asked for.
fn run_tool(hwnd: HWND, request: ToolRequest) -> io::Result<()> {
    match request {
        ToolRequest::TogglePin => {
            top::toggle(hwnd)?;
            update_view(hwnd, true);
            Ok(())
        }
        ToolRequest::StepOpacity(delta) => {
            let _ = top::step_tray_opacity(delta);
            Ok(())
        }
        ToolRequest::ToggleForegroundPin => {
            top::toggle_hotkey(hwnd)?;
            update_view(hwnd, true);
            Ok(())
        }
        ToolRequest::StepForegroundOpacity(delta) => {
            let _ = top::step_foreground_opacity(delta);
            Ok(())
        }
        ToolRequest::ToggleAlwaysOn => awake::toggle(hwnd),
        ToolRequest::ToggleRuler => {
            let result = ruler::toggle();
            update_view(hwnd, true);
            result
        }
        ToolRequest::OpenResize => {
            open_resize(hwnd, Vec::new(), current_resize());
            Ok(())
        }
        ToolRequest::ToggleDisplay => toggle_display(hwnd),
        ToolRequest::ExtendAlwaysOn => awake::extend(hwnd),
        ToolRequest::BindLastWindow => awake::bind_last(hwnd),
        ToolRequest::StartAlwaysOn(choice) => {
            let interval = with_state(|s| s.app.prefs.interval_minutes);
            match choice.start(awake::uptime_ms(), interval) {
                Some(next) => awake::start(hwnd, next),
                None => Ok(()),
            }
        }
        ToolRequest::SetInterval(minutes) => apply_interval(hwnd, minutes),
        ToolRequest::StartExpiry(pick) => start_expiry(hwnd, pick),
        ToolRequest::ToggleRing => {
            let labels = with_state(|s| {
                let strings = language::resolve(s.app.prefs.language).strings();
                s.app.ring_labels(strings, keys::META_KEY)
            });
            let mut labels = labels.into_iter();
            power_mouse::toggle_ring(std::array::from_fn(|_| labels.next().flatten()));
            Ok(())
        }
        ToolRequest::ToggleFrameAccent => apply_edit(
            hwnd,
            Edit::ToggleFrameAccent {
                accent: color::from_colorref(frame::accent_color()),
            },
        ),
    }
}
/// Reports a failed command; the smoke test instead stops at the first failure.
fn settle(hwnd: HWND, result: io::Result<()>) {
    if let Err(e) = result {
        if with_state(|s| s.smoke) {
            fail_closed(hwnd, e);
        } else {
            report_error(&e);
        }
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: Windows invokes this procedure with a live window on its UI thread.
    unsafe {
        let taskbar = with_state(|s| s.taskbar_message);
        if taskbar != 0 && msg == taskbar {
            let smoke = with_state(|s| s.smoke);
            let available = !smoke && tray(hwnd, NIM_ADD, true).is_ok();
            with_state_mut(|s| s.tray_available = available);
            update_view(hwnd, true);
            if !available && !smoke {
                present(hwnd);
            }
            return 0;
        }
        if let Some(event) = events::from_message(msg, w, l) {
            on_event(hwnd, event);
            return events::reply(hwnd, msg, w, l);
        }
        match msg {
            instance::ALWAYS_ON => {
                if let Some(request) = instance::always_on_from_message(w, l) {
                    apply_request(hwnd, request);
                }
                0
            }
            CALLBACK if l as u32 == NIN_BALLOONUSERCLICK => {
                let kind = with_state_mut(|s| std::mem::take(&mut s.balloon));
                match kind {
                    Balloon::AlwaysOnEnding => {
                        on_intent(hwnd, Intent::Command(Command::AlwaysOnExtend));
                    }
                    Balloon::PowerStoreUpdates => {
                        on_intent(hwnd, Intent::Command(Command::PowerStoreOpen));
                    }
                    Balloon::Info => {}
                }
                0
            }
            CALLBACK if l as u32 == NIN_BALLOONTIMEOUT || l as u32 == NIN_BALLOONHIDE => {
                with_state_mut(|s| s.balloon = Balloon::Info);
                0
            }
            CALLBACK
                if l as u32 == WM_RBUTTONUP
                    || l as u32 == WM_LBUTTONUP
                    || l as u32 == WM_CONTEXTMENU =>
            {
                menu(hwnd);
                0
            }
            WM_DPICHANGED => {
                if with_state(|s| s.tray_available) && tray(hwnd, NIM_MODIFY, true).is_err() {
                    mark_tray_unavailable(hwnd);
                }
                0
            }
            winui_host::CREATED => {
                let previous = with_state(|s| s.icons);
                let icons = icons::apply_window(w as HWND, previous);
                with_state_mut(|s| s.icons = icons);
                winui_host::apply_shell_opacity(with_state(|s| s.app.prefs.shell_opacity));
                // Packaged StartupTask APIs need this UI thread; sync once the
                // settings shell exists rather than during pre-WinUI launch.
                if !with_state(|s| s.smoke) {
                    let run_at_startup = with_state(|s| s.app.prefs.startup);
                    if let Err(error) = startup::set_enabled(run_at_startup) {
                        let warning = format!("{} ({error})", ui_strings().warn_startup);
                        with_state_mut(|s| s.app.warning = Some(warning));
                        update_view(hwnd, true);
                    }
                }
                0
            }
            winui_host::DIALOG_CREATED => {
                // Share the settings icons; do not replace IconSet ownership.
                if let Some(settings) = winui_host::settings() {
                    icons::copy_window_icons(settings, w as HWND);
                }
                // Defer activation: HCBT_CREATEWND fires before the window is shown.
                PostMessageW(hwnd, winui_host::DIALOG_PRESENT, 0, 0);
                0
            }
            winui_host::DIALOG_PRESENT => {
                winui_host::present_dialog();
                0
            }
            winui_host::CAPTURED_KEY => {
                if let Some(capturing) = with_state(|s| s.app.capturing()) {
                    let vk = w as u16;
                    if vk == VK_ESCAPE {
                        cancel_capture(hwnd);
                    } else {
                        match capturing {
                            Capturing::MouseAddFKey => finish_f_key_capture(hwnd, vk),
                            Capturing::Hotkey(_) | Capturing::MouseCustom(_) => {
                                if let Some(key) = read_capture(vk, l) {
                                    finish_capture(hwnd, key);
                                }
                            }
                        }
                    }
                }
                0
            }
            winui_host::DEACTIVATED => {
                cancel_capture(hwnd);
                0
            }
            WM_HOTKEY => {
                match hotkeys::slot(w as i32) {
                    Some(slot) => on_intent(hwnd, Intent::Shortcut(slot)),
                    None => return DefWindowProcW(hwnd, msg, w, l),
                }
                0
            }
            WM_COMMAND if (w >> 16) == 0 => on_command(hwnd, w & 0xffff),
            WM_COPYDATA => {
                // SAFETY: this is the WM_COPYDATA handler, so the sender's buffer
                // stays valid until it returns; the request is copied out here.
                // Each request is carried out from a posted message, never while
                // the sending process waits.
                match instance::accept(l) {
                    Some(instance::Request::Rename(paths)) => {
                        with_state_mut(|s| s.rename_handoff.get_or_insert_default().extend(paths));
                        PostMessageW(hwnd, media_renamer::handoff::HANDOFF, 0, 0);
                    }
                    Some(instance::Request::Resize(files)) => {
                        resize::queue(files);
                        PostMessageW(hwnd, resize::SHOW, 0, 0);
                    }
                    Some(instance::Request::AlwaysOn(request)) => {
                        let (kind, value) = instance::always_on_message(request);
                        PostMessageW(hwnd, instance::ALWAYS_ON, kind, value);
                    }
                    Some(instance::Request::Open) => {
                        PostMessageW(hwnd, WM_COMMAND, OPEN, 0);
                    }
                    None => return 0,
                }
                1
            }
            media_renamer::handoff::HANDOFF => {
                receive_rename_handoff(hwnd);
                0
            }
            intents::WAKE => {
                drain_intents(hwnd);
                0
            }
            events::EVENT => {
                if let Some(event) = events::from_wparam(w) {
                    on_event(hwnd, event);
                }
                0
            }
            resize::SHOW => {
                open_pending_resize(hwnd);
                0
            }
            WM_TIMER if w == ruler::TIMER => {
                ruler::on_timer();
                0
            }
            WM_TIMER if w == menu_hint::TIMER => {
                menu_hint::on_timer();
                0
            }
            WM_MENUSELECT => {
                menu_hint::on_menu_select(w, l);
                0
            }
            ruler::ENDED => {
                if let Some(error) = ruler::take_failure() {
                    report_error(&error);
                }
                update_view(hwnd, true);
                0
            }
            ruler::DISPLAY_CHANGED => {
                ruler::on_display_change();
                0
            }
            WM_TIMER if w == POLL_TIMER => {
                let Tick { changed, updates } =
                    with_state_mut(|s| s.app.tick(std::time::Instant::now()));
                top::remember_foreground(hwnd);
                if top::reconcile() || changed {
                    update_view(hwnd, true);
                }
                if let Some(count) = updates {
                    let t = ui_strings();
                    tray_balloon(
                        hwnd,
                        t.power_store_balloon_title,
                        &format!("{count} {}", t.power_store_balloon_body),
                        Balloon::PowerStoreUpdates,
                    );
                }
                window::tick_hero();
                0
            }
            WM_TIMER if w == awake::TIMER => {
                if let Err(e) = awake::on_timer(hwnd) {
                    fail_closed(hwnd, e);
                }
                0
            }
            WM_TIMER if w == SMOKE => {
                // Two stages prove timer expiry, then the exact user Exit command releases an active request.
                if with_state(|s| s.smoke_stage) == 0 {
                    if awake::session() != Session::Off {
                        fail_closed(hwnd, io::Error::other("Süreli oturum sona ermedi"));
                    } else if let Err(e) = awake::start(hwnd, Session::Indefinite) {
                        fail_closed(hwnd, e);
                    } else {
                        with_state_mut(|s| s.smoke_stage = 1);
                        PostMessageW(hwnd, WM_COMMAND, EXIT, 0);
                    }
                }
                0
            }
            WM_CLOSE | winui_host::CLOSE_REQUESTED => {
                if with_state(|s| s.tray_available) {
                    winui_host::hide();
                } else {
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_ENDSESSION if w != 0 => {
                with_state_mut(|s| {
                    s.app.power_store.shutdown();
                    s.app.media_renamer.shutdown();
                });
                hotkeys::shutdown();
                let _ = awake::shutdown(hwnd);
                top::shutdown();
                ruler::shutdown();
                power_mouse::shutdown();
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, SMOKE);
                KillTimer(hwnd, POLL_TIMER);
                with_state_mut(|s| {
                    s.app.power_store.shutdown();
                    s.app.media_renamer.shutdown();
                });
                hotkeys::shutdown();
                if let Err(e) = awake::shutdown(hwnd) {
                    with_state_mut(|s| s.fault = Some(e.to_string()));
                }
                top::shutdown();
                ruler::shutdown();
                power_mouse::shutdown();
                WTSUnRegisterSessionNotification(hwnd);
                let _ = tray(hwnd, NIM_DELETE, false);
                let icons = with_state_mut(|s| std::mem::take(&mut s.icons));
                icons::release_all(icons);
                // The reactor loop ends once every WinUI window closes; posting
                // WM_QUIT while a WinUI window is alive crashes WinUI.
                resize_dialog::close_for_exit();
                if !winui_host::close_for_exit() {
                    PostQuitMessage(0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
/// The icon the settings UI gives a tool, for the tray menu.
fn tool_glyph(tool: powerkit_core::tray_layout::TrayTool) -> &'static str {
    use window::PageGlyph;
    window::tray_page(tool).glyph()
}
pub fn run() -> io::Result<()> {
    powerkit_platform_windows::install_shell(powerkit_platform_windows::Shell {
        update_view,
        tray_balloon,
        tool_glyph,
    }); // SAFETY: handles and buffers stay alive throughout the message loop; cleanup is on this thread.
    unsafe {
        let smoke = std::env::args().any(|a| a == "--smoke-test");
        let idle_test = smoke && std::env::args().any(|a| a == "--idle");
        let mutex_name = if smoke {
            format!("Local\\PowerKit.Smoke.{}", std::process::id())
        } else {
            "Local\\PowerKit.Singleton".into()
        };
        let args: Vec<String> = std::env::args().collect();
        let open_singleton = || {
            let mutex = CreateMutexW(null(), 0, wide(&mutex_name).as_ptr());
            if mutex.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok((Handle(mutex), GetLastError() == ERROR_ALREADY_EXISTS))
        };
        let (mut _mutex, mut already_running) = open_singleton()?;
        let restarted = !smoke && recovery::is_restart(&args);
        if already_running && restarted {
            // A crashed PowerKit holds the name until Windows has finished
            // reporting the crash; wait for it rather than defer to it.
            let gone = recovery::wait_for_previous(|| {
                drop(std::mem::replace(&mut _mutex, Handle(null_mut())));
                match open_singleton() {
                    Ok((handle, exists)) => {
                        _mutex = handle;
                        exists
                    }
                    Err(_) => true,
                }
            });
            already_running = !gone;
            if already_running {
                // Another PowerKit really is running: nothing to replace, and
                // its window must not pop up for a restart nobody asked for.
                return Ok(());
            }
        }
        // The smoke test asks for nothing and never reads a list file.
        let launch = if smoke {
            instance::Request::Open
        } else {
            instance::Request::parse(&args, instance::take_path_list)
        };
        if already_running {
            let forwarded = instance::forward(&launch);
            return match launch {
                // The user chose files; say so when they went nowhere.
                instance::Request::Resize(_) | instance::Request::Rename(_) => forwarded,
                instance::Request::AlwaysOn(_) | instance::Request::Open => Ok(()),
            };
        }
        let (resize_files, rename_paths, request) = match launch {
            instance::Request::Resize(files) => (Some(files), None, None),
            instance::Request::Rename(paths) => (None, Some(paths), None),
            instance::Request::AlwaysOn(request) => (None, None, Some(request)),
            instance::Request::Open => (None, None, None),
        };
        let _power = PowerGuard::default();
        let _pin = top::PinGuard::default();
        let (prefs, warning) = if smoke {
            (
                powerkit_core::settings::Preferences {
                    sound: false,
                    ..powerkit_core::settings::Preferences::default()
                },
                None,
            )
        } else {
            // The warning text is chosen once the loaded language is in STATE.
            type Pick = fn(&powerkit_core::locale::Strings) -> &'static str;
            match load_prefs() {
                Ok(loaded) if loaded.dropped > 0 => {
                    (loaded.prefs, Some((|t| t.warn_settings_partial) as Pick))
                }
                Ok(loaded) => (loaded.prefs, None),
                Err(_) => (
                    powerkit_core::settings::Preferences::default(),
                    Some((|t| t.warn_settings_unreadable) as Pick),
                ),
            }
        };
        let show_tray = prefs.show_tray;
        with_state_mut(|s| {
            s.app.prefs = prefs.clone();
            powerkit_platform_windows::set_language(prefs.language);
            s.smoke = smoke;
            s.taskbar_message = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        });
        if let Some(pick) = warning {
            let warning = pick(ui_strings()).to_string();
            with_state_mut(|s| s.app.warning = Some(warning));
        }
        let instance = GetModuleHandleW(null());
        let class_name = wide(if smoke {
            "PowerKit.Smoke.Window"
        } else {
            "PowerKit.Window"
        });
        let class_icon = icons::class_icon();
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            hbrBackground: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: class_icon,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            icons::release_owned(class_icon as isize);
            return Err(io::Error::last_os_error());
        }
        // Never shown: it owns the tray icon, hotkeys, timers and power request, and
        // stays the FindWindow target for second instances.
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("PowerKit").as_ptr(),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        with_state_mut(|s| s.app.media_renamer = media_renamer::session(hwnd, &prefs.rename));
        apply_feature_prefs(hwnd, &prefs);
        // Self-test must not add an icon to, or change preferences in, the user's desktop.
        if !smoke {
            top::listen(hwnd);
            ruler::listen(hwnd);
            // Shortcuts register only from here on; a tool that is switched off
            // holds no chord.
            hotkeys::listen(hwnd);
            hotkeys::sync(&prefs);
            power_mouse::listen(hwnd);
            // Lock, unlock and user switches arrive as WM_WTSSESSION_CHANGE.
            // Without them PowerMouse still works; it just keeps running on
            // the lock screen, so a failure is not reported.
            WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
            recovery::arm();
            if !package::is_packaged() {
                resize::register_context_menu();
                media_renamer::handoff::register_context_menu();
            }
        }
        let tray_available = if smoke {
            false
        } else if show_tray {
            tray(hwnd, NIM_ADD, true).is_ok()
        } else {
            // Keep hide-on-close; the Start menu can reopen settings via the singleton.
            true
        };
        with_state_mut(|s| s.tray_available = tray_available);
        update_view(hwnd, true);
        if smoke {
            if !idle_test && let Err(e) = smoke::run(hwnd) {
                DestroyWindow(hwnd);
                return Err(e);
            }
            if SetTimer(hwnd, SMOKE, if idle_test { 15000 } else { 2500 }, None) == 0 {
                DestroyWindow(hwnd);
                return Err(io::Error::last_os_error());
            }
        }
        if !smoke {
            with_state_mut(|s| {
                if let Ok(path) = settings_path() {
                    s.app
                        .power_store
                        .load_history(path.with_file_name(POWER_STORE_HISTORY_FILE));
                    s.app
                        .power_store
                        .set_installed_cache(path.with_file_name(POWER_STORE_INSTALLED_FILE));
                    s.app.media_renamer.load_history(
                        path.with_file_name(powerkit_core::media_renamer::history::FILE_NAME),
                    );
                }
            });
            if SetTimer(hwnd, POLL_TIMER, 1000, None) == 0 {
                DestroyWindow(hwnd);
                return Err(io::Error::last_os_error());
            }
        }
        if smoke {
            pump(hwnd)?;
        } else {
            if let Some(request) = request {
                apply_request(hwnd, request);
            }
            if let Some(paths) = rename_paths.clone() {
                with_state_mut(|s| s.app.replace_rename(paths, std::time::Instant::now()));
                window::show_page(window::Page::MediaRenamer);
                update_view(hwnd, true);
            }
            // Logon/crash restarts stay in the tray when the icon is shown;
            // without a tray the settings shell must appear or the session is
            // unreachable. Explorer handoffs also start hidden until present.
            let handoff = resize_files.is_some() || request.is_some() || rename_paths.is_some();
            let start_hidden = (startup::start_in_tray(&args) && show_tray) || handoff;
            // Defer WinUI until the first settings open so logon only pays for
            // the tray, hotkeys and PowerMouse — not the App Runtime.
            let defer_shell =
                startup::start_in_tray(&args) && show_tray && tray_available && !handoff;
            if defer_shell {
                with_state_mut(|s| s.shell_deferred = true);
                if !pump_until_shell(hwnd)? || IsWindow(hwnd) == 0 {
                    // User exited from the tray before opening settings.
                } else {
                    with_state_mut(|s| s.shell_deferred = false);
                    run_settings_shell(hwnd, false)?;
                }
            } else {
                if let Some(files) = resize_files {
                    // Queue for the WinUI shell; falls back to Win32 if reactor fails.
                    resize_dialog::queue(resize_dialog::Seed {
                        host: shell_host(hwnd),
                        files,
                        settings: current_resize(),
                    });
                }
                run_settings_shell(hwnd, start_hidden)?;
            }
        }
        if let Some(fault) = with_state(|s| s.fault.clone()) {
            return Err(io::Error::other(fault));
        }
        if smoke {
            if with_state(|s| s.smoke_stage) != 1 {
                return Err(io::Error::other("Smoke testi tamamlanmadı"));
            }
            let previous = SetThreadExecutionState(ES_CONTINUOUS);
            if previous == 0 || previous & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) != 0 {
                return Err(io::Error::other("Çıkışta güç isteği temizlenmedi"));
            }
            println!("PowerKit smoke test passed");
        }
        Ok(())
    }
}
/// Starts the WinUI settings shell and reports + exits when it cannot run.
fn run_settings_shell(hwnd: HWND, start_hidden: bool) -> io::Result<()> {
    let host = winui_host::install(hwnd, start_hidden);
    let settings = window::run(shell_host(hwnd));
    drop(host);
    // SAFETY: hwnd was valid when WinUI ran; IsWindow only reads its lifetime.
    if unsafe { IsWindow(hwnd) } == 0 {
        return Ok(());
    }
    // WinUI did not start (for example the runtime is missing) or ended
    // without app teardown. Tray-only is not a product mode: tell the
    // user and exit. A pending --resize has no dialog without WinUI.
    let warning = match settings {
        Ok(()) => ui_strings().settings_unavailable.to_string(),
        Err(error) => format!("{} ({error})", ui_strings().settings_unavailable),
    };
    with_state_mut(|s| {
        s.settings_unavailable = true;
        s.app.warning = Some(warning.clone());
    });
    report(&warning);
    unsafe {
        DestroyWindow(hwnd);
    }
    pump(hwnd)
}

/// Plain Win32 loop for the smoke test, and to drain teardown when WinUI is absent.
fn pump(hwnd: HWND) -> io::Result<()> {
    match pump_loop(hwnd, false)? {
        PumpEnd::Quit => Ok(()),
        PumpEnd::StartShell => Ok(()),
    }
}

/// Tray/logon pump: runs until exit (`false`) or the first settings open (`true`).
fn pump_until_shell(hwnd: HWND) -> io::Result<bool> {
    Ok(matches!(pump_loop(hwnd, true)?, PumpEnd::StartShell))
}

enum PumpEnd {
    Quit,
    StartShell,
}

fn pump_loop(hwnd: HWND, watch_shell: bool) -> io::Result<PumpEnd> {
    // SAFETY: MSG lives across the loop; dispatch stays on this UI thread.
    unsafe {
        let mut message = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == -1 {
                DestroyWindow(hwnd);
                return Err(io::Error::last_os_error());
            }
            if result == 0 {
                if message.wParam != 0 {
                    return Err(io::Error::other("AlwaysOn smoke test failed"));
                }
                return Ok(PumpEnd::Quit);
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
            if watch_shell && SHELL_REQUESTED.replace(false) {
                return Ok(PumpEnd::StartShell);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_exposes_icon_index_zero() {
        const RT_GROUP_ICON: usize = 14;
        // SAFETY: a null module name selects this executable. Ids below 64Ki are integer resources.
        let module = unsafe { GetModuleHandleW(null()) };
        assert!(!module.is_null());
        let resource = |id: usize| std::ptr::with_exposed_provenance::<u16>(id);
        let icon = unsafe { FindResourceW(module, resource(1), resource(RT_GROUP_ICON)) };
        assert!(!icon.is_null());
    }

    #[test]
    fn every_tool_and_the_shortcuts_submenu_have_distinct_glyphs() {
        let mut glyphs: Vec<&str> = powerkit_core::tray_layout::TrayTool::ALL
            .into_iter()
            .map(tool_glyph)
            .collect();
        glyphs.push(menu::GLYPH_KEYBOARD);
        for (index, glyph) in glyphs.iter().enumerate() {
            assert!(!glyph.is_empty());
            assert!(!glyphs[..index].contains(glyph), "{glyph:?}");
        }
    }
}
