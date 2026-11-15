//! Isolated native message-loop acceptance checks; never accesses user settings.
use super::command::{CUSTOM, DISPLAY, EXTEND, FOREVER, OFF};
use super::*;

fn flags() -> u32 {
    // SAFETY: re-asserting the flags returns the previous ones for this thread.
    unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) }
}

pub(super) fn run(hwnd: HWND) -> io::Result<()> {
    // SAFETY: hwnd is our live isolated window, invoked only on its owning UI thread.
    // Exercise real command handlers and resume reconciliation without user preferences.
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, FOREVER, 0);
        if awake::session() != Session::Indefinite {
            return Err(io::Error::other("Süresiz komutu başarısız"));
        }
        awake::set_display(hwnd, true)?;
        let required = ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED;
        if flags() & required != required {
            return Err(io::Error::other("Güç bayrakları eksik"));
        }
        awake::set_display(hwnd, false)?;
        SendMessageW(hwnd, WM_COMMAND, CUSTOM, 0);
        let Session::Interval {
            custom: true, end, ..
        } = awake::session()
        else {
            return Err(io::Error::other("Özel süre komutu başarısız"));
        };
        // Toggling the display keeps the running deadline and asserts the display flag.
        SendMessageW(hwnd, WM_COMMAND, DISPLAY, 0);
        if awake::session().deadline() != Some(end) || !with_state(|s| s.prefs.display) {
            return Err(io::Error::other("Ekran tercihi süreli oturumu değiştirdi"));
        }
        if flags() & required != required {
            return Err(io::Error::other("Ekran bayrağı süreli oturumda eksik"));
        }
        SendMessageW(hwnd, WM_COMMAND, DISPLAY, 0);
        if with_state(|s| s.prefs.display) || flags() & ES_DISPLAY_REQUIRED != 0 {
            return Err(io::Error::other("Ekran tercihi kapatılamadı"));
        }
        SendMessageW(hwnd, WM_COMMAND, EXTEND, 0);
        if awake::session().deadline() != Some(end + 30 * crate::always_on::MINUTE_MS) {
            return Err(io::Error::other("Uzatma komutu başarısız"));
        }
        SendMessageW(hwnd, WM_COMMAND, OFF, 0);
        if awake::session() != Session::Off {
            return Err(io::Error::other("Durdurma komutu başarısız"));
        }
        top::smoke(hwnd)?;
        let now = GetTickCount64();
        awake::start(
            hwnd,
            Session::Interval {
                end: now.saturating_sub(1),
                minutes: 0,
                custom: false,
            },
        )?;
        SendMessageW(hwnd, WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, 0);
        if awake::session() != Session::Off {
            return Err(io::Error::other("Uyku dönüşü süresi geçmiş oturum durmadı"));
        }
        // A clock change ends an expiry whose wall-clock target already passed.
        let utc = clock::now_utc();
        awake::start(
            hwnd,
            Session::Expiry {
                end: now.saturating_add(3_600_000),
                target: utc - 1,
            },
        )?;
        SendMessageW(hwnd, WM_TIMECHANGE, 0, 0);
        if awake::session() != Session::Off {
            return Err(io::Error::other(
                "Saat değişikliği geçmiş bitişi durdurmadı",
            ));
        }
        // Binding to this process is refused; a running session is left alone.
        if awake::bind(hwnd, std::process::id()).is_ok() || awake::session() != Session::Off {
            return Err(io::Error::other("PowerKit kendi sürecine bağlandı"));
        }
        // A command-line request arriving at the running instance starts a session.
        let requests = with_state(|s| s.request_message);
        let (kind, value) = Request::Minutes(5).encode();
        SendMessageW(hwnd, requests, kind, value);
        if !matches!(awake::session(), Session::Interval { minutes: 5, .. }) {
            return Err(io::Error::other("Komut satırı isteği uygulanmadı"));
        }
        SendMessageW(hwnd, WM_COMMAND, OFF, 0);
        // There is deliberately no shell icon for this test HWND. A failed update must
        // clear the tray flag, so X cannot hide an inaccessible window.
        with_state_mut(|s| s.tray_available = true);
        update_view(hwnd, true);
        if with_state(|s| s.tray_available) {
            return Err(io::Error::other("Eksik bildirim simgesi algılanmadı"));
        }
        // A simulated Explorer restart must leave the power session unchanged.
        let taskbar = with_state(|s| s.taskbar_message);
        if taskbar == 0 {
            return Err(io::Error::other("TaskbarCreated kaydı başarısız"));
        }
        SendMessageW(hwnd, taskbar, 0, 0);
        if awake::session() != Session::Off {
            return Err(io::Error::other(
                "Explorer bildirimi güç durumunu değiştirdi",
            ));
        }
        awake::start(
            hwnd,
            Session::Interval {
                end: GetTickCount64().saturating_add(1000),
                minutes: 0,
                custom: false,
            },
        )?;
    }
    Ok(())
}
