//! Isolated native message-loop acceptance checks; never accesses user settings.
use super::*;

pub(super) fn run(hwnd: HWND) -> io::Result<()> {
    // SAFETY: hwnd is our live isolated window, invoked only on its owning UI thread.
    // Exercise real command handlers and resume reconciliation without user preferences.
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, FOREVER, 0);
        if STATE.with(|s| s.borrow().mode) != Mode::Indefinite {
            return Err(io::Error::other("Süresiz komutu başarısız"));
        }
        change(hwnd, Mode::Indefinite, true)?;
        let previous =
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        if previous & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
            != ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
        {
            return Err(io::Error::other("Güç bayrakları eksik"));
        }
        SendMessageW(hwnd, WM_COMMAND, OFF, 0);
        if STATE.with(|s| s.borrow().mode) != Mode::Off {
            return Err(io::Error::other("Durdurma komutu başarısız"));
        }
        change(hwnd, Mode::Until(GetTickCount64().saturating_sub(1)), false)?;
        SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, 0);
        if STATE.with(|s| s.borrow().mode) != Mode::Off {
            return Err(io::Error::other("Uyku dönüşü süresi geçmiş oturum durmadı"));
        }
        ShowWindow(hwnd, SW_MINIMIZE);
        SendMessageW(hwnd, WM_COMMAND, OPEN, 0);
        if IsIconic(hwnd) != 0 {
            return Err(io::Error::other("Küçültülmüş pencere geri getirilemedi"));
        }
        ShowWindow(hwnd, SW_HIDE);
        for dpi in [96, 144, 192] {
            layout(hwnd, dpi)?;
            let mut bounds = RECT::default();
            if GetWindowRect(GetDlgItem(hwnd, FOREVER as i32), &mut bounds) == 0
                || bounds.bottom - bounds.top != (32 * dpi / 96) as i32
            {
                return Err(io::Error::other("DPI kontrol boyutu hatalı"));
            }
        }
        layout(hwnd, GetDpiForWindow(hwnd))?;
        // There is deliberately no shell icon for this test HWND. A failed update must
        // clear the tray flag, so X cannot hide an inaccessible window.
        STATE.with(|s| s.borrow_mut().tray_available = true);
        update_view(hwnd, true);
        if STATE.with(|s| s.borrow().tray_available) {
            return Err(io::Error::other("Eksik bildirim simgesi algılanmadı"));
        }
        // A simulated Explorer restart must leave the power session unchanged.
        let taskbar = STATE.with(|s| s.borrow().taskbar_message);
        if taskbar == 0 {
            return Err(io::Error::other("TaskbarCreated kaydı başarısız"));
        }
        SendMessageW(hwnd, taskbar, 0, 0);
        if STATE.with(|s| s.borrow().mode) != Mode::Off {
            return Err(io::Error::other(
                "Explorer bildirimi güç durumunu değiştirdi",
            ));
        }
        change(hwnd, Mode::timed(GetTickCount64(), 1000), false)?;
    }
    Ok(())
}
