//! Native Win32 boundary. All UI and power calls run on the message-loop thread.
mod awake;
mod clock;
mod command;
mod fault;
mod frame;
mod icons;
mod media_renamer;
mod menu;
mod package;
mod power;
mod power_mouse;
mod power_store;
mod resize;
mod resize_dialog;
mod resize_io;
mod resize_wic;
mod ruler;
mod ruler_bar;
mod settings_ui;
mod shell_package;
mod smoke;
mod startup;
mod theme;
mod top;
mod window;
mod winui_host;

use crate::always_on::{Request, Session};
use crate::power_store::{OperationKind, QueuedOperation};
use command::{AppCommand, EXIT, OPEN};
use icons::IconSet;
use media_renamer::Action as RenameAction;
use power::PowerGuard;
use power_store::{Action as StoreAction, Notice};
use std::{
    cell::RefCell,
    io,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, Power::*, SystemInformation::*, Threading::*},
    UI::{Input::KeyboardAndMouse::VK_ESCAPE, Shell::*, WindowsAndMessaging::*},
};
const CALLBACK: u32 = WM_APP + 1;
const SMOKE: usize = 2;
const POLL_TIMER: usize = 3;
const REQUEST_MESSAGE: &str = "PowerKit.AlwaysOn.Request";
/// Beside settings.txt so history shares its location without bloating settings writes.
const POWER_STORE_HISTORY_FILE: &str = "power-store-history.txt";
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
fn ui_strings() -> &'static crate::locale::Strings {
    with_state(|s| crate::locale::UiLanguage::resolve(s.prefs.language).strings())
}
/// What a click on the last tray balloon means.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Balloon {
    #[default]
    Info,
    AlwaysOnEnding,
    PowerStoreUpdates,
}
#[derive(Default)]
struct State {
    taskbar_message: u32,
    request_message: u32,
    balloon: Balloon,
    tray_available: bool,
    smoke: bool,
    fault: Option<String>,
    warning: Option<String>,
    capturing: Option<Capturing>,
    settings_unavailable: bool,
    last_caption: String,
    smoke_stage: u8,
    icons: IconSet,
    prefs: crate::settings::Preferences,
    power_store: power_store::Session,
    media_renamer: media_renamer::Session,
    /// Explorer selection received by WM_COPYDATA, confirmed on `handoff::HANDOFF`.
    rename_handoff: Option<Vec<PathBuf>>,
    /// When the last Explorer selection was taken; classic verbs start one process
    /// per item, so handoffs this close together extend that selection.
    rename_handoff_at: Option<std::time::Instant>,
}
const RENAME_HANDOFF_BURST: std::time::Duration = std::time::Duration::from_secs(2);
fn with_state<R>(f: impl FnOnce(&State) -> R) -> R {
    STATE.with(|s| f(&s.borrow()))
}
fn with_state_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn settings_path() -> io::Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or(fault::Fault::SettingsLocation)?)
            .join("PowerKit")
            .join("settings.txt"),
    )
}
fn load_prefs() -> io::Result<crate::settings::Loaded> {
    crate::settings::load(&settings_path()?)
}
fn save_prefs(prefs: crate::settings::Preferences) -> io::Result<()> {
    crate::settings::save(&settings_path()?, prefs)
}
/// Saves and applies preferences. The smoke test never writes the user's file.
fn persist(hwnd: HWND, prefs: crate::settings::Preferences) -> io::Result<()> {
    if !with_state(|s| s.smoke) {
        save_prefs(prefs.clone())?;
    }
    with_state_mut(|s| {
        s.prefs = prefs.clone();
        s.warning = None;
        s.capturing = None;
    });
    winui_host::end_key_capture();
    apply_feature_prefs(hwnd, &prefs);
    winui_host::apply_shell_opacity(prefs.shell_opacity);
    update_view(hwnd, true);
    Ok(())
}
fn apply_feature_prefs(hwnd: HWND, prefs: &crate::settings::Preferences) {
    // Workers read the manager switches and program paths, so publish them first.
    power_store::manager_config::apply(&prefs.power_store);
    if prefs.modules.always_top {
        top::apply_prefs(prefs);
    } else {
        let _ = top::release();
        top::pause_hotkeys();
    }
    if prefs.modules.screen_ruler {
        ruler::apply_prefs(prefs);
    } else {
        ruler::shutdown();
    }
    if prefs.modules.always_on {
        awake::apply_prefs(hwnd, prefs);
    } else {
        let _ = awake::start(hwnd, crate::always_on::Session::Off);
        awake::pause_hotkeys();
    }
    power_mouse::apply_prefs(prefs);
}
fn module_allows(command: AppCommand) -> bool {
    let (modules, mouse) = with_state(|s| (s.prefs.modules, s.prefs.mouse.enabled));
    match command {
        AppCommand::ToggleModuleAlwaysOn
        | AppCommand::ToggleModuleAlwaysTop
        | AppCommand::ToggleModuleScreenRuler
        | AppCommand::ToggleModuleImageResizer
        | AppCommand::ToggleModulePowerStore
        | AppCommand::ToggleModuleMediaRenamer
        | AppCommand::ToggleMouse
        | AppCommand::OpenSettings
        | AppCommand::Exit
        | AppCommand::ToggleStartup
        | AppCommand::ToggleShowTray
        | AppCommand::OpenColorSettings
        | AppCommand::ApplyShellOpacity
        | AppCommand::ThemeSystem
        | AppCommand::ThemeLight
        | AppCommand::ThemeDark
        | AppCommand::LanguageSystem
        | AppCommand::LanguageTurkish
        | AppCommand::LanguageEnglish
        | AppCommand::LanguageArabic => true,
        c if c.always_on_session(0, 0).is_some()
            || matches!(
                c,
                AppCommand::AlwaysOnExpiry
                    | AppCommand::ApplyInterval
                    | AppCommand::AlwaysOnBindProcess
                    | AppCommand::AlwaysOnExtend
                    | AppCommand::CaptureAlwaysOnHotkey
                    | AppCommand::ToggleAlwaysOnWarn
                    | AppCommand::ToggleAcOnly
                    | AppCommand::BatteryFloorDown
                    | AppCommand::BatteryFloorUp
                    | AppCommand::ToggleDisplay
            ) =>
        {
            modules.always_on
        }
        AppCommand::TogglePin
        | AppCommand::ToggleSound
        | AppCommand::OpacityUp
        | AppCommand::OpacityDown
        | AppCommand::CapturePinHotkey
        | AppCommand::CaptureOpacityUpHotkey
        | AppCommand::CaptureOpacityDownHotkey
        | AppCommand::ToggleFrameAccent
        | AppCommand::PickFrameColor
        | AppCommand::FrameOpacityDown
        | AppCommand::FrameOpacityUp
        | AppCommand::FrameThicknessDown
        | AppCommand::FrameThicknessUp => modules.always_top,
        AppCommand::ToggleMeasure
        | AppCommand::CaptureRulerHotkey
        | AppCommand::RulerModeNone
        | AppCommand::RulerModeBounds
        | AppCommand::RulerModeSpacing
        | AppCommand::RulerModeHorizontal
        | AppCommand::RulerModeVertical
        | AppCommand::ToggleRulerContinuous
        | AppCommand::ToggleRulerChannel
        | AppCommand::ToggleRulerFeet
        | AppCommand::RulerToleranceDown
        | AppCommand::RulerToleranceUp
        | AppCommand::PickRulerColor
        | AppCommand::RulerUnitPx
        | AppCommand::RulerUnitIn
        | AppCommand::RulerUnitCm
        | AppCommand::RulerUnitMm
        | AppCommand::RulerUnitDip => modules.screen_ruler,
        AppCommand::ResizeOpen
        | AppCommand::ApplyResizeFilename
        | AppCommand::ToggleResizeShrink
        | AppCommand::ToggleResizeReplace
        | AppCommand::ToggleResizeOrientation
        | AppCommand::ToggleResizeMetadata
        | AppCommand::ToggleResizeKeepDate
        | AppCommand::ResizeJpegDown
        | AppCommand::ResizeJpegUp
        | AppCommand::ResizeEncoderJpeg
        | AppCommand::ResizeEncoderPng
        | AppCommand::ResizeEncoderBmp
        | AppCommand::ResizeEncoderGif
        | AppCommand::ResizeEncoderTiff
        | AppCommand::ResizePngDefault
        | AppCommand::ResizePngOn
        | AppCommand::ResizePngOff
        | AppCommand::ResizeTiffDown
        | AppCommand::ResizeTiffUp => modules.image_resizer,
        AppCommand::PowerStoreRefresh
        | AppCommand::PowerStoreEnsureLoaded
        | AppCommand::PowerStoreAction
        | AppCommand::PowerStoreCancelPending
        | AppCommand::PowerStoreUpdateAll
        | AppCommand::PowerStoreCancelRunning
        | AppCommand::PowerStoreCheckNow
        | AppCommand::PowerStoreOpen => modules.power_store,
        AppCommand::MediaRenamerAction | AppCommand::MediaRenamerOpen => modules.media_renamer,
        AppCommand::PowerMouseAction => mouse,
        _ => true,
    }
}
fn toggle_module(hwnd: HWND, command: AppCommand) -> io::Result<()> {
    let mut prefs = with_state(|s| s.prefs.clone());
    let page = match command {
        AppCommand::ToggleModuleAlwaysOn => {
            prefs.modules.always_on = !prefs.modules.always_on;
            window::Page::AlwaysOn
        }
        AppCommand::ToggleModuleAlwaysTop => {
            prefs.modules.always_top = !prefs.modules.always_top;
            window::Page::AlwaysTop
        }
        AppCommand::ToggleModuleScreenRuler => {
            prefs.modules.screen_ruler = !prefs.modules.screen_ruler;
            window::Page::ScreenRuler
        }
        AppCommand::ToggleModuleImageResizer => {
            prefs.modules.image_resizer = !prefs.modules.image_resizer;
            window::Page::ImageResizer
        }
        AppCommand::ToggleModulePowerStore => {
            prefs.modules.power_store = !prefs.modules.power_store;
            window::Page::PowerStore
        }
        AppCommand::ToggleModuleMediaRenamer => {
            prefs.modules.media_renamer = !prefs.modules.media_renamer;
            window::Page::MediaRenamer
        }
        AppCommand::ToggleMouse => {
            prefs.mouse.enabled = !prefs.mouse.enabled;
            window::Page::PowerMouse
        }
        _ => return Ok(()),
    };
    let leave =
        !page.module_enabled(prefs.modules, prefs.mouse.enabled) && window::current_page() == page;
    persist(hwnd, prefs)?;
    if leave {
        window::show_page(window::Page::General);
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capturing {
    Hotkey(AppCommand),
    MouseCustom {
        trigger: crate::power_mouse::Trigger,
        slot: crate::power_mouse::ActionSlot,
    },
    MouseAddFKey,
}
fn begin_capture(hwnd: HWND, command: AppCommand) {
    begin_capturing(hwnd, Capturing::Hotkey(command));
}
fn begin_capturing(hwnd: HWND, capturing: Capturing) {
    with_state_mut(|s| s.capturing = Some(capturing));
    top::pause_hotkeys();
    ruler::pause_hotkeys();
    awake::pause_hotkeys();
    winui_host::begin_key_capture();
    update_view(hwnd, true);
}
fn cancel_capture(hwnd: HWND) {
    if with_state(|s| s.capturing).is_none() {
        return;
    }
    with_state_mut(|s| s.capturing = None);
    winui_host::end_key_capture();
    let prefs = with_state(|s| s.prefs.clone());
    apply_feature_prefs(hwnd, &prefs);
    update_view(hwnd, true);
}
fn finish_capture(hwnd: HWND, key: crate::always_top::Hotkey) {
    let Some(capturing) = with_state(|s| s.capturing) else {
        return;
    };
    let mut prefs = with_state(|s| s.prefs.clone());
    match capturing {
        Capturing::Hotkey(command) => match command {
            AppCommand::CapturePinHotkey => prefs.pin = key,
            AppCommand::CaptureOpacityUpHotkey => prefs.opacity_up = key,
            AppCommand::CaptureOpacityDownHotkey => prefs.opacity_down = key,
            AppCommand::CaptureRulerHotkey => prefs.ruler_hotkey = key,
            AppCommand::CaptureAlwaysOnHotkey => prefs.always_on_hotkey = key,
            _ => {}
        },
        Capturing::MouseCustom { trigger, slot } => {
            let binding = prefs.mouse.binding(trigger);
            let next = binding.with_slot(slot, crate::power_mouse::Action::Custom(key));
            if !prefs.mouse.set_binding(trigger, next) {
                cancel_capture(hwnd);
                return;
            }
        }
        Capturing::MouseAddFKey => {
            cancel_capture(hwnd);
            return;
        }
    }
    let _ = persist(hwnd, prefs).map_err(|error| report_error(&error));
}
fn finish_f_key_capture(hwnd: HWND, vk: u16) {
    let Some(Capturing::MouseAddFKey) = with_state(|s| s.capturing) else {
        return;
    };
    let Some(trigger) = crate::power_mouse::Trigger::from_vk(vk) else {
        return;
    };
    let mut prefs = with_state(|s| s.prefs.clone());
    if prefs.mouse.bindings.contains_key(&trigger) {
        cancel_capture(hwnd);
        return;
    }
    if !prefs.mouse.set_binding(
        trigger,
        crate::power_mouse::Binding::Single(crate::power_mouse::Action::None),
    ) {
        cancel_capture(hwnd);
        return;
    }
    let _ = persist(hwnd, prefs).map_err(|error| report_error(&error));
}
fn step_frame(hwnd: HWND, opacity_delta: i32, thickness_delta: i32) -> io::Result<()> {
    let mut prefs = with_state(|s| s.prefs.clone());
    prefs.frame_opacity =
        crate::always_top::clamp_frame_opacity(prefs.frame_opacity + opacity_delta);
    prefs.frame_thickness =
        crate::always_top::clamp_frame_thickness(prefs.frame_thickness + thickness_delta);
    persist(hwnd, prefs)
}
fn step_ruler(hwnd: HWND, edit: impl FnOnce(&mut crate::settings::Preferences)) -> io::Result<()> {
    let mut prefs = with_state(|s| s.prefs.clone());
    edit(&mut prefs);
    persist(hwnd, prefs)
}
fn read_capture(vk: u16, mods: isize) -> Option<crate::always_top::Hotkey> {
    crate::always_top::Hotkey::new(
        mods & winui_host::MOD_WIN != 0,
        mods & winui_host::MOD_CTRL != 0,
        mods & winui_host::MOD_ALT != 0,
        mods & winui_host::MOD_SHIFT != 0,
        vk,
    )
}
/// Runs a PowerStore WinGet request when launched as the worker child; returns its exit code.
pub fn run_power_store_worker() -> Option<i32> {
    power_store::broker::run_worker(&std::env::args().collect::<Vec<_>>())
}
/// Shows `error` in the current UI language (see [`fault::describe`]).
pub fn report_error(error: &io::Error) {
    report(&fault::describe(error, ui_strings()));
}
pub fn report(message: &str) {
    // SAFETY: valid temporary UTF-16 strings, no window ownership transferred.
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            wide("PowerKit").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: Handle owns the non-null mutex handle returned by CreateMutexW.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn tooltip() -> String {
    let t = ui_strings();
    let always_on = if awake::session().active() {
        format!("PowerKit — {}", awake::status(t))
    } else {
        t.status_off.into()
    };
    let (always_top_on, ruler_on) = (t.tray_always_top_on, t.ruler_on);
    let mut text = match top::session().title() {
        Some(title) => format!("{always_on} · {always_top_on}: {title}"),
        None => always_on,
    };
    if ruler::active() {
        text.push_str(" · ");
        text.push_str(ruler_on);
    }
    text
}
fn current_resize() -> crate::image_resizer::ResizeSettings {
    with_state(|s| s.prefs.resize.clone())
}
fn remember_resize(hwnd: HWND, incoming: crate::image_resizer::ResizeSettings) -> io::Result<()> {
    step_ruler(hwnd, |prefs| {
        prefs.resize.selected = incoming.selected;
        prefs.resize.custom = incoming.custom;
        prefs.resize.shrink_only = incoming.shrink_only;
        prefs.resize.replace = incoming.replace;
        prefs.resize.ignore_orientation = incoming.ignore_orientation;
    })
}
fn present_resize_page(hwnd: HWND) {
    window::show_page(window::Page::ImageResizer);
    present(hwnd);
}
fn save_resize_filename(hwnd: HWND) {
    let text = window::take_filename().unwrap_or_default();
    let Some(filename) = crate::image_resizer::ResizeSettings::parse_filename(&text) else {
        window::refresh();
        return;
    };
    if let Err(error) = step_ruler(hwnd, |prefs| prefs.resize.filename = filename) {
        report_error(&error);
        window::refresh();
    }
}
/// Shows the settings window, or explains why it cannot exist on this machine.
fn present(_hwnd: HWND) {
    if !winui_host::present() && with_state(|s| s.settings_unavailable && !s.smoke) {
        report(ui_strings().settings_unavailable);
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
fn power_store_refresh(hwnd: HWND) {
    with_state_mut(|s| {
        s.power_store
            .set_remote_icons(s.prefs.power_store.remote_icons);
        s.power_store.refresh_active();
    });
    update_view(hwnd, true);
}
fn open_power_store(hwnd: HWND) {
    window::show_page(window::Page::PowerStore);
    with_state_mut(|s| {
        s.power_store
            .set_remote_icons(s.prefs.power_store.remote_icons);
        s.power_store.show_installed();
    });
    update_view(hwnd, true);
    present(hwnd);
}
fn open_media_renamer(hwnd: HWND) {
    window::show_page(window::Page::MediaRenamer);
    update_view(hwnd, true);
    present(hwnd);
}
/// Explorer input replaces the preview; unapplied renames are only dropped after a yes.
fn receive_rename_handoff(hwnd: HWND) {
    let Some(paths) = with_state_mut(|s| s.rename_handoff.take()) else {
        return;
    };
    let (busy, unapplied, burst) = with_state(|s| {
        (
            s.media_renamer.busy(),
            s.media_renamer.has_unapplied(),
            s.rename_handoff_at
                .is_some_and(|at| at.elapsed() < RENAME_HANDOFF_BURST),
        )
    });
    if busy {
        open_media_renamer(hwnd);
        return;
    }
    if burst {
        with_state_mut(|s| {
            s.rename_handoff_at = Some(std::time::Instant::now());
            s.media_renamer.add(paths);
        });
        open_media_renamer(hwnd);
        return;
    }
    if unapplied {
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
        if answer != IDYES {
            open_media_renamer(hwnd);
            return;
        }
    }
    with_state_mut(|s| {
        s.rename_handoff_at = Some(std::time::Instant::now());
        s.media_renamer.replace(paths);
    });
    open_media_renamer(hwnd);
}
fn power_mouse_action(hwnd: HWND, action: window::PowerMouseAction) -> io::Result<()> {
    match action {
        window::PowerMouseAction::SetBinding { trigger, binding } => {
            let mut prefs = with_state(|s| s.prefs.clone());
            if !prefs.mouse.set_binding(trigger, binding) {
                return Ok(());
            }
            persist(hwnd, prefs)
        }
        window::PowerMouseAction::ResetDefaults => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.mouse.bindings.clear();
            persist(hwnd, prefs)
        }
        window::PowerMouseAction::BeginCustomCapture { trigger, slot } => {
            begin_capturing(hwnd, Capturing::MouseCustom { trigger, slot });
            Ok(())
        }
        window::PowerMouseAction::BeginAddFKey => {
            begin_capturing(hwnd, Capturing::MouseAddFKey);
            Ok(())
        }
        window::PowerMouseAction::RemoveFKey { trigger } => {
            let mut prefs = with_state(|s| s.prefs.clone());
            if !trigger.is_function_key() {
                return Ok(());
            }
            let _ = prefs
                .mouse
                .set_binding(trigger, crate::power_mouse::Binding::PassThrough);
            persist(hwnd, prefs)
        }
    }
}
fn media_renamer_action(hwnd: HWND, action: RenameAction) -> io::Result<()> {
    let owner = power_store_owner();
    let t = ui_strings();
    let edit = |edit: &dyn Fn(&mut crate::media_renamer::Preferences)| {
        let mut prefs = with_state(|s| s.prefs.clone());
        edit(&mut prefs.rename);
        persist(hwnd, prefs)
    };
    match action {
        RenameAction::AddFiles => {
            let paths =
                media_renamer::pick::files(owner, t.rename.pick_files, t.rename.pick_filter);
            with_state_mut(|s| s.media_renamer.add(paths));
        }
        RenameAction::AddFolder => {
            if let Some(path) = media_renamer::pick::folder(owner, t.rename.pick_folder) {
                with_state_mut(|s| s.media_renamer.add(vec![path]));
            }
        }
        RenameAction::SaveDefaultTemplate => {
            let template = with_state(|s| s.media_renamer.template_text().to_string());
            edit(&|prefs| prefs.template = template.clone())?;
        }
        RenameAction::SetDefaultRecurse(on) => edit(&|prefs| prefs.recurse = on)?,
        RenameAction::SetDefaultMtimeFallback(on) => edit(&|prefs| prefs.mtime_fallback = on)?,
        RenameAction::SetLowercaseExt(on) => {
            edit(&|prefs| prefs.lowercase_ext = on)?;
            with_state_mut(|s| s.media_renamer.set_lowercase_ext(on));
        }
        action => with_state_mut(|s| s.media_renamer.act(action)),
    }
    Ok(())
}
/// Saves PowerStore preferences changed by `edit`; returns `Ok(false)` when it rejects.
fn edit_power_store(
    hwnd: HWND,
    edit: impl FnOnce(&mut crate::power_store::PowerStorePreferences) -> bool,
) -> io::Result<bool> {
    let mut prefs = with_state(|s| s.prefs.clone());
    if !edit(&mut prefs.power_store) {
        return Ok(false);
    }
    persist(hwnd, prefs)?;
    Ok(true)
}
/// Saves one manager's settings. `relist` reloads the installed list, which a changed
/// on/off switch or program path alters; a default scope only affects later operations.
fn edit_manager(
    hwnd: HWND,
    kind: crate::power_store::ManagerKind,
    relist: bool,
    edit: impl FnOnce(&mut crate::power_store::ManagerSettings),
) -> io::Result<()> {
    let changed = edit_power_store(hwnd, |prefs| {
        let before = prefs.manager(kind);
        let mut after = before.clone();
        edit(&mut after);
        after != before && prefs.set_manager(kind, after)
    })?;
    if changed && relist {
        with_state_mut(|s| {
            s.power_store.refresh_installed(true);
            // The drawer shows the program and version, which this edit may have changed.
            s.power_store.probe_manager(kind);
        });
    }
    Ok(())
}
fn power_store_owner() -> HWND {
    winui_host::settings().unwrap_or(null_mut())
}
fn power_store_action(hwnd: HWND, action: StoreAction) -> io::Result<()> {
    match action {
        StoreAction::SetFilterText(text) => {
            with_state_mut(|s| s.power_store.set_filter_text(text));
        }
        StoreAction::SetSourceChecked(source, checked) => {
            with_state_mut(|s| s.power_store.set_source_checked(&source, checked));
        }
        StoreAction::ClearSourceFilter => {
            with_state_mut(|s| s.power_store.clear_source_filter());
        }
        StoreAction::SetStatusFilter(status) => {
            with_state_mut(|s| s.power_store.set_status_filter(status));
        }
        StoreAction::SetScopeFilter(scope) => {
            with_state_mut(|s| s.power_store.set_scope_filter(scope));
        }
        StoreAction::SetSortKey(key) => {
            with_state_mut(|s| s.power_store.set_sort_key(key));
        }
        StoreAction::SetSort(key, ascending) => {
            with_state_mut(|s| s.power_store.set_sort(key, ascending));
        }
        StoreAction::StartDiscover { query, source } => {
            with_state_mut(|s| s.power_store.start_discover(query, source));
        }
        StoreAction::ExitDiscover => {
            with_state_mut(|s| s.power_store.exit_discover());
        }
        StoreAction::Queue(kind, key, name) => with_state_mut(|s| {
            let options = s.prefs.power_store.operation_options_for(&key);
            s.power_store.enqueue(QueuedOperation {
                kind,
                key,
                name,
                options,
            });
        }),
        StoreAction::QueueSelected(kind) => {
            with_state_mut(|s| s.power_store.enqueue_selected(kind, &s.prefs.power_store));
        }
        StoreAction::QueueVisibleUpdates => {
            with_state_mut(|s| s.power_store.enqueue_visible_updates(&s.prefs.power_store));
        }
        StoreAction::OpenStoreUpdates => {
            open_shell_uri("ms-windows-store://downloadsandupdates");
        }
        StoreAction::IgnoreSelected => {
            let keys = with_state_mut(|s| s.power_store.take_selected_non_own());
            edit_power_store(hwnd, |prefs| {
                let mut changed = false;
                for key in keys {
                    changed |= prefs.filters.ignore(key);
                }
                changed
            })?;
        }
        StoreAction::SetSelected(key, selected) => {
            with_state_mut(|s| s.power_store.set_selected(key, selected));
        }
        StoreAction::SetSelectAllVisible(select) => {
            with_state_mut(|s| {
                s.power_store
                    .set_select_all_visible(select, &s.prefs.power_store)
            });
        }
        StoreAction::ClearSelection => with_state_mut(|s| s.power_store.clear_selection()),
        StoreAction::OpenInspector(key) => {
            with_state_mut(|s| s.power_store.open_inspector(key));
        }
        StoreAction::ToggleHistoryDrawer => {
            with_state_mut(|s| s.power_store.toggle_history_drawer());
        }
        StoreAction::ToggleSettingsDrawer => {
            with_state_mut(|s| s.power_store.toggle_settings_drawer());
        }
        StoreAction::CloseDrawer => with_state_mut(|s| s.power_store.close_drawer()),
        StoreAction::SaveOptions(key, options) => {
            let id = key.id.clone();
            if edit_power_store(hwnd, |prefs| prefs.set_options(key, options))? {
                with_state_mut(|s| s.power_store.set_notice(Notice::OptionsSaved(id)));
            } else {
                with_state_mut(|s| s.power_store.set_notice(Notice::OptionsRejected));
                window::refresh();
            }
        }
        StoreAction::SkipVersion(key, version) => {
            edit_power_store(hwnd, |prefs| prefs.filters.skip_version(key, version))?;
        }
        StoreAction::Ignore(key) => {
            edit_power_store(hwnd, |prefs| prefs.filters.ignore(key))?;
        }
        StoreAction::Unignore(key) => {
            edit_power_store(hwnd, |prefs| {
                prefs.filters.clear_ignore(&key);
                true
            })?;
        }
        StoreAction::Unskip(key) => {
            edit_power_store(hwnd, |prefs| {
                prefs.filters.clear_skip(&key);
                true
            })?;
        }
        StoreAction::CopyId(id) => {
            ruler::set_clipboard(&id);
            with_state_mut(|s| s.power_store.set_notice(Notice::Copied(id)));
        }
        StoreAction::DismissNotice => {
            with_state_mut(|s| s.power_store.dismiss_notice());
        }
        StoreAction::StepCheckMinutes(up) => {
            edit_power_store(hwnd, |prefs| {
                prefs.check_minutes =
                    crate::power_store::step_check_minutes(prefs.check_minutes, up);
                true
            })?;
            with_state_mut(|s| s.power_store.reschedule(s.prefs.power_store.check_minutes));
        }
        StoreAction::SetNotify(notify) => {
            edit_power_store(hwnd, |prefs| {
                let changed = prefs.notify != notify;
                prefs.notify = notify;
                changed
            })?;
        }
        StoreAction::SetRemoteIcons(enabled) => {
            edit_power_store(hwnd, |prefs| {
                let changed = prefs.remote_icons != enabled;
                prefs.remote_icons = enabled;
                changed
            })?;
            with_state_mut(|s| {
                s.power_store
                    .set_remote_icons(s.prefs.power_store.remote_icons);
                // Reload so database-assigned logos appear or disappear immediately.
                s.power_store.refresh_installed(true);
            });
        }
        StoreAction::SetManagerEnabled(kind, enabled) => {
            edit_manager(hwnd, kind, true, |settings| settings.enabled = enabled)?;
        }
        StoreAction::OpenManagerDrawer(kind) => {
            with_state_mut(|s| s.power_store.open_manager_drawer(kind));
        }
        StoreAction::ShowSettingsDrawer => {
            with_state_mut(|s| s.power_store.show_settings_drawer());
        }
        StoreAction::PickManagerExecutable(kind) => {
            let t = ui_strings();
            if let Some(path) = power_store::file_dialog::open_program(
                power_store_owner(),
                t.power_store_manager_pick,
            ) {
                let path = path.display().to_string();
                edit_manager(hwnd, kind, true, |settings| {
                    settings.executable = Some(path)
                })?;
            }
        }
        StoreAction::ResetManagerExecutable(kind) => {
            edit_manager(hwnd, kind, true, |settings| settings.executable = None)?;
        }
        StoreAction::SetManagerScope(kind, scope) => {
            edit_manager(hwnd, kind, false, |settings| settings.scope = scope)?;
        }
        StoreAction::SetManagerArchitecture(kind, architecture) => {
            edit_manager(hwnd, kind, false, |settings| {
                settings.architecture = architecture
            })?;
        }
        StoreAction::SetManagerTriplet(kind, triplet) => {
            edit_manager(hwnd, kind, false, |settings| settings.triplet = triplet)?;
        }
        StoreAction::Export => {
            let t = ui_strings();
            if let Some(path) = power_store::file_dialog::save_json(
                power_store_owner(),
                t.power_store_export,
                "powerkit-packages.json",
            ) {
                with_state_mut(|s| s.power_store.export(path, &s.prefs.power_store));
            }
        }
        StoreAction::Import => {
            let t = ui_strings();
            if let Some(path) =
                power_store::file_dialog::open_json(power_store_owner(), t.power_store_import)
            {
                let parsed = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| crate::power_store::parse_import(&text));
                with_state_mut(|s| match parsed {
                    Some(packages) => s.power_store.load_import(packages),
                    None => s
                        .power_store
                        .set_notice(Notice::ImportFailed(path.display().to_string())),
                });
            }
        }
        StoreAction::SetImport(key, selected) => {
            with_state_mut(|s| s.power_store.set_import(&key, selected));
        }
        StoreAction::ClearImport => with_state_mut(|s| s.power_store.clear_import()),
        StoreAction::InstallImport => {
            let packages = with_state_mut(|s| s.power_store.take_import_selection());
            edit_power_store(hwnd, |prefs| {
                let mut changed = false;
                for package in packages
                    .iter()
                    .filter(|package| !package.options.is_empty())
                {
                    changed |= prefs.set_options(package.key.clone(), package.options.clone());
                }
                changed
            })?;
            with_state_mut(|s| {
                for package in packages {
                    let options = s.prefs.power_store.operation_options_for(&package.key);
                    s.power_store.enqueue(QueuedOperation {
                        kind: OperationKind::Install,
                        name: package.key.id.clone(),
                        key: package.key,
                        options,
                    });
                }
            });
        }
        StoreAction::ClearHistory => with_state_mut(|s| s.power_store.clear_history()),
        StoreAction::RemovePending(index) => {
            with_state_mut(|s| s.power_store.remove_pending(index));
        }
    }
    Ok(())
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
    let ruler_active = ruler::active();
    let session = awake::session();
    let always_on_hotkey = awake::hotkey_label();
    let bound_app = awake::bound_app();
    let hero_phase = window::hero_phase();
    with_state(|s| {
        let prefs = &s.prefs;
        let ui_language = crate::locale::UiLanguage::resolve(prefs.language);
        let t = ui_language.strings();
        let top_status = match top::session().title() {
            Some(title) => format!("{} ({title})", t.tray_always_top_on),
            None => t.tray_always_top_off.into(),
        };
        let hint = if let Some(warning) = &s.warning {
            warning.clone()
        } else if s.tray_available && prefs.show_tray {
            t.hint_tray.into()
        } else if s.tray_available {
            t.hint_tray_hidden.into()
        } else {
            t.hint_no_tray.into()
        };
        window::Snapshot {
            status,
            mode: window::mode_index(session),
            display: prefs.display,
            interval_minutes: prefs.interval_minutes,
            always_on_hotkey: always_on_hotkey.unwrap_or_else(|| prefs.always_on_hotkey.label()),
            always_on_warn: prefs.always_on_warn,
            ac_only: prefs.always_on_battery.ac_only,
            battery_floor: prefs.always_on_battery.floor,
            bound_app,
            top_status,
            sound: prefs.sound,
            capturing: s.capturing.map(|capturing| match capturing {
                Capturing::Hotkey(command) => window::CaptureTarget::Hotkey(command),
                Capturing::MouseCustom { trigger, slot } => {
                    window::CaptureTarget::MouseCustom { trigger, slot }
                }
                Capturing::MouseAddFKey => window::CaptureTarget::MouseAddFKey,
            }),
            pin_hotkey: prefs.pin.label(),
            opacity_up_hotkey: prefs.opacity_up.label(),
            opacity_down_hotkey: prefs.opacity_down.label(),
            frame_accent: prefs.frame_color.is_none(),
            frame_color: prefs.frame_color.unwrap_or_else(frame::accent_color),
            frame_opacity: prefs.frame_opacity,
            frame_thickness: prefs.frame_thickness,
            ruler_active,
            ruler_hotkey: prefs.ruler_hotkey.label(),
            ruler_mode: window::mode_id(prefs.ruler_mode),
            ruler_unit: window::unit_id(prefs.ruler_unit),
            ruler_continuous: prefs.ruler_continuous,
            ruler_per_channel: prefs.ruler_per_channel,
            ruler_feet: prefs.ruler_feet,
            ruler_tolerance: prefs.ruler_tolerance,
            ruler_color: prefs.ruler_color,
            resize: prefs.resize.clone(),
            startup: prefs.startup,
            theme: window::theme_id(prefs.theme),
            language: window::language_id(prefs.language),
            show_tray: prefs.show_tray,
            shell_opacity: prefs.shell_opacity,
            rail_accent: theme::shell_accent(prefs.theme),
            ui_language,
            hint,
            power_store: s.power_store.snapshot(&s.prefs.power_store),
            media_renamer: s.media_renamer.snapshot(&s.prefs.rename),
            mouse: prefs.mouse.clone(),
            mouse_elevated: power_mouse::elevated_foreground(),
            modules: prefs.modules,
            hero_phase,
        }
    })
}
fn fail_closed(hwnd: HWND, error: io::Error) {
    let message = error.to_string();
    with_state_mut(|s| {
        s.fault = Some(message);
        s.power_store.shutdown();
        s.media_renamer.shutdown();
    });
    // Release BEFORE displaying any modal error dialog.
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
    let mut prefs = with_state(|s| s.prefs.clone());
    let minutes = crate::always_on::clamp_interval(i64::from(minutes));
    if prefs.interval_minutes == minutes {
        return Ok(());
    }
    prefs.interval_minutes = minutes;
    persist(hwnd, prefs)?;
    match awake::session() {
        Session::Interval { custom: true, .. } => {
            awake::start(hwnd, Session::interval(awake::uptime_ms(), minutes, true))
        }
        _ => Ok(()),
    }
}
/// Toggles the display preference: power first, then the file, undoing power if saving fails.
fn toggle_display(hwnd: HWND) -> io::Result<()> {
    let mut prefs = with_state(|s| s.prefs.clone());
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
fn step_battery_floor(hwnd: HWND, delta: i32) -> io::Result<()> {
    step_ruler(hwnd, |prefs| {
        let floor = i32::from(prefs.always_on_battery.floor) + delta;
        prefs.always_on_battery.floor = crate::always_on::clamp_battery_floor(floor);
    })
}
fn wide_from_lparam(l: LPARAM) -> Option<String> {
    if l == 0 {
        return None;
    }
    let ptr = l as *const u16;
    let mut len = 0;
    // SAFETY: lParam is a NUL-terminated setting name for the duration of the handler.
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 256 {
                return None;
            }
        }
        String::from_utf16(std::slice::from_raw_parts(ptr, len)).ok()
    }
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
    if operation != NIM_DELETE && !with_state(|s| s.prefs.show_tray) {
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
    if !with_state(|s| s.tray_available && s.prefs.show_tray) {
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
    let show = with_state(|s| s.prefs.show_tray);
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
    let (display, interval_minutes, sound, lang, power_store, renamer, modules, mouse_enabled) =
        with_state(|s| {
            (
                s.prefs.display,
                s.prefs.interval_minutes,
                s.prefs.sound,
                crate::locale::UiLanguage::resolve(s.prefs.language),
                menu::PowerStoreMenu {
                    updates: s.power_store.update_count(&s.prefs.power_store.filters),
                    checking: s.power_store.checking(),
                },
                s.media_renamer.tray_status(),
                s.prefs.modules,
                s.prefs.mouse.enabled,
            )
        });
    let session = awake::session();
    let status = awake::status(lang.strings());
    let expiry = awake::expiry_label();
    let bound_app = awake::bound_app();
    let pin_hotkey = top::hotkey_label();
    let ruler_hotkey = ruler::hotkey_label();
    let (opacity_up, opacity_down) = top::opacity_hotkeys();
    if let Some(command) = menu::track(
        hwnd,
        &menu::tray_entries(menu::TrayMenus {
            always_on: menu::AlwaysOnMenu {
                session,
                display,
                interval_minutes,
                status: &status,
                expiry: expiry.as_deref(),
                bound_app: bound_app.as_deref(),
            },
            always_top: menu::AlwaysTopMenu {
                pin: &top::session(),
                hotkey: pin_hotkey.as_deref(),
                sound,
                opacity_up: opacity_up.as_deref(),
                opacity_down: opacity_down.as_deref(),
            },
            ruler: menu::ScreenRulerMenu {
                active: ruler::active(),
                hotkey: ruler_hotkey.as_deref(),
            },
            power_store,
            media_renamer: renamer,
            modules,
            mouse_enabled,
            lang,
        }),
    ) {
        unsafe {
            PostMessageW(hwnd, WM_COMMAND, command, 0);
        }
    }
}
fn on_command(hwnd: HWND, id: usize) -> LRESULT {
    let Some(command) = AppCommand::from_id(id) else {
        return 0;
    };
    if !module_allows(command) {
        return 0;
    }
    let result = match command {
        AppCommand::OpenSettings => {
            present(hwnd);
            return 0;
        }
        AppCommand::Exit => {
            unsafe {
                DestroyWindow(hwnd);
            }
            return 0;
        }
        AppCommand::ToggleModuleAlwaysOn
        | AppCommand::ToggleModuleAlwaysTop
        | AppCommand::ToggleModuleScreenRuler
        | AppCommand::ToggleModuleImageResizer
        | AppCommand::ToggleModulePowerStore
        | AppCommand::ToggleModuleMediaRenamer
        | AppCommand::ToggleMouse => toggle_module(hwnd, command),
        AppCommand::TogglePin => {
            let result = top::toggle(hwnd).map(|_| ());
            if result.is_ok() {
                update_view(hwnd, true);
            }
            result
        }
        AppCommand::ToggleSound => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.sound = !prefs.sound;
            persist(hwnd, prefs)
        }
        AppCommand::ToggleStartup => {
            let mut prefs = with_state(|s| s.prefs.clone());
            let previous = prefs.startup;
            prefs.startup = !previous;
            // Registration first, then the file; undo the registration if saving fails.
            startup::set_enabled(prefs.startup).and_then(|()| {
                persist(hwnd, prefs).inspect_err(|_| {
                    let _ = startup::set_enabled(previous);
                })
            })
        }
        AppCommand::ToggleShowTray => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.show_tray = !prefs.show_tray;
            match persist(hwnd, prefs).and_then(|_| apply_tray_preference(hwnd)) {
                Ok(()) => {
                    update_view(hwnd, true);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        AppCommand::OpenColorSettings => {
            open_shell_uri("ms-settings:colors");
            Ok(())
        }
        AppCommand::ApplyShellOpacity => match window::take_shell_opacity() {
            Some(opacity) => {
                let mut prefs = with_state(|s| s.prefs.clone());
                prefs.shell_opacity = crate::settings::clamp_shell_opacity(opacity);
                persist(hwnd, prefs)
            }
            None => Ok(()),
        },
        command if command.theme().is_some() => {
            let mut prefs = with_state(|s| s.prefs.clone());
            if let Some(theme) = command.theme() {
                prefs.theme = theme;
            }
            persist(hwnd, prefs)
        }
        command if command.language().is_some() => {
            let mut prefs = with_state(|s| s.prefs.clone());
            if let Some(language) = command.language() {
                prefs.language = language;
            }
            persist(hwnd, prefs).inspect(|_| resize::register_context_menu())
        }
        AppCommand::OpacityUp => {
            let _ = top::step_pinned_opacity(crate::always_top::OPACITY_STEP);
            Ok(())
        }
        AppCommand::OpacityDown => {
            let _ = top::step_pinned_opacity(-crate::always_top::OPACITY_STEP);
            Ok(())
        }
        AppCommand::ToggleMeasure => ruler::toggle(hwnd).inspect(|_| update_view(hwnd, true)),
        AppCommand::ApplyResizeFilename => {
            save_resize_filename(hwnd);
            return 0;
        }
        AppCommand::ResizeOpen => {
            resize::open(hwnd, Vec::new(), current_resize());
            Ok(())
        }
        AppCommand::ToggleResizeShrink => step_ruler(hwnd, |prefs| {
            prefs.resize.shrink_only = !prefs.resize.shrink_only;
        }),
        AppCommand::ToggleResizeReplace => step_ruler(hwnd, |prefs| {
            prefs.resize.replace = !prefs.resize.replace;
        }),
        AppCommand::ToggleResizeOrientation => step_ruler(hwnd, |prefs| {
            prefs.resize.ignore_orientation = !prefs.resize.ignore_orientation;
        }),
        AppCommand::ToggleResizeMetadata => step_ruler(hwnd, |prefs| {
            prefs.resize.remove_metadata = !prefs.resize.remove_metadata;
        }),
        AppCommand::ToggleResizeKeepDate => step_ruler(hwnd, |prefs| {
            prefs.resize.keep_date = !prefs.resize.keep_date;
        }),
        AppCommand::ResizeJpegDown => step_ruler(hwnd, |prefs| {
            prefs.resize.jpeg_quality = (prefs.resize.jpeg_quality - 1).max(1);
        }),
        AppCommand::ResizeJpegUp => step_ruler(hwnd, |prefs| {
            prefs.resize.jpeg_quality = (prefs.resize.jpeg_quality + 1).min(100);
        }),
        AppCommand::ResizeTiffDown => step_ruler(hwnd, |prefs| {
            prefs.resize.tiff = prefs.resize.tiff.next(-1);
        }),
        AppCommand::ResizeTiffUp => step_ruler(hwnd, |prefs| {
            prefs.resize.tiff = prefs.resize.tiff.next(1);
        }),
        command if command.resize_encoder().is_some() => step_ruler(hwnd, |prefs| {
            if let Some(encoder) = command.resize_encoder() {
                prefs.resize.fallback = encoder;
            }
        }),
        command if command.resize_png().is_some() => step_ruler(hwnd, |prefs| {
            if let Some(png) = command.resize_png() {
                prefs.resize.png = png;
            }
        }),
        AppCommand::ToggleDisplay => toggle_display(hwnd),
        AppCommand::ToggleAlwaysOnWarn => step_ruler(hwnd, |prefs| {
            prefs.always_on_warn = !prefs.always_on_warn;
        }),
        AppCommand::ToggleAcOnly => step_ruler(hwnd, |prefs| {
            prefs.always_on_battery.ac_only = !prefs.always_on_battery.ac_only;
        }),
        AppCommand::BatteryFloorDown => {
            step_battery_floor(hwnd, -i32::from(crate::always_on::BATTERY_FLOOR_STEP))
        }
        AppCommand::BatteryFloorUp => {
            step_battery_floor(hwnd, i32::from(crate::always_on::BATTERY_FLOOR_STEP))
        }
        AppCommand::AlwaysOnExtend => awake::extend(hwnd),
        AppCommand::AlwaysOnBindProcess => awake::bind_last(hwnd),
        AppCommand::CapturePinHotkey
        | AppCommand::CaptureOpacityUpHotkey
        | AppCommand::CaptureOpacityDownHotkey
        | AppCommand::CaptureRulerHotkey
        | AppCommand::CaptureAlwaysOnHotkey => {
            begin_capture(hwnd, command);
            Ok(())
        }
        AppCommand::ToggleFrameAccent => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.frame_color = if prefs.frame_color.is_none() {
                Some(frame::accent_color())
            } else {
                None
            };
            persist(hwnd, prefs)
        }
        AppCommand::PickFrameColor => match window::take_color() {
            Some(color) => {
                let mut prefs = with_state(|s| s.prefs.clone());
                prefs.frame_color = Some(color);
                persist(hwnd, prefs)
            }
            None => Ok(()),
        },
        AppCommand::FrameOpacityDown => step_frame(hwnd, -10, 0),
        AppCommand::FrameOpacityUp => step_frame(hwnd, 10, 0),
        AppCommand::FrameThicknessDown => step_frame(hwnd, 0, -1),
        AppCommand::FrameThicknessUp => step_frame(hwnd, 0, 1),
        AppCommand::ToggleRulerContinuous => step_ruler(hwnd, |prefs| {
            prefs.ruler_continuous = !prefs.ruler_continuous;
        }),
        AppCommand::ToggleRulerChannel => step_ruler(hwnd, |prefs| {
            prefs.ruler_per_channel = !prefs.ruler_per_channel;
        }),
        AppCommand::ToggleRulerFeet => {
            step_ruler(hwnd, |prefs| prefs.ruler_feet = !prefs.ruler_feet)
        }
        AppCommand::RulerToleranceDown => step_ruler(hwnd, |prefs| {
            prefs.ruler_tolerance = prefs.ruler_tolerance.saturating_sub(1);
        }),
        AppCommand::RulerToleranceUp => step_ruler(hwnd, |prefs| {
            prefs.ruler_tolerance = prefs.ruler_tolerance.saturating_add(1);
        }),
        AppCommand::PickRulerColor => match window::take_color() {
            Some(color) => step_ruler(hwnd, |prefs| prefs.ruler_color = color),
            None => Ok(()),
        },
        command if command.ruler_mode().is_some() => step_ruler(hwnd, |prefs| {
            if let Some(mode) = command.ruler_mode() {
                prefs.ruler_mode = mode;
            }
        }),
        command if command.ruler_unit().is_some() => step_ruler(hwnd, |prefs| {
            if let Some(unit) = command.ruler_unit() {
                prefs.ruler_unit = unit;
            }
        }),
        AppCommand::AlwaysOnExpiry => match window::take_expiry() {
            Some(pick) => start_expiry(hwnd, pick),
            None => Ok(()),
        },
        AppCommand::ApplyInterval => match window::take_interval() {
            Some(minutes) => apply_interval(hwnd, minutes),
            None => Ok(()),
        },
        AppCommand::PowerStoreRefresh => {
            power_store_refresh(hwnd);
            Ok(())
        }
        AppCommand::PowerStoreEnsureLoaded => {
            with_state_mut(|s| {
                s.power_store
                    .set_remote_icons(s.prefs.power_store.remote_icons);
                s.power_store.ensure_loaded();
            });
            update_view(hwnd, true);
            Ok(())
        }
        AppCommand::PowerStoreAction => {
            let mut result = Ok(());
            for action in window::take_power_store_actions() {
                result = result.and(power_store_action(hwnd, action));
            }
            update_view(hwnd, true);
            result
        }
        AppCommand::MediaRenamerAction => {
            let mut result = Ok(());
            for action in window::take_media_renamer_actions() {
                result = result.and(media_renamer_action(hwnd, action));
            }
            update_view(hwnd, true);
            result
        }
        AppCommand::MediaRenamerOpen => {
            open_media_renamer(hwnd);
            Ok(())
        }
        AppCommand::PowerMouseAction => {
            let mut result = Ok(());
            for action in window::take_power_mouse_actions() {
                result = result.and(power_mouse_action(hwnd, action));
            }
            result
        }
        AppCommand::PowerStoreCheckNow => {
            with_state_mut(|s| s.power_store.check_now());
            update_view(hwnd, true);
            Ok(())
        }
        AppCommand::PowerStoreOpen => {
            open_power_store(hwnd);
            Ok(())
        }
        AppCommand::PowerStoreCancelPending => {
            with_state_mut(|s| s.power_store.clear_pending());
            update_view(hwnd, true);
            Ok(())
        }
        AppCommand::PowerStoreUpdateAll => {
            with_state_mut(|s| s.power_store.enqueue_updates(&s.prefs.power_store));
            update_view(hwnd, true);
            Ok(())
        }
        AppCommand::PowerStoreCancelRunning => {
            with_state_mut(|s| s.power_store.cancel_running());
            update_view(hwnd, true);
            Ok(())
        }
        command => {
            let interval = with_state(|s| s.prefs.interval_minutes);
            match command.always_on_session(awake::uptime_ms(), interval) {
                Some(next) => awake::start(hwnd, next),
                None => Ok(()),
            }
        }
    };
    if let Err(e) = result {
        if with_state(|s| s.smoke) {
            fail_closed(hwnd, e);
        } else {
            report_error(&e);
        }
    }
    0
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
        let requests = with_state(|s| s.request_message);
        if requests != 0 && msg == requests {
            if let Some(request) = Request::decode(w, l) {
                apply_request(hwnd, request);
            }
            return 0;
        }
        match msg {
            CALLBACK if l as u32 == NIN_BALLOONUSERCLICK => {
                let kind = with_state_mut(|s| std::mem::take(&mut s.balloon));
                match kind {
                    Balloon::AlwaysOnEnding => {
                        on_command(hwnd, AppCommand::AlwaysOnExtend.id());
                    }
                    Balloon::PowerStoreUpdates => {
                        open_power_store(hwnd);
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
                top::remember_foreground(hwnd);
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
                winui_host::apply_shell_opacity(with_state(|s| s.prefs.shell_opacity));
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
                if let Some(capturing) = with_state(|s| s.capturing) {
                    let vk = w as u16;
                    if vk == VK_ESCAPE {
                        cancel_capture(hwnd);
                    } else {
                        match capturing {
                            Capturing::MouseAddFKey => finish_f_key_capture(hwnd, vk),
                            Capturing::Hotkey(_) | Capturing::MouseCustom { .. } => {
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
            WM_SETTINGCHANGE => {
                if icons::theme_setting(wide_from_lparam(l).as_deref()) {
                    // Refreshes the tray icon and the shell rail accent together.
                    update_view(hwnd, true);
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_POWERBROADCAST if w == PBT_APMSUSPEND as usize => {
                power_mouse::on_suspend();
                1
            }
            WM_POWERBROADCAST
                if w == PBT_APMRESUMEAUTOMATIC as usize || w == PBT_APMRESUMESUSPEND as usize =>
            {
                power_mouse::on_resume();
                if let Err(e) = awake::reconcile(hwnd) {
                    fail_closed(hwnd, e);
                }
                1
            }
            WM_POWERBROADCAST if w == PBT_APMPOWERSTATUSCHANGE as usize => {
                awake::enforce_policy(hwnd);
                1
            }
            WM_TIMECHANGE => {
                if let Err(e) = awake::reconcile(hwnd) {
                    fail_closed(hwnd, e);
                }
                0
            }
            WM_HOTKEY if w as i32 == awake::HOTKEY => {
                if let Err(error) = awake::toggle(hwnd) {
                    report_error(&error);
                }
                0
            }
            WM_HOTKEY if w as i32 == top::HOTKEY_PIN => {
                match top::toggle_hotkey(hwnd) {
                    Ok(_) => update_view(hwnd, true),
                    Err(e) => report_error(&e),
                }
                0
            }
            WM_HOTKEY if w as i32 == top::HOTKEY_OPACITY_UP => {
                let _ = top::step_foreground_opacity(crate::always_top::OPACITY_STEP);
                0
            }
            WM_HOTKEY if w as i32 == top::HOTKEY_OPACITY_DOWN => {
                let _ = top::step_foreground_opacity(-crate::always_top::OPACITY_STEP);
                0
            }
            WM_HOTKEY if w as i32 == ruler::HOTKEY => {
                if let Err(error) = ruler::toggle(hwnd) {
                    report_error(&error);
                }
                update_view(hwnd, true);
                0
            }
            WM_COMMAND if (w >> 16) == 0 => on_command(hwnd, w & 0xffff),
            WM_COPYDATA => {
                // SAFETY: WM_COPYDATA keeps the sender's COPYDATASTRUCT alive until this
                // handler returns; the paths are copied out before returning.
                let rename = (l != 0)
                    .then(|| {
                        media_renamer::handoff::accept(
                            &*(l
                                as *const windows_sys::Win32::System::DataExchange::COPYDATASTRUCT),
                        )
                    })
                    .flatten();
                if let Some(paths) = rename {
                    with_state_mut(|s| s.rename_handoff.get_or_insert_default().extend(paths));
                    PostMessageW(hwnd, media_renamer::handoff::HANDOFF, 0, 0);
                } else {
                    resize::accept_lparam(l);
                    PostMessageW(hwnd, resize::SHOW, 0, 0);
                }
                1
            }
            media_renamer::handoff::HANDOFF => {
                receive_rename_handoff(hwnd);
                0
            }
            media_renamer::WAKE => {
                if with_state_mut(|s| s.media_renamer.poll()) {
                    update_view(hwnd, true);
                }
                0
            }
            resize::SHOW => {
                resize::open_pending(hwnd, current_resize());
                0
            }
            resize::JOB => {
                resize::on_job();
                0
            }
            WM_TIMER if w == ruler::TIMER => {
                ruler::on_timer();
                0
            }
            WM_TIMER if w == POLL_TIMER => {
                let (store_changed, found, notify) = with_state_mut(|s| {
                    let power_store_on = s.prefs.modules.power_store;
                    let renamer_on = s.prefs.modules.media_renamer;
                    if power_store_on {
                        s.power_store
                            .tick(std::time::Instant::now(), s.prefs.power_store.check_minutes);
                    }
                    let (changed, found) = if power_store_on {
                        s.power_store.poll(&s.prefs.power_store.filters)
                    } else {
                        (false, None)
                    };
                    let renamed = if renamer_on {
                        s.media_renamer.poll()
                    } else {
                        false
                    };
                    (
                        changed || renamed,
                        found,
                        power_store_on && s.prefs.power_store.notify,
                    )
                });
                top::remember_foreground(hwnd);
                if top::reconcile() || store_changed {
                    update_view(hwnd, true);
                }
                if let Some(count) = found.filter(|_| notify) {
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
                    s.power_store.shutdown();
                    s.media_renamer.shutdown();
                });
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
                    s.power_store.shutdown();
                    s.media_renamer.shutdown();
                });
                if let Err(e) = awake::shutdown(hwnd) {
                    with_state_mut(|s| s.fault = Some(e.to_string()));
                }
                top::shutdown();
                ruler::shutdown();
                power_mouse::shutdown();
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
pub fn run() -> io::Result<()> {
    // SAFETY: handles and buffers stay alive throughout the message loop; cleanup is on this thread.
    unsafe {
        let smoke = std::env::args().any(|a| a == "--smoke-test");
        let idle_test = smoke && std::env::args().any(|a| a == "--idle");
        let mutex_name = if smoke {
            format!("Local\\PowerKit.Smoke.{}", std::process::id())
        } else {
            "Local\\PowerKit.Singleton".into()
        };
        let mutex = CreateMutexW(null(), 0, wide(&mutex_name).as_ptr());
        if mutex.is_null() {
            return Err(io::Error::last_os_error());
        }
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        let _mutex = Handle(mutex);
        let args: Vec<String> = std::env::args().collect();
        let request = if smoke { None } else { Request::parse(&args) };
        let request_message = RegisterWindowMessageW(wide(REQUEST_MESSAGE).as_ptr());
        if already_running {
            if let Some(files) = resize::command_files() {
                return resize::forward(&files);
            }
            if let Some(paths) = media_renamer::handoff::command_paths() {
                return media_renamer::handoff::forward(&paths);
            }
            let existing = FindWindowW(wide("PowerKit.Window").as_ptr(), null());
            if !existing.is_null() {
                match request {
                    Some(request) if request_message != 0 => {
                        let (kind, value) = request.encode();
                        PostMessageW(existing, request_message, kind, value);
                    }
                    _ => {
                        PostMessageW(existing, WM_COMMAND, OPEN, 0);
                    }
                }
            }
            return Ok(());
        }
        let _power = PowerGuard::default();
        let _pin = top::PinGuard::default();
        let (prefs, warning) = if smoke {
            (
                crate::settings::Preferences {
                    sound: false,
                    ..crate::settings::Preferences::default()
                },
                None,
            )
        } else {
            // The warning text is chosen once the loaded language is in STATE.
            type Pick = fn(&crate::locale::Strings) -> &'static str;
            match load_prefs() {
                Ok(loaded) if loaded.dropped > 0 => {
                    (loaded.prefs, Some((|t| t.warn_settings_partial) as Pick))
                }
                Ok(loaded) => (loaded.prefs, None),
                Err(_) => (
                    crate::settings::Preferences::default(),
                    Some((|t| t.warn_settings_unreadable) as Pick),
                ),
            }
        };
        let show_tray = prefs.show_tray;
        let run_at_startup = prefs.startup;
        with_state_mut(|s| {
            s.prefs = prefs.clone();
            s.smoke = smoke;
            s.taskbar_message = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
            s.request_message = request_message;
        });
        if let Some(pick) = warning {
            let warning = pick(ui_strings()).to_string();
            with_state_mut(|s| s.warning = Some(warning));
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
        with_state_mut(|s| {
            s.media_renamer = media_renamer::Session::new(hwnd as isize, &prefs.rename)
        });
        apply_feature_prefs(hwnd, &prefs);
        // Self-test must not add an icon to, or change preferences in, the user's desktop.
        if !smoke {
            top::listen(hwnd);
            ruler::listen(hwnd);
            awake::listen(hwnd);
            power_mouse::listen(hwnd);
            if !package::is_packaged() {
                resize::register_context_menu();
                media_renamer::handoff::register_context_menu();
            }
        }
        if !smoke && let Err(error) = startup::set_enabled(run_at_startup) {
            let warning = format!("{} ({error})", ui_strings().warn_startup);
            with_state_mut(|s| s.warning = Some(warning));
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
                    s.power_store
                        .load_history(path.with_file_name(POWER_STORE_HISTORY_FILE));
                    s.media_renamer.load_history(
                        path.with_file_name(crate::media_renamer::history::FILE_NAME),
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
            let resize_files = resize::command_files();
            let rename_paths = media_renamer::handoff::command_paths();
            if let Some(request) = request {
                apply_request(hwnd, request);
            }
            if let Some(paths) = rename_paths.clone() {
                with_state_mut(|s| {
                    s.rename_handoff_at = Some(std::time::Instant::now());
                    s.media_renamer.replace(paths);
                });
                window::show_page(window::Page::MediaRenamer);
                update_view(hwnd, true);
            }
            let host = winui_host::install(
                hwnd,
                resize_files.is_some() || request.is_some() || rename_paths.is_some(),
            );
            if let Some(files) = resize_files {
                // Queue for the WinUI shell; falls back to Win32 if reactor fails.
                resize_dialog::queue(resize_dialog::Seed {
                    app: hwnd as isize,
                    files,
                    settings: current_resize(),
                });
            }
            let settings = window::run(hwnd);
            drop(host);
            if IsWindow(hwnd) != 0 {
                // WinUI did not start (for example the runtime is missing) or ended
                // without app teardown. Tray-only is not a product mode: tell the
                // user, finish a pending --resize via Win32 if needed, then exit.
                let warning = match settings {
                    Ok(()) => ui_strings().settings_unavailable.to_string(),
                    Err(error) => {
                        format!("{} ({error})", ui_strings().settings_unavailable)
                    }
                };
                with_state_mut(|s| {
                    s.settings_unavailable = true;
                    s.warning = Some(warning.clone());
                });
                report(&warning);
                if let Some(seed) = resize_dialog::take_pending() {
                    resize::open_win32(hwnd, seed.files, seed.settings);
                }
                DestroyWindow(hwnd);
                pump(hwnd)?;
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
/// Plain Win32 loop for the smoke test, and to drain teardown when WinUI is absent.
fn pump(hwnd: HWND) -> io::Result<()> {
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
                return Ok(());
            }
            let dialog_handled =
                resize::live().is_some_and(|dialog| IsDialogMessageW(dialog, &message) != 0);
            if !dialog_handled {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}
