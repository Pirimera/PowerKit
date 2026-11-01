//! Native Win32 boundary. All UI and power calls run on the message-loop thread.
use crate::always_on::Mode;
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
    UI::{Shell::*, WindowsAndMessaging::*},
};
const CALLBACK: u32 = WM_APP + 1;
const TIMER: usize = 1;
const SMOKE: usize = 2;
const OFF: usize = 10;
const FOREVER: usize = 11;
const HALF_HOUR: usize = 12;
const HOUR: usize = 13;
const TWO_HOURS: usize = 14;
const DISPLAY: usize = 15;
const EXIT: usize = 16;
const OPEN: usize = 17;
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
#[derive(Default)]
struct State {
    mode: Mode,
    display: bool,
    taskbar_message: u32,
    tray_available: bool,
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
fn power(mode: Mode, display: bool) -> io::Result<()> {
    let mut flags = ES_CONTINUOUS;
    if mode.active() {
        flags |= ES_SYSTEM_REQUIRED;
        if display {
            flags |= ES_DISPLAY_REQUIRED;
        }
    }
    // SAFETY: documented flag combination, called only on the owning UI thread.
    if unsafe { SetThreadExecutionState(flags) } == 0 {
        Err(io::Error::other("Windows güç isteğini uygulayamadı"))
    } else {
        Ok(())
    }
}
struct PowerGuard;
impl Drop for PowerGuard {
    fn drop(&mut self) {
        let _ = power(Mode::Off, false);
    }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn tooltip() -> String {
    STATE.with(|cell| {
        let s = cell.borrow();
        match s.mode {
            Mode::Off => "PowerKit — AlwaysOn kapalı".into(),
            Mode::Indefinite => "PowerKit — AlwaysOn süresiz açık".into(),
            Mode::Until(end) => format!(
                "PowerKit — AlwaysOn: {} dk kaldı",
                end.saturating_sub(unsafe { GetTickCount64() })
                    .div_ceil(60000)
            ),
        }
    })
}
fn tray(hwnd: HWND, operation: u32) -> io::Result<()> {
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = CALLBACK;
    data.hIcon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
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
        let was_timed = STATE.with(|s| matches!(s.borrow().mode, Mode::Until(_)));
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
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.mode = mode;
        s.display = display;
    });
    unsafe {
        SetWindowTextW(GetDlgItem(hwnd, 100), wide(&tooltip()).as_ptr());
        SendMessageW(
            GetDlgItem(hwnd, DISPLAY as i32),
            BM_SETCHECK,
            usize::from(display),
            0,
        );
    }
    if STATE.with(|s| s.borrow().tray_available) && tray(hwnd, NIM_MODIFY).is_err() {
        STATE.with(|s| s.borrow_mut().tray_available = false);
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
        }
    }
    Ok(())
}
fn menu(hwnd: HWND) {
    let (mode, display) = STATE.with(|s| {
        let s = s.borrow();
        (s.mode, s.display)
    });
    // SAFETY: menu lives until TrackPopupMenu returns; command processing follows destruction.
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            report("Menü oluşturulamadı");
            return;
        }
        AppendMenuW(menu, MF_STRING | MF_DISABLED, 0, wide(&tooltip()).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        for (id, label, checked) in [
            (OFF, "AlwaysOn: Kapalı", mode == Mode::Off),
            (FOREVER, "Süresiz açık tut", mode == Mode::Indefinite),
            (HALF_HOUR, "30 dakika açık tut", false),
            (HOUR, "1 saat açık tut", false),
            (TWO_HOURS, "2 saat açık tut", false),
            (DISPLAY, "Ekranı da açık tut", display),
        ] {
            AppendMenuW(
                menu,
                MF_STRING | if checked { MF_CHECKED } else { 0 },
                id,
                wide(label).as_ptr(),
            );
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(
            menu,
            MF_STRING,
            OPEN,
            wide("PowerKit penceresini aç").as_ptr(),
        );
        AppendMenuW(menu, MF_STRING, EXIT, wide("PowerKit'ten çık").as_ptr());
        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        ) as usize;
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        if command != 0 {
            PostMessageW(hwnd, WM_COMMAND, command, 0);
        }
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: Windows invokes this procedure with a live window on its UI thread.
    unsafe {
        let taskbar = STATE.with(|s| s.borrow().taskbar_message);
        if taskbar != 0 && msg == taskbar {
            let available = tray(hwnd, NIM_ADD).is_ok();
            STATE.with(|s| s.borrow_mut().tray_available = available);
            if !available {
                ShowWindow(hwnd, SW_SHOW);
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
            WM_COMMAND => {
                let (mode, display) = STATE.with(|s| {
                    let s = s.borrow();
                    (s.mode, s.display)
                });
                let result = match w & 0xffff {
                    OPEN => {
                        ShowWindow(hwnd, SW_SHOW);
                        SetForegroundWindow(hwnd);
                        return 0;
                    }
                    EXIT => {
                        DestroyWindow(hwnd);
                        return 0;
                    }
                    OFF => change(hwnd, Mode::Off, display),
                    FOREVER => change(hwnd, Mode::Indefinite, display),
                    id @ (HALF_HOUR | HOUR | TWO_HOURS) => {
                        let minutes = match id {
                            HALF_HOUR => 30,
                            HOUR => 60,
                            _ => 120,
                        };
                        change(
                            hwnd,
                            Mode::timed(GetTickCount64(), minutes * 60000),
                            display,
                        )
                    }
                    DISPLAY => match save_display(!display) {
                        Ok(()) => {
                            let result = change(hwnd, mode, !display);
                            if result.is_err() {
                                let _ = save_display(display);
                            }
                            result
                        }
                        Err(e) => Err(e),
                    },
                    _ => Ok(()),
                };
                if let Err(e) = result {
                    report(&e.to_string());
                }
                0
            }
            WM_TIMER if w == TIMER => {
                let (mode, display) = STATE.with(|s| {
                    let s = s.borrow();
                    (s.mode, s.display)
                });
                if mode.expired(GetTickCount64()) {
                    if let Err(e) = change(hwnd, Mode::Off, display) {
                        report(&e.to_string());
                        DestroyWindow(hwnd);
                    }
                } else {
                    SetWindowTextW(GetDlgItem(hwnd, 100), wide(&tooltip()).as_ptr());
                    if STATE.with(|s| s.borrow().tray_available) {
                        let _ = tray(hwnd, NIM_MODIFY);
                    }
                }
                0
            }
            WM_TIMER if w == SMOKE => {
                let expired = STATE.with(|s| s.borrow().mode == Mode::Off);
                DestroyWindow(hwnd);
                PostQuitMessage(if expired { 0 } else { 2 });
                0
            }
            WM_CLOSE => {
                if STATE.with(|s| s.borrow().tray_available) {
                    ShowWindow(hwnd, SW_HIDE);
                } else {
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
                let _ = power(Mode::Off, false);
                let _ = tray(hwnd, NIM_DELETE);
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
        let mutex = CreateMutexW(null(), 0, wide("Local\\PowerKit.Singleton").as_ptr());
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
        let _power = PowerGuard;
        let display = load_display()?;
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.display = display;
            s.taskbar_message = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        });
        let instance = GetModuleHandleW(null());
        let class_name = wide("PowerKit.Window");
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return Err(io::Error::last_os_error());
        }
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("PowerKit — AlwaysOn").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            440,
            410,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        for (row, (index, label)) in [
            (100, "PowerKit — AlwaysOn kapalı"),
            (FOREVER, "Süresiz açık tut"),
            (HALF_HOUR, "30 dakika açık tut"),
            (HOUR, "1 saat açık tut"),
            (TWO_HOURS, "2 saat açık tut"),
            (DISPLAY, "Ekranı da açık tut"),
            (OFF, "AlwaysOn durdur"),
            (EXIT, "PowerKit'ten çık"),
        ]
        .iter()
        .enumerate()
        {
            let control = CreateWindowExW(
                0,
                wide(if *index == 100 { "STATIC" } else { "BUTTON" }).as_ptr(),
                wide(label).as_ptr(),
                WS_CHILD
                    | WS_VISIBLE
                    | if *index == 100 { 0 } else { WS_TABSTOP }
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
            SendMessageW(
                control,
                WM_SETFONT,
                windows_sys::Win32::Graphics::Gdi::GetStockObject(
                    windows_sys::Win32::Graphics::Gdi::DEFAULT_GUI_FONT,
                ) as usize,
                1,
            );
            if *index == DISPLAY {
                SendMessageW(control, BM_SETCHECK, usize::from(display), 0);
            }
        }
        let tray_available = tray(hwnd, NIM_ADD).is_ok();
        STATE.with(|s| s.borrow_mut().tray_available = tray_available);
        if std::env::args().any(|a| a == "--smoke-test") {
            change(hwnd, Mode::Indefinite, false)?;
            change(hwnd, Mode::Indefinite, true)?;
            change(hwnd, Mode::Off, false)?;
            change(hwnd, Mode::timed(GetTickCount64(), 1000), false)?;
            if SetTimer(hwnd, SMOKE, 2500, None) == 0 {
                DestroyWindow(hwnd);
                return Err(io::Error::last_os_error());
            }
        }
        if !std::env::args().any(|a| a == "--smoke-test") {
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_power_flags_toggle_and_release_on_owning_thread() {
        // Native ARM64 integration: Windows returns the previous flags for this thread.
        let _guard = PowerGuard;
        power(Mode::Off, false).unwrap();
        power(Mode::Indefinite, false).unwrap();
        let previous = unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
        };
        assert_ne!(previous & ES_SYSTEM_REQUIRED, 0);
        assert_eq!(previous & ES_DISPLAY_REQUIRED, 0);
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        assert_ne!(previous & ES_SYSTEM_REQUIRED, 0);
        assert_ne!(previous & ES_DISPLAY_REQUIRED, 0);
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        assert_eq!(previous & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED), 0);
    }
    #[test]
    fn guard_clears_request_on_scope_exit() {
        {
            let _guard = PowerGuard;
            power(Mode::Indefinite, false).unwrap();
        }
        let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        assert_eq!(previous & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED), 0);
    }
}
