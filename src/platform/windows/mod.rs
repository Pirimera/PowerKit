//! Native Win32 boundary. All UI and power calls run on the message-loop thread.
mod frame;
mod icons;
mod menu;
mod power;
mod resize;
mod resize_dialog;
mod resize_io;
mod resize_wic;
mod ruler;
mod shell_package;
mod smoke;
mod startup;
mod theme;
mod top;
mod window;
mod winui_host;
use crate::always_on::Mode;
use icons::IconSet;
use power::{PowerGuard, power};
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
const TIMER: usize = 1;
const SMOKE: usize = 2;
const UPDATE_TIMER: usize = 3;
const OFF: usize = 10;
const FOREVER: usize = 11;
const HALF_HOUR: usize = 12;
const HOUR: usize = 13;
const TWO_HOURS: usize = 14;
const DISPLAY: usize = 15;
const EXIT: usize = 16;
const OPEN: usize = 17;
const UPDATE: usize = 18;
const PIN: usize = 20;
const SOUND: usize = 21;
const OPACITY_UP: usize = 22;
const OPACITY_DOWN: usize = 23;
const MEASURE: usize = 24;
const RESIZE: usize = 25;
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
fn ui_strings() -> &'static crate::locale::Strings {
    with_state(|s| crate::locale::UiLanguage::resolve(s.prefs.language).strings())
}
#[derive(Default)]
struct State {
    mode: Mode,
    display: bool,
    taskbar_message: u32,
    tray_available: bool,
    smoke: bool,
    fault: Option<String>,
    warning: Option<String>,
    capturing: Option<usize>,
    settings_unavailable: bool,
    last_caption: String,
    smoke_stage: u8,
    updates: crate::updates::Updates,
    update_on_exit: bool,
    restart_for_update: bool,
    last_update_key: u8,
    last_update_ready: bool,
    icons: IconSet,
    timed_choice: Option<usize>,
    sound: bool,
    prefs: crate::settings::Preferences,
}
fn with_state<R>(f: impl FnOnce(&State) -> R) -> R {
    STATE.with(|s| f(&s.borrow()))
}
fn with_state_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}
fn command_mode(id: usize, now_ms: u64) -> Option<Mode> {
    Some(match id {
        OFF => Mode::Off,
        FOREVER => Mode::Indefinite,
        HALF_HOUR => Mode::timed(now_ms, 30 * 60_000),
        HOUR => Mode::timed(now_ms, 60 * 60_000),
        TWO_HOURS => Mode::timed(now_ms, 120 * 60_000),
        _ => return None,
    })
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn settings_path() -> io::Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::other("LOCALAPPDATA bulunamadı"))?,
    )
    .join("PowerKit")
    .join("settings.txt"))
}
fn load_prefs() -> io::Result<crate::settings::Preferences> {
    crate::settings::load(&settings_path()?)
}
fn save_prefs(prefs: crate::settings::Preferences) -> io::Result<()> {
    crate::settings::save(&settings_path()?, prefs)
}
fn persist(hwnd: HWND, prefs: crate::settings::Preferences) -> io::Result<()> {
    save_prefs(prefs.clone())?;
    let display = prefs.display;
    let sound = prefs.sound;
    with_state_mut(|s| {
        s.prefs = prefs;
        s.display = display;
        s.sound = sound;
        s.warning = None;
        s.capturing = None;
    });
    winui_host::end_key_capture();
    let prefs = with_state(|s| s.prefs.clone());
    top::apply_prefs(&prefs);
    ruler::apply_prefs(&prefs);
    update_view(hwnd, true);
    Ok(())
}
fn begin_capture(hwnd: HWND, id: usize) {
    with_state_mut(|s| s.capturing = Some(id));
    top::pause_hotkeys();
    ruler::pause_hotkeys();
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
    top::apply_prefs(&prefs);
    ruler::apply_prefs(&prefs);
    update_view(hwnd, true);
}
fn finish_capture(hwnd: HWND, key: crate::always_top::Hotkey) {
    let Some(id) = with_state(|s| s.capturing) else {
        return;
    };
    let mut prefs = with_state(|s| s.prefs.clone());
    match id {
        window::HOTKEY_PIN_EDIT => prefs.pin = key,
        window::HOTKEY_OP_UP_EDIT => prefs.opacity_up = key,
        window::HOTKEY_OP_DOWN_EDIT => prefs.opacity_down = key,
        window::RULER_HOTKEY => prefs.ruler_hotkey = key,
        _ => {}
    }
    let _ = persist(hwnd, prefs).map_err(|error| report(&error.to_string()));
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
    let (always_on, always_top_on, ruler_on) = with_state(|s| {
        let t = crate::locale::UiLanguage::resolve(s.prefs.language).strings();
        let always_on = match s.mode {
            Mode::Off => t.status_off.into(),
            Mode::Indefinite => format!("PowerKit — {}", t.tray_always_on_on),
            Mode::Until(end) => format!(
                "PowerKit — {}: {} {}",
                t.tray_always_on_on,
                end.saturating_sub(unsafe { GetTickCount64() })
                    .div_ceil(60000),
                t.tray_always_on_left
            ),
        };
        (always_on, t.tray_always_top_on, t.ruler_on)
    });
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
        report(&error.to_string());
        window::refresh();
    }
}
/// Shows the settings window, or explains why it cannot exist on this machine.
fn present(_hwnd: HWND) {
    if !winui_host::present() && with_state(|s| s.settings_unavailable && !s.smoke) {
        report(ui_strings().settings_unavailable);
    }
}
/// Owner for modal pickers: the visible settings window, else the hidden app window.
fn dialog_owner(hwnd: HWND) -> HWND {
    winui_host::window().unwrap_or(hwnd)
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
            mode: window::mode_index(s.mode, s.timed_choice),
            display: s.display,
            top_status,
            sound: s.sound,
            capturing: s.capturing,
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
            update_status: s.updates.status(ui_language),
            update_ready: s.updates.ready(),
            startup: prefs.startup,
            theme: window::theme_id(prefs.theme),
            language: window::language_id(prefs.language),
            show_tray: prefs.show_tray,
            auto_update: prefs.auto_update,
            update_notify: prefs.update_notify,
            show_notes_after_update: prefs.show_notes_after_update,
            ui_language,
            hint,
        }
    })
}
fn fail_closed(hwnd: HWND, error: io::Error) {
    let message = error.to_string();
    with_state_mut(|s| s.fault = Some(message));
    // Release BEFORE displaying any modal error dialog.
    let _ = power(Mode::Off, false);
    top::shutdown();
    ruler::shutdown();
    unsafe {
        DestroyWindow(hwnd);
    }
    // main displays the saved error after cleanup.
}
fn reconcile(hwnd: HWND) -> io::Result<()> {
    let (mode, display) = with_state(|s| (s.mode, s.display));
    change(hwnd, mode.at(unsafe { GetTickCount64() }), display)
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
    let (mode, previous) = with_state(|s| (s.mode, s.icons));
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
        Err(io::Error::other("Bildirim alanı simgesi güncellenemedi"))
    } else {
        Ok(())
    }
}
fn tray_balloon(hwnd: HWND, title: &str, body: &str) {
    if !with_state(|s| s.tray_available && s.prefs.show_tray && s.prefs.update_notify) {
        return;
    }
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
fn change(hwnd: HWND, mode: Mode, display: bool) -> io::Result<()> {
    let timed = matches!(mode, Mode::Until(_));
    if timed && unsafe { SetTimer(hwnd, TIMER, 1000, None) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if let Err(e) = power(mode, display) {
        let was_timed = with_state(|s| matches!(s.mode, Mode::Until(_)));
        if !was_timed {
            unsafe {
                KillTimer(hwnd, TIMER);
            }
        }
        return Err(e);
    }
    if !timed {
        unsafe {
            KillTimer(hwnd, TIMER);
        }
    }
    with_state_mut(|s| {
        s.mode = mode;
        s.display = display;
        if !matches!(mode, Mode::Until(_)) {
            s.timed_choice = None;
        }
    });
    update_view(hwnd, true);
    Ok(())
}
fn menu(hwnd: HWND) {
    top::remember_foreground(hwnd);
    let _ = top::reconcile();
    let (mode, display, timed_choice, sound) =
        with_state(|s| (s.mode, s.display, s.timed_choice, s.sound));
    let now = unsafe { GetTickCount64() };
    let remaining = match mode {
        Mode::Until(end) => menu::remaining_minutes(now, end),
        _ => 0,
    };
    let pin_hotkey = top::hotkey_label();
    let ruler_hotkey = ruler::hotkey_label();
    let (opacity_up, opacity_down) = top::opacity_hotkeys();
    if let Some(command) = menu::track(
        hwnd,
        &menu::tray_entries(
            mode,
            display,
            remaining,
            timed_choice,
            menu::AlwaysTopMenu {
                pin: &top::session(),
                hotkey: pin_hotkey.as_deref(),
                sound,
                opacity_up: opacity_up.as_deref(),
                opacity_down: opacity_down.as_deref(),
            },
            menu::ScreenRulerMenu {
                active: ruler::active(),
                hotkey: ruler_hotkey.as_deref(),
            },
            with_state(|s| crate::locale::UiLanguage::resolve(s.prefs.language)),
        ),
    ) {
        unsafe {
            PostMessageW(hwnd, WM_COMMAND, command, 0);
        }
    }
}
fn on_command(hwnd: HWND, id: usize) -> LRESULT {
    let (mode, display, sound) = with_state(|s| (s.mode, s.display, s.sound));
    let result = match id {
        OPEN => {
            present(hwnd);
            return 0;
        }
        UPDATE => {
            let ready = with_state(|s| s.updates.ready());
            if ready && mode.active() {
                report(
                    "Güncellemeyi şimdi uygulamak için önce AlwaysOn'u durdurun. Normal çıkışta güncelleme otomatik uygulanır.",
                );
            } else if ready {
                with_state_mut(|s| {
                    s.update_on_exit = true;
                    s.restart_for_update = true;
                });
                unsafe {
                    DestroyWindow(hwnd);
                }
            } else {
                with_state_mut(|s| s.updates.check(true));
                update_status(hwnd);
                present(hwnd);
            }
            return 0;
        }
        EXIT => {
            with_state_mut(|s| s.update_on_exit = true);
            unsafe {
                DestroyWindow(hwnd);
            }
            return 0;
        }
        PIN => {
            let result = top::toggle(hwnd).map(|_| ());
            if result.is_ok() {
                update_view(hwnd, true);
            }
            result
        }
        SOUND => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.sound = !sound;
            persist(hwnd, prefs)
        }
        window::STARTUP => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.startup = !prefs.startup;
            match startup::set_enabled(prefs.startup) {
                Ok(()) => persist(hwnd, prefs),
                Err(error) => Err(error),
            }
        }
        window::SHOW_TRAY => {
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
        window::AUTO_UPDATE => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.auto_update = !prefs.auto_update;
            let auto = prefs.auto_update;
            match persist(hwnd, prefs) {
                Ok(()) => {
                    with_state_mut(|s| {
                        s.updates.set_background(auto);
                        if auto {
                            s.updates.check(false);
                        }
                    });
                    update_status(hwnd);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        window::UPDATE_NOTIFY => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.update_notify = !prefs.update_notify;
            persist(hwnd, prefs)
        }
        window::SHOW_NOTES => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.show_notes_after_update = !prefs.show_notes_after_update;
            persist(hwnd, prefs)
        }
        window::OPEN_COLOR_SETTINGS => {
            open_shell_uri("ms-settings:colors");
            Ok(())
        }
        id if window::theme_from_id(id).is_some() => {
            let mut prefs = with_state(|s| s.prefs.clone());
            if let Some(theme) = window::theme_from_id(id) {
                prefs.theme = theme;
            }
            persist(hwnd, prefs)
        }
        id if window::language_from_id(id).is_some() => {
            let mut prefs = with_state(|s| s.prefs.clone());
            if let Some(language) = window::language_from_id(id) {
                prefs.language = language;
            }
            persist(hwnd, prefs)
        }
        OPACITY_UP => {
            let _ = top::step_pinned_opacity(crate::always_top::OPACITY_STEP);
            Ok(())
        }
        OPACITY_DOWN => {
            let _ = top::step_pinned_opacity(-crate::always_top::OPACITY_STEP);
            Ok(())
        }
        MEASURE => ruler::toggle(hwnd).inspect(|_| update_view(hwnd, true)),
        window::RESIZE_FILENAME => {
            save_resize_filename(hwnd);
            return 0;
        }
        RESIZE | window::RESIZE_OPEN => {
            resize::open(hwnd, Vec::new(), current_resize());
            Ok(())
        }
        window::RESIZE_SHRINK => step_ruler(hwnd, |prefs| {
            prefs.resize.shrink_only = !prefs.resize.shrink_only;
        }),
        window::RESIZE_REPLACE => step_ruler(hwnd, |prefs| {
            prefs.resize.replace = !prefs.resize.replace;
        }),
        window::RESIZE_ORIENTATION => step_ruler(hwnd, |prefs| {
            prefs.resize.ignore_orientation = !prefs.resize.ignore_orientation;
        }),
        window::RESIZE_METADATA => step_ruler(hwnd, |prefs| {
            prefs.resize.remove_metadata = !prefs.resize.remove_metadata;
        }),
        window::RESIZE_KEEP_DATE => step_ruler(hwnd, |prefs| {
            prefs.resize.keep_date = !prefs.resize.keep_date;
        }),
        window::RESIZE_JPEG_DOWN => step_ruler(hwnd, |prefs| {
            prefs.resize.jpeg_quality = (prefs.resize.jpeg_quality - 1).max(1);
        }),
        window::RESIZE_JPEG_UP => step_ruler(hwnd, |prefs| {
            prefs.resize.jpeg_quality = (prefs.resize.jpeg_quality + 1).min(100);
        }),
        window::RESIZE_TIFF_DOWN => step_ruler(hwnd, |prefs| {
            prefs.resize.tiff = prefs.resize.tiff.next(-1);
        }),
        window::RESIZE_TIFF_UP => step_ruler(hwnd, |prefs| {
            prefs.resize.tiff = prefs.resize.tiff.next(1);
        }),
        id if window::resize_encoder(id).is_some() => step_ruler(hwnd, |prefs| {
            if let Some(encoder) = window::resize_encoder(id) {
                prefs.resize.fallback = encoder;
            }
        }),
        id if window::resize_png(id).is_some() => step_ruler(hwnd, |prefs| {
            if let Some(png) = window::resize_png(id) {
                prefs.resize.png = png;
            }
        }),
        DISPLAY => {
            let mut prefs = with_state(|s| s.prefs.clone());
            let next_display = !display;
            prefs.display = next_display;
            match persist(hwnd, prefs.clone()) {
                Ok(()) => {
                    let result = change(hwnd, mode, next_display);
                    if result.is_err() {
                        let mut rollback = prefs;
                        rollback.display = display;
                        if let Err(error) = persist(hwnd, rollback) {
                            report(&format!(
                                "Güç isteği değişmedi; ayar geri alınamadı: {error}"
                            ));
                            return 0;
                        }
                    }
                    result
                }
                Err(e) => Err(e),
            }
        }
        window::HOTKEY_PIN_EDIT
        | window::HOTKEY_OP_UP_EDIT
        | window::HOTKEY_OP_DOWN_EDIT
        | window::RULER_HOTKEY => {
            begin_capture(hwnd, id);
            Ok(())
        }
        window::FRAME_ACCENT => {
            let mut prefs = with_state(|s| s.prefs.clone());
            prefs.frame_color = if prefs.frame_color.is_none() {
                Some(frame::accent_color())
            } else {
                None
            };
            persist(hwnd, prefs)
        }
        window::FRAME_COLOR => {
            let current = with_state(|s| s.prefs.frame_color).unwrap_or_else(frame::accent_color);
            if let Some(color) = window::pick_color(dialog_owner(hwnd), current) {
                let mut prefs = with_state(|s| s.prefs.clone());
                prefs.frame_color = Some(color);
                persist(hwnd, prefs)
            } else {
                Ok(())
            }
        }
        window::FRAME_OPACITY_DOWN => step_frame(hwnd, -10, 0),
        window::FRAME_OPACITY_UP => step_frame(hwnd, 10, 0),
        window::FRAME_THICK_DOWN => step_frame(hwnd, 0, -1),
        window::FRAME_THICK_UP => step_frame(hwnd, 0, 1),
        window::RULER_CONTINUOUS => step_ruler(hwnd, |prefs| {
            prefs.ruler_continuous = !prefs.ruler_continuous;
        }),
        window::RULER_CHANNEL => step_ruler(hwnd, |prefs| {
            prefs.ruler_per_channel = !prefs.ruler_per_channel;
        }),
        window::RULER_FEET => step_ruler(hwnd, |prefs| prefs.ruler_feet = !prefs.ruler_feet),
        window::RULER_TOLERANCE_DOWN => step_ruler(hwnd, |prefs| {
            prefs.ruler_tolerance = prefs.ruler_tolerance.saturating_sub(1);
        }),
        window::RULER_TOLERANCE_UP => step_ruler(hwnd, |prefs| {
            prefs.ruler_tolerance = prefs.ruler_tolerance.saturating_add(1);
        }),
        window::RULER_COLOR => {
            let current = with_state(|s| s.prefs.ruler_color);
            if let Some(color) = window::pick_color(dialog_owner(hwnd), current) {
                step_ruler(hwnd, |prefs| prefs.ruler_color = color)
            } else {
                Ok(())
            }
        }
        id if window::ruler_mode(id).is_some() => step_ruler(hwnd, |prefs| {
            if let Some(mode) = window::ruler_mode(id) {
                prefs.ruler_mode = mode;
            }
        }),
        id if window::ruler_unit(id).is_some() => step_ruler(hwnd, |prefs| {
            if let Some(unit) = window::ruler_unit(id) {
                prefs.ruler_unit = unit;
            }
        }),
        id => match command_mode(id, unsafe { GetTickCount64() }) {
            Some(next) => {
                let result = change(hwnd, next, display);
                if result.is_ok() {
                    with_state_mut(|s| {
                        s.timed_choice = matches!(s.mode, Mode::Until(_)).then_some(id);
                    });
                }
                result
            }
            None => Ok(()),
        },
    };
    if let Err(e) = result {
        if with_state(|s| s.smoke) {
            fail_closed(hwnd, e);
        } else {
            report(&e.to_string());
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
        match msg {
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
                0
            }
            winui_host::DIALOG_CREATED => {
                // Share the settings icons; do not replace IconSet ownership.
                if let Some(settings) = winui_host::window() {
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
                if with_state(|s| s.capturing).is_some() {
                    let vk = w as u16;
                    if vk == VK_ESCAPE {
                        cancel_capture(hwnd);
                    } else if let Some(key) = read_capture(vk, l) {
                        finish_capture(hwnd, key);
                    }
                }
                0
            }
            winui_host::DEACTIVATED => {
                cancel_capture(hwnd);
                0
            }
            WM_SETTINGCHANGE => {
                if icons::theme_setting(wide_from_lparam(l).as_deref())
                    && with_state(|s| s.tray_available)
                    && tray(hwnd, NIM_MODIFY, true).is_err()
                {
                    mark_tray_unavailable(hwnd);
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_POWERBROADCAST
                if w == PBT_APMRESUMEAUTOMATIC as usize || w == PBT_APMRESUMESUSPEND as usize =>
            {
                if let Err(e) = reconcile(hwnd) {
                    fail_closed(hwnd, e);
                }
                1
            }
            WM_HOTKEY if w as i32 == top::HOTKEY_PIN => {
                match top::toggle_hotkey(hwnd) {
                    Ok(_) => update_view(hwnd, true),
                    Err(e) => report(&e.to_string()),
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
                    report(&error.to_string());
                }
                update_view(hwnd, true);
                0
            }
            WM_COMMAND if (w >> 16) == 0 => on_command(hwnd, w & 0xffff),
            WM_COPYDATA => {
                resize::accept_lparam(l);
                PostMessageW(hwnd, resize::SHOW, 0, 0);
                1
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
            WM_TIMER if w == UPDATE_TIMER => {
                with_state_mut(|s| {
                    s.updates.poll();
                    s.updates.check(false);
                });
                top::remember_foreground(hwnd);
                if top::reconcile() {
                    update_view(hwnd, true);
                }
                update_status(hwnd);
                0
            }
            WM_TIMER if w == TIMER => {
                if top::reconcile() {
                    update_view(hwnd, true);
                }
                let mode = with_state(|s| s.mode);
                if mode.expired(GetTickCount64()) {
                    if let Err(e) = reconcile(hwnd) {
                        fail_closed(hwnd, e);
                    }
                } else {
                    update_view(hwnd, false);
                }
                0
            }
            WM_TIMER if w == SMOKE => {
                // Two stages prove timer expiry, then the exact user Exit command releases an active request.
                if with_state(|s| s.smoke_stage) == 0 {
                    if with_state(|s| s.mode != Mode::Off) {
                        fail_closed(hwnd, io::Error::other("Süreli oturum sona ermedi"));
                    } else if let Err(e) = change(hwnd, Mode::Indefinite, false) {
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
                    with_state_mut(|s| s.update_on_exit = true);
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_ENDSESSION if w != 0 => {
                let _ = power(Mode::Off, false);
                top::shutdown();
                ruler::shutdown();
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, TIMER);
                KillTimer(hwnd, SMOKE);
                KillTimer(hwnd, UPDATE_TIMER);
                if let Err(e) = power(Mode::Off, false) {
                    with_state_mut(|s| s.fault = Some(e.to_string()));
                }
                top::shutdown();
                ruler::shutdown();
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
        if already_running {
            if let Some(files) = resize::command_files() {
                return resize::forward(&files);
            }
            let existing = FindWindowW(wide("PowerKit.Window").as_ptr(), null());
            if !existing.is_null() {
                PostMessageW(existing, WM_COMMAND, OPEN, 0);
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
            match load_prefs() {
                Ok(prefs) => (prefs, None),
                Err(_) => (
                    crate::settings::Preferences::default(),
                    Some("Ayar okunamadı; ekran tercihi kapalı başlatıldı. Mevcut dosya korunmuştur. Tercihi değiştirerek yeniden kaydedebilirsiniz.".into()),
                ),
            }
        };
        let display = prefs.display;
        let sound = prefs.sound;
        let show_tray = prefs.show_tray;
        let run_at_startup = prefs.startup;
        top::apply_prefs(&prefs);
        ruler::apply_prefs(&prefs);
        with_state_mut(|s| {
            s.prefs = prefs;
            s.display = display;
            s.sound = sound;
            s.smoke = smoke;
            s.warning = warning;
            s.taskbar_message = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        });
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
        // Self-test must not add an icon to, or change preferences in, the user's desktop.
        if !smoke {
            top::listen(hwnd);
            ruler::listen(hwnd);
            top::set_sound(sound);
            resize::register_context_menu();
        }
        if !smoke && let Err(error) = startup::set_enabled(run_at_startup) {
            with_state_mut(|s| {
                s.warning = Some(format!(
                    "Başlangıç kaydı uygulanamadı: {error}. Tercih dosyası korundu."
                ));
            });
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
                s.updates.set_background(s.prefs.auto_update);
                s.updates.check(false);
            });
            update_status(hwnd);
            if SetTimer(hwnd, UPDATE_TIMER, 1000, None) == 0 {
                DestroyWindow(hwnd);
                return Err(io::Error::last_os_error());
            }
        }
        if smoke {
            pump(hwnd)?;
        } else {
            let resize_files = resize::command_files();
            let host = winui_host::install(hwnd, resize_files.is_some());
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
                // WinUI did not start (for example the runtime is missing) or its
                // window went away; PowerKit keeps running from the tray.
                let warning = match settings {
                    Ok(()) => ui_strings().settings_unavailable.to_string(),
                    Err(error) => {
                        format!("{} ({error})", ui_strings().settings_unavailable)
                    }
                };
                with_state_mut(|s| {
                    s.settings_unavailable = true;
                    s.warning = Some(warning);
                });
                if let Some(seed) = resize_dialog::take_pending() {
                    resize::open_win32(hwnd, seed.files, seed.settings);
                }
                if !with_state(|s| s.tray_available) {
                    DestroyWindow(hwnd);
                }
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
        if !smoke {
            // WM_DESTROY cleared the power request on this thread. The helper waits for exit.
            let error = with_state_mut(|s| {
                s.updates.poll();
                if s.update_on_exit {
                    if s.prefs.show_notes_after_update && s.updates.ready() {
                        open_shell_uri("https://github.com/Pirimera/PowerKit/releases");
                    }
                    let restart = s.restart_for_update;
                    s.updates.apply_on_exit(restart).err()
                } else {
                    None
                }
            });
            if let Some(error) = error {
                report(&error);
            }
        }
        Ok(())
    }
}
/// Plain Win32 loop for the smoke test and for tray-only mode without WinUI.
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
fn update_status(hwnd: HWND) {
    let (changed, notify_now, balloon) = with_state_mut(|s| {
        let key = s.updates.status_key();
        let ready = s.updates.ready();
        let lang = crate::locale::UiLanguage::resolve(s.prefs.language);
        let notify_now = ready && !s.last_update_ready && s.prefs.update_notify;
        let changed = s.last_update_key != key || s.last_update_ready != ready;
        s.last_update_key = key;
        s.last_update_ready = ready;
        (changed, notify_now, lang.strings().update_balloon)
    });
    if notify_now {
        tray_balloon(hwnd, "PowerKit", balloon);
    }
    if changed {
        update_view(hwnd, true);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_mode_maps_duration_commands_without_wrapping() {
        assert_eq!(command_mode(OFF, 1_000), Some(Mode::Off));
        assert_eq!(command_mode(FOREVER, 1_000), Some(Mode::Indefinite));
        assert_eq!(command_mode(HALF_HOUR, 0), Some(Mode::Until(30 * 60_000)));
        assert_eq!(
            command_mode(HOUR, 1_000),
            Some(Mode::Until(1_000 + 60 * 60_000))
        );
        assert_eq!(command_mode(TWO_HOURS, 0), Some(Mode::Until(120 * 60_000)));
        assert_eq!(
            command_mode(HALF_HOUR, u64::MAX - 1),
            Some(Mode::Until(u64::MAX))
        );
        assert_eq!(command_mode(DISPLAY, 0), None);
        assert_eq!(command_mode(UPDATE, 0), None);
        assert_eq!(command_mode(OPEN, 0), None);
        assert_eq!(command_mode(PIN, 0), None);
        assert_eq!(command_mode(SOUND, 0), None);
        assert_eq!(command_mode(OPACITY_UP, 0), None);
        assert_eq!(command_mode(OPACITY_DOWN, 0), None);
    }
}
