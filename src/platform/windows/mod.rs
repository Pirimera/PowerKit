//! Native Win32 boundary. All UI and power calls run on the message-loop thread.
mod icons;
mod menu;
mod power;
mod smoke;
mod theme;
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
    UI::{HiDpi::*, Shell::*, WindowsAndMessaging::*},
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
const STATUS: usize = 100;
const HINT: usize = 101;
const UPDATE_STATUS: usize = 102;
const LABEL_FOREVER: &str = "Süresiz açık tut";
const LABEL_HALF_HOUR: &str = "30 dakika açık tut";
const LABEL_HOUR: &str = "1 saat açık tut";
const LABEL_TWO_HOURS: &str = "2 saat açık tut";
const LABEL_DISPLAY: &str = "Ekranı da açık tut";
const LABEL_OFF_WINDOW: &str = "AlwaysOn durdur";
const LABEL_EXIT: &str = "PowerKit'ten çık";
const LABEL_UPDATE_CHECK: &str = "Güncellemeleri denetle";
const LABEL_UPDATE_MENU: &str = "Güncellemeler";
const LABEL_OPEN: &str = "PowerKit penceresini aç";
const LABEL_STATUS_OFF: &str = "PowerKit — AlwaysOn kapalı";
const CONTROLS: &[(usize, &str)] = &[
    (STATUS, LABEL_STATUS_OFF),
    (FOREVER, LABEL_FOREVER),
    (HALF_HOUR, LABEL_HALF_HOUR),
    (HOUR, LABEL_HOUR),
    (TWO_HOURS, LABEL_TWO_HOURS),
    (DISPLAY, LABEL_DISPLAY),
    (OFF, LABEL_OFF_WINDOW),
    (EXIT, LABEL_EXIT),
    (UPDATE, LABEL_UPDATE_CHECK),
    (UPDATE_STATUS, ""),
    (HINT, ""),
];
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
    font: HFONT,
    last_caption: String,
    smoke_stage: u8,
    updates: crate::updates::Updates,
    update_on_exit: bool,
    restart_for_update: bool,
    last_update_status: &'static str,
    icons: IconSet,
    timed_choice: Option<usize>,
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
fn load_display() -> io::Result<bool> {
    crate::settings::load(&settings_path()?)
}
fn save_display(value: bool) -> io::Result<()> {
    crate::settings::save(&settings_path()?, value)
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
    with_state(|s| match s.mode {
        Mode::Off => LABEL_STATUS_OFF.into(),
        Mode::Indefinite => "PowerKit — AlwaysOn süresiz açık".into(),
        Mode::Until(end) => format!(
            "PowerKit — AlwaysOn: {} dk kaldı",
            end.saturating_sub(unsafe { GetTickCount64() })
                .div_ceil(60000)
        ),
    })
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
        (changed, s.display, s.tray_available, s.warning.clone())
    });
    // SAFETY: controls belong to this window; synchronous UTF-16 buffers remain alive.
    unsafe {
        if changed {
            SetWindowTextW(GetDlgItem(hwnd, STATUS as i32), wide(&caption).as_ptr());
        }
        SendMessageW(
            GetDlgItem(hwnd, DISPLAY as i32),
            BM_SETCHECK,
            usize::from(display),
            0,
        );
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
        }
    }
}
fn fail_closed(hwnd: HWND, error: io::Error) {
    let message = error.to_string();
    with_state_mut(|s| s.fault = Some(message));
    // Release BEFORE displaying any modal error dialog.
    let _ = power(Mode::Off, false);
    unsafe {
        DestroyWindow(hwnd);
    }
    // main displays the saved error after cleanup.
}
fn reconcile(hwnd: HWND) -> io::Result<()> {
    let (mode, display) = with_state(|s| (s.mode, s.display));
    change(hwnd, mode.at(unsafe { GetTickCount64() }), display)
}
fn layout(hwnd: HWND, dpi: u32) -> io::Result<()> {
    let scale = |n: i32| n * dpi as i32 / 96;
    // SAFETY: resource lifetime is maintained in STATE and old fonts are deleted after reassignment.
    unsafe {
        let font = CreateFontW(
            -scale(16),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        if font.is_null() {
            return Err(io::Error::last_os_error());
        }
        for (row, (id, _)) in CONTROLS.iter().enumerate() {
            let control = GetDlgItem(hwnd, *id as i32);
            SetWindowPos(
                control,
                null_mut(),
                scale(16),
                scale(16 + row as i32 * 40),
                scale(408),
                scale(if *id == HINT { 64 } else { 32 }),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            SendMessageW(control, WM_SETFONT, font as usize, 1);
        }
        let old = with_state_mut(|s| std::mem::replace(&mut s.font, font));
        if !old.is_null() {
            DeleteObject(old);
        }
        InvalidateRect(hwnd, null(), 1);
    }
    Ok(())
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
    let (mode, display, timed_choice) = with_state(|s| (s.mode, s.display, s.timed_choice));
    let now = unsafe { GetTickCount64() };
    let remaining = match mode {
        Mode::Until(end) => menu::remaining_minutes(now, end),
        _ => 0,
    };
    if let Some(command) = menu::track(
        hwnd,
        &menu::tray_entries(mode, display, remaining, timed_choice),
    ) {
        unsafe {
            PostMessageW(hwnd, WM_COMMAND, command, 0);
        }
    }
}
fn on_command(hwnd: HWND, id: usize) -> LRESULT {
    let (mode, display) = with_state(|s| (s.mode, s.display));
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
        DISPLAY => match save_display(!display) {
            Ok(()) => {
                let result = change(hwnd, mode, !display);
                if result.is_err()
                    && let Err(rollback) = save_display(display)
                {
                    report(&format!(
                        "Güç isteği değişmedi; ayar geri alınamadı: {rollback}"
                    ));
                    return 0;
                }
                if result.is_ok() {
                    with_state_mut(|s| s.warning = None);
                    update_view(hwnd, true);
                }
                result
            }
            Err(e) => Err(e),
        },
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
            WM_COMMAND if (w >> 16) == 0 => on_command(hwnd, w & 0xffff),
            WM_TIMER if w == UPDATE_TIMER => {
                with_state_mut(|s| {
                    s.updates.poll();
                    s.updates.check(false);
                });
                update_status(hwnd);
                0
            }
            WM_TIMER if w == TIMER => {
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
                let _ = tray(hwnd, NIM_DELETE, false);
                let icons = with_state_mut(|s| std::mem::take(&mut s.icons));
                icons::release_all(icons);
                let font = with_state_mut(|s| std::mem::replace(&mut s.font, null_mut()));
                if !font.is_null() {
                    DeleteObject(font);
                }
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
            let existing = FindWindowW(wide("PowerKit.Window").as_ptr(), null());
            if !existing.is_null() {
                PostMessageW(existing, WM_COMMAND, OPEN, 0);
            }
            return Ok(());
        }
        let _power = PowerGuard::default();
        let (display, warning) = if smoke {
            (false, None)
        } else {
            match load_display() { Ok(v) => (v, None), Err(_) => (false, Some("Ayar okunamadı; ekran tercihi kapalı başlatıldı. Mevcut dosya korunmuştur. Tercihi değiştirerek yeniden kaydedebilirsiniz.".into())) }
        };
        with_state_mut(|s| {
            s.display = display;
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
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: class_icon,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            icons::release_owned(class_icon as isize);
            return Err(io::Error::last_os_error());
        }
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("PowerKit — AlwaysOn").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            480,
            520,
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
        for (row, (index, label)) in CONTROLS.iter().enumerate() {
            let control = CreateWindowExW(
                0,
                wide(if *index >= 100 { "STATIC" } else { "BUTTON" }).as_ptr(),
                wide(label).as_ptr(),
                WS_CHILD
                    | WS_VISIBLE
                    | if *index >= 100 { 0 } else { WS_TABSTOP }
                    | if *index == DISPLAY {
                        BS_CHECKBOX as u32
                    } else {
                        0
                    },
                16,
                16 + row as i32 * 40,
                390,
                32,
                hwnd,
                *index as _,
                instance,
                null(),
            );
            if control.is_null() {
                DestroyWindow(hwnd);
                return Err(io::Error::last_os_error());
            }
            if *index == DISPLAY {
                SendMessageW(control, BM_SETCHECK, usize::from(display), 0);
            }
        }
        let dpi = GetDpiForWindow(hwnd);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: (440 * dpi / 96) as i32,
            bottom: (504 * dpi / 96) as i32,
        };
        if AdjustWindowRectExForDpi(
            &mut rect,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            0,
            0,
            dpi,
        ) == 0
        {
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
            ShowWindow(hwnd, SW_SHOW);
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
            if IsDialogMessageW(hwnd, &message) == 0 {
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
    }
}
