//! Native Win32 boundary. All UI and power calls run on the message-loop thread.
mod frame;
mod icons;
mod menu;
mod power;
mod resize;
mod resize_io;
mod resize_wic;
mod ruler;
mod smoke;
mod theme;
mod top;
mod window;
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
    Graphics::Gdi::*,
    System::{LibraryLoader::*, Power::*, SystemInformation::*, Threading::*},
    UI::{
        Controls::DRAWITEMSTRUCT,
        HiDpi::*,
        Input::KeyboardAndMouse::{
            GetFocus, GetKeyState, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        },
        Shell::*,
        WindowsAndMessaging::*,
    },
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
const STATUS: usize = 100;
const HINT: usize = 101;
const UPDATE_STATUS: usize = 102;
const LABEL_FOREVER: &str = "Süresiz açık tut";
const LABEL_HALF_HOUR: &str = "30 dakika açık tut";
const LABEL_HOUR: &str = "1 saat açık tut";
const LABEL_TWO_HOURS: &str = "2 saat açık tut";
const LABEL_DISPLAY: &str = "Ekranı da açık tut";
const LABEL_EXIT: &str = "PowerKit'ten çık";
const LABEL_UPDATE_CHECK: &str = "Güncellemeleri denetle";
const LABEL_UPDATE_MENU: &str = "Güncellemeler";
const LABEL_OPEN: &str = "PowerKit penceresini aç";
const LABEL_STATUS_OFF: &str = "PowerKit — AlwaysOn kapalı";
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
#[derive(Default)]
struct State {
    mode: Mode,
    display: bool,
    taskbar_message: u32,
    tray_available: bool,
    smoke: bool,
    fault: Option<String>,
    warning: Option<String>,
    chrome: window::Chrome,
    last_caption: String,
    smoke_stage: u8,
    updates: crate::updates::Updates,
    update_on_exit: bool,
    restart_for_update: bool,
    last_update_status: &'static str,
    icons: IconSet,
    timed_choice: Option<usize>,
    sound: bool,
    laying_out: bool,
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
        s.chrome.capturing = None;
    });
    let prefs = with_state(|s| s.prefs.clone());
    top::apply_prefs(&prefs);
    ruler::apply_prefs(&prefs);
    update_view(hwnd, true);
    Ok(())
}
fn begin_capture(hwnd: HWND, id: usize) {
    with_state_mut(|s| s.chrome.capturing = Some(id));
    top::pause_hotkeys();
    ruler::pause_hotkeys();
    update_view(hwnd, true);
}
fn cancel_capture(hwnd: HWND) {
    if with_state(|s| s.chrome.capturing).is_none() {
        return;
    }
    with_state_mut(|s| s.chrome.capturing = None);
    let prefs = with_state(|s| s.prefs.clone());
    top::apply_prefs(&prefs);
    ruler::apply_prefs(&prefs);
    update_view(hwnd, true);
}
fn finish_capture(hwnd: HWND, key: crate::always_top::Hotkey) {
    let Some(id) = with_state(|s| s.chrome.capturing) else {
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
fn read_capture(vk: u16) -> Option<crate::always_top::Hotkey> {
    let down = |key: u16| unsafe { GetKeyState(key as i32) } < 0;
    crate::always_top::Hotkey::new(
        down(VK_LWIN) || down(VK_RWIN),
        down(VK_CONTROL),
        down(VK_MENU),
        down(VK_SHIFT),
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
    let always_on = with_state(|s| match s.mode {
        Mode::Off => LABEL_STATUS_OFF.into(),
        Mode::Indefinite => "PowerKit — AlwaysOn süresiz açık".into(),
        Mode::Until(end) => format!(
            "PowerKit — AlwaysOn: {} dk kaldı",
            end.saturating_sub(unsafe { GetTickCount64() })
                .div_ceil(60000)
        ),
    });
    let mut text = match top::session().title() {
        Some(title) => format!("{always_on} · AlwaysTop: {title}"),
        None => always_on,
    };
    if ruler::active() {
        text.push_str(" · ScreenRuler açık");
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
    STATE.with(|state| {
        window::select_page(&mut state.borrow_mut().chrome, window::Page::ImageResizer)
    });
    present(hwnd);
    if let Err(error) = layout(hwnd, unsafe { GetDpiForWindow(hwnd) }) {
        fail_closed(hwnd, error);
        return;
    }
    update_view(hwnd, true);
}
fn save_resize_filename(hwnd: HWND) {
    let text = window::control_string(unsafe { GetDlgItem(hwnd, window::RESIZE_FILENAME as i32) });
    let Some(filename) = crate::image_resizer::ResizeSettings::parse_filename(&text) else {
        update_view(hwnd, true);
        return;
    };
    if let Err(error) = step_ruler(hwnd, |prefs| prefs.resize.filename = filename) {
        report(&error.to_string());
    }
}
fn present(hwnd: HWND) {
    // SAFETY: only our live top-level window is passed here.
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
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
fn update_view(hwnd: HWND, force: bool) {
    let caption = tooltip();
    let (changed, display, available, warning) = with_state_mut(|s| {
        let changed = force || s.last_caption != caption;
        s.last_caption = caption.clone();
        s.chrome.selected_mode = window::mode_button(s.mode, s.timed_choice);
        s.chrome.frame_preview = s.prefs.frame_color.unwrap_or_else(frame::accent_color);
        (changed, s.display, s.tray_available, s.warning.clone())
    });
    // SAFETY: controls belong to this window; synchronous UTF-16 buffers remain alive.
    unsafe {
        if changed {
            SetWindowTextW(GetDlgItem(hwnd, STATUS as i32), wide(&caption).as_ptr());
            let top_status = match top::session().title() {
                Some(title) => format!("AlwaysTop açık ({title})"),
                None => "AlwaysTop kapalı".into(),
            };
            SetWindowTextW(
                GetDlgItem(hwnd, window::TOP_STATUS as i32),
                wide(&top_status).as_ptr(),
            );
            window::set_check(GetDlgItem(hwnd, SOUND as i32), with_state(|s| s.sound));
            let prefs = with_state(|s| s.prefs.clone());
            let capturing = with_state(|s| s.chrome.capturing);
            let pin = match capturing {
                Some(window::HOTKEY_PIN_EDIT) => "Yeni kısayola basın…".into(),
                _ => format!("Etkinleştirme kısayolu: {}", prefs.pin.label()),
            };
            let up = match capturing {
                Some(window::HOTKEY_OP_UP_EDIT) => "Yeni kısayola basın…".into(),
                _ => format!("Saydamlığı artır: {}", prefs.opacity_up.label()),
            };
            let down = match capturing {
                Some(window::HOTKEY_OP_DOWN_EDIT) => "Yeni kısayola basın…".into(),
                _ => format!("Saydamlığı azalt: {}", prefs.opacity_down.label()),
            };
            SetWindowTextW(
                GetDlgItem(hwnd, window::HOTKEY_PIN_EDIT as i32),
                wide(&pin).as_ptr(),
            );
            SetWindowTextW(
                GetDlgItem(hwnd, window::HOTKEY_OP_UP_EDIT as i32),
                wide(&up).as_ptr(),
            );
            SetWindowTextW(
                GetDlgItem(hwnd, window::HOTKEY_OP_DOWN_EDIT as i32),
                wide(&down).as_ptr(),
            );
            window::set_check(
                GetDlgItem(hwnd, window::FRAME_ACCENT as i32),
                prefs.frame_color.is_none(),
            );
            SetWindowTextW(
                GetDlgItem(hwnd, window::FRAME_OPACITY as i32),
                wide(&format!("Kenarlık opaklığı: %{}", prefs.frame_opacity)).as_ptr(),
            );
            SetWindowTextW(
                GetDlgItem(hwnd, window::FRAME_THICKNESS as i32),
                wide(&format!("Kenarlık kalınlığı: {} px", prefs.frame_thickness)).as_ptr(),
            );
            let ruler_key = match capturing {
                Some(window::RULER_HOTKEY) => "Yeni kısayola basın…".into(),
                _ => format!("Etkinleştirme kısayolu: {}", prefs.ruler_hotkey.label()),
            };
            SetWindowTextW(
                GetDlgItem(hwnd, window::RULER_HOTKEY as i32),
                wide(&ruler_key).as_ptr(),
            );
            let measuring = if ruler::active() {
                "Ölçümü kapat"
            } else {
                "Ölçümü başlat"
            };
            SetWindowTextW(GetDlgItem(hwnd, MEASURE as i32), wide(measuring).as_ptr());
            SetWindowTextW(
                GetDlgItem(hwnd, window::RULER_STATUS as i32),
                wide(if ruler::active() {
                    "ScreenRuler açık"
                } else {
                    "ScreenRuler kapalı"
                })
                .as_ptr(),
            );
            SetWindowTextW(
                GetDlgItem(hwnd, window::RULER_TOLERANCE as i32),
                wide(&format!("Piksel toleransı: {}", prefs.ruler_tolerance)).as_ptr(),
            );
            window::set_check(
                GetDlgItem(hwnd, window::RULER_CONTINUOUS as i32),
                prefs.ruler_continuous,
            );
            window::set_check(
                GetDlgItem(hwnd, window::RULER_CHANNEL as i32),
                prefs.ruler_per_channel,
            );
            window::set_check(
                GetDlgItem(hwnd, window::RULER_FEET as i32),
                prefs.ruler_feet,
            );
            with_state_mut(|s| {
                s.chrome.ruler_mode = window::mode_id(prefs.ruler_mode);
                s.chrome.ruler_unit = window::unit_id(prefs.ruler_unit);
                s.chrome.ruler_preview = prefs.ruler_color;
                s.chrome.resize_encoder = window::encoder_button(prefs.resize.fallback);
                s.chrome.resize_png = window::png_button(prefs.resize.png);
            });
            window::set_check(
                GetDlgItem(hwnd, window::RESIZE_SHRINK as i32),
                prefs.resize.shrink_only,
            );
            window::set_check(
                GetDlgItem(hwnd, window::RESIZE_REPLACE as i32),
                prefs.resize.replace,
            );
            window::set_check(
                GetDlgItem(hwnd, window::RESIZE_ORIENTATION as i32),
                prefs.resize.ignore_orientation,
            );
            window::set_check(
                GetDlgItem(hwnd, window::RESIZE_METADATA as i32),
                prefs.resize.remove_metadata,
            );
            window::set_check(
                GetDlgItem(hwnd, window::RESIZE_KEEP_DATE as i32),
                prefs.resize.keep_date,
            );
            SetWindowTextW(
                GetDlgItem(hwnd, window::RESIZE_JPEG as i32),
                wide(&format!("JPEG kalitesi: {}", prefs.resize.jpeg_quality)).as_ptr(),
            );
            SetWindowTextW(
                GetDlgItem(hwnd, window::RESIZE_TIFF as i32),
                wide(&format!("TIFF sıkıştırma: {}", prefs.resize.tiff.label())).as_ptr(),
            );
            let filename = GetDlgItem(hwnd, window::RESIZE_FILENAME as i32);
            if GetFocus() != filename {
                SetWindowTextW(filename, wide(&prefs.resize.filename).as_ptr());
            }
        }
        window::set_check(GetDlgItem(hwnd, DISPLAY as i32), display);
        if changed && available && tray(hwnd, NIM_MODIFY, force).is_err() {
            mark_tray_unavailable(hwnd);
        }
        let available = with_state(|s| s.tray_available);
        let hint = if let Some(warning) = warning {
            warning
        } else if available {
            "X pencereyi gizler; AlwaysOn çalışmaya devam eder. Çıkmak için PowerKit’ten çık düğmesini kullanın.".into()
        } else {
            "Bildirim alanına erişilemiyor. Bu pencereyi kapatmak AlwaysOn'u durdurur ve uygulamadan çıkar.".into()
        };
        if force || changed {
            SetWindowTextW(GetDlgItem(hwnd, HINT as i32), wide(&hint).as_ptr());
            for id in window::MODE_ORDER {
                InvalidateRect(GetDlgItem(hwnd, id as i32), null(), 1);
            }
            for id in window::RULER_MODE_ORDER
                .iter()
                .chain(window::RULER_UNIT_ORDER.iter())
                .chain(window::RESIZE_ENCODER_ORDER.iter())
                .chain(window::RESIZE_PNG_ORDER.iter())
            {
                InvalidateRect(GetDlgItem(hwnd, *id as i32), null(), 1);
            }
            InvalidateRect(GetDlgItem(hwnd, window::FRAME_COLOR as i32), null(), 1);
            InvalidateRect(GetDlgItem(hwnd, window::RULER_COLOR as i32), null(), 1);
        }
    }
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
fn apply_theme(hwnd: HWND) {
    let next = STATE.with(|state| window::refresh_theme(&mut state.borrow_mut().chrome));
    window::apply_theme_surfaces(hwnd, next);
}
fn layout(hwnd: HWND, dpi: u32) -> io::Result<()> {
    let busy = STATE.with(|state| state.try_borrow().map(|s| s.laying_out).unwrap_or(true));
    if busy {
        return Ok(());
    }
    with_state_mut(|s| s.laying_out = true);
    let result = (|| {
        let (page, fonts) =
            STATE.with(|state| window::prepare_fonts(dpi, &mut state.borrow_mut().chrome))?;
        window::apply_layout(hwnd, dpi, page, &fonts);
        unsafe {
            InvalidateRect(hwnd, null(), 1);
        }
        Ok(())
    })();
    with_state_mut(|s| s.laying_out = false);
    result
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
        ),
    ) {
        unsafe {
            PostMessageW(hwnd, WM_COMMAND, command, 0);
        }
    }
}
fn on_command(hwnd: HWND, id: usize) -> LRESULT {
    if let Some(page) = window::page_from_nav(id) {
        STATE.with(|state| window::select_page(&mut state.borrow_mut().chrome, page));
        if let Err(e) = layout(hwnd, unsafe { GetDpiForWindow(hwnd) }) {
            fail_closed(hwnd, e);
        } else {
            update_view(hwnd, true);
        }
        return 0;
    }
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
        OPACITY_UP => {
            let _ = top::step_pinned_opacity(crate::always_top::OPACITY_STEP);
            Ok(())
        }
        OPACITY_DOWN => {
            let _ = top::step_pinned_opacity(-crate::always_top::OPACITY_STEP);
            Ok(())
        }
        MEASURE => ruler::toggle(hwnd),
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
            let current = with_state(|s| s.chrome.frame_preview);
            if let Some(color) = window::pick_color(hwnd, current) {
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
            if let Some(color) = window::pick_color(hwnd, current) {
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
                let rect = &*(l as *const RECT);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let previous = with_state(|s| s.icons);
                let icons = icons::apply_window(hwnd, previous);
                with_state_mut(|s| s.icons = icons);
                if with_state(|s| s.tray_available) && tray(hwnd, NIM_MODIFY, true).is_err() {
                    mark_tray_unavailable(hwnd);
                }
                if let Err(e) = layout(hwnd, (w & 0xffff) as u32) {
                    fail_closed(hwnd, e);
                }
                0
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if with_state(|s| s.chrome.capturing).is_some() {
                    let vk = w as u16;
                    if vk == VK_ESCAPE {
                        cancel_capture(hwnd);
                    } else if let Some(key) = read_capture(vk) {
                        finish_capture(hwnd, key);
                    }
                    0
                } else {
                    DefWindowProcW(hwnd, msg, w, l)
                }
            }
            WM_ACTIVATE => {
                if w as u32 == WA_INACTIVE {
                    cancel_capture(hwnd);
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_SETTINGCHANGE => {
                if icons::theme_setting(wide_from_lparam(l).as_deref()) {
                    apply_theme(hwnd);
                    InvalidateRect(hwnd, null(), 1);
                    if with_state(|s| s.tray_available) && tray(hwnd, NIM_MODIFY, true).is_err() {
                        mark_tray_unavailable(hwnd);
                    }
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_ERASEBKGND => {
                let painted = STATE.with(|state| {
                    state
                        .try_borrow()
                        .ok()
                        .is_some_and(|s| window::erase(hwnd, w as HDC, &s.chrome))
                });
                i32::from(painted) as LRESULT
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT => STATE
                .with(|state| {
                    state
                        .try_borrow()
                        .ok()
                        .and_then(|s| window::colorize(hwnd, msg, w as HDC, l as HWND, &s.chrome))
                })
                .unwrap_or_else(|| DefWindowProcW(hwnd, msg, w, l)),
            WM_DRAWITEM => {
                let drawn = STATE.with(|state| {
                    state.try_borrow().ok().is_some_and(|s| {
                        window::draw_item(hwnd, &*(l as *const DRAWITEMSTRUCT), &s.chrome)
                    })
                });
                i32::from(drawn) as LRESULT
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(l as *mut MINMAXINFO);
                let dpi = GetDpiForWindow(hwnd).max(96);
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: window::MIN_WIDTH * dpi as i32 / 96,
                    bottom: window::MIN_HEIGHT * dpi as i32 / 96,
                };
                let style =
                    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_THICKFRAME;
                AdjustWindowRectExForDpi(&mut rect, style, 0, 0, dpi);
                info.ptMinTrackSize.x = rect.right - rect.left;
                info.ptMinTrackSize.y = rect.bottom - rect.top;
                0
            }
            WM_SIZE => {
                if w != SIZE_MINIMIZED as usize
                    && !GetDlgItem(hwnd, window::NAV_ALWAYS_ON as i32).is_null()
                    && let Err(e) = layout(hwnd, GetDpiForWindow(hwnd))
                {
                    fail_closed(hwnd, e);
                }
                0
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
            WM_COMMAND if (w >> 16) == 0x0200 && (w & 0xffff) == window::RESIZE_FILENAME => {
                save_resize_filename(hwnd);
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
            WM_CLOSE => {
                if with_state(|s| s.tray_available) {
                    ShowWindow(hwnd, SW_HIDE);
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
                with_state_mut(|s| s.chrome.release());
                PostQuitMessage(0);
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
        let window_style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_THICKFRAME;
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("PowerKit").as_ptr(),
            window_style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            600,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        let previous = with_state(|s| s.icons);
        let window_icons = icons::apply_window(hwnd, previous);
        with_state_mut(|s| s.icons = window_icons);
        theme::apply_window_chrome(hwnd);
        if let Err(e) = window::create_controls(hwnd, instance, display, sound) {
            DestroyWindow(hwnd);
            return Err(e);
        }
        apply_theme(hwnd);
        let dpi = GetDpiForWindow(hwnd);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: window::CLIENT_WIDTH * dpi as i32 / 96,
            bottom: window::CLIENT_HEIGHT * dpi as i32 / 96,
        };
        if AdjustWindowRectExForDpi(&mut rect, window_style, 0, 0, dpi) == 0 {
            DestroyWindow(hwnd);
            return Err(io::Error::last_os_error());
        }
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        if let Err(e) = layout(hwnd, dpi) {
            DestroyWindow(hwnd);
            return Err(e);
        }
        // Self-test must not add an icon to, or change preferences in, the user's desktop.
        if !smoke {
            top::listen(hwnd);
            ruler::listen(hwnd);
            top::set_sound(sound);
            resize::register_context_menu();
        }
        let tray_available = !smoke && tray(hwnd, NIM_ADD, true).is_ok();
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
            with_state_mut(|s| s.updates.check(false));
            update_status(hwnd);
            if SetTimer(hwnd, UPDATE_TIMER, 1000, None) == 0 {
                DestroyWindow(hwnd);
                return Err(io::Error::last_os_error());
            }
            if let Some(files) = resize::command_files() {
                resize::open(hwnd, files, current_resize());
            } else {
                ShowWindow(hwnd, SW_SHOW);
            }
        }
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
                break;
            }
            let dialog_handled =
                resize::live().is_some_and(|dialog| IsDialogMessageW(dialog, &message) != 0);
            if !dialog_handled && IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
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
fn update_status(hwnd: HWND) {
    let (status, ready, changed) = with_state_mut(|s| {
        let status = s.updates.status();
        let changed = s.last_update_status != status;
        s.last_update_status = status;
        (status, s.updates.ready(), changed)
    });
    if !changed {
        return;
    }
    // SAFETY: synchronous calls to child controls owned by this live window.
    unsafe {
        SetWindowTextW(
            GetDlgItem(hwnd, UPDATE_STATUS as i32),
            wide(status).as_ptr(),
        );
        SetWindowTextW(
            GetDlgItem(hwnd, UPDATE as i32),
            wide(if ready {
                "Güncelle ve yeniden başlat"
            } else {
                LABEL_UPDATE_CHECK
            })
            .as_ptr(),
        );
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
