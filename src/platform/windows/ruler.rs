//! ScreenRuler overlay. Measurement math lives in `screen_ruler`; this module owns the UI thread.
use super::ruler_bar::{self, Hit};
use super::wide;
use crate::always_top::Hotkey;
use crate::screen_ruler::{
    AfterClick, BoundsRight, FEET_HALF, Mode, Span, Unit, bounds_right, clipboard_text,
    detect_edges, escape_copies, overlay_label, spacing_after_click, wheel_tolerance,
};
use crate::settings::Preferences;
use std::{
    cell::RefCell,
    io,
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        DataExchange::*,
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
    },
    UI::{
        Controls::WM_MOUSELEAVE,
        HiDpi::*,
        Input::KeyboardAndMouse::{
            GetKeyState, RegisterHotKey, ReleaseCapture, SetCapture, UnregisterHotKey, VK_CONTROL,
            VK_ESCAPE, VK_SHIFT,
        },
        WindowsAndMessaging::*,
    },
};
pub(super) const TIMER: usize = 4;
pub(super) const HOTKEY: i32 = 4;
const OVERLAY_CLASS: &str = "PowerKit.ScreenRuler.Overlay";
const TOOLBAR_CLASS: &str = "PowerKit.ScreenRuler.Toolbar";
const COLOR_KEY: u32 = 0x00FF_00FF;
const CAPTURE: u32 = SRCCOPY | 0x4000_0000;
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
thread_local! {
    static RULER: RefCell<Ruler> = RefCell::new(Ruler::default());
    static SHOTS: RefCell<Vec<Shot>> = const { RefCell::new(Vec::new()) };
}
#[derive(Clone)]
struct Config {
    key: Hotkey,
    mode: Mode,
    tolerance: u8,
    per_channel: bool,
    feet: bool,
    continuous: bool,
    unit: Unit,
    color: u32,
    language: crate::locale::UiLanguage,
}
impl Default for Config {
    fn default() -> Self {
        Self::from_prefs(&Preferences::default())
    }
}
impl Config {
    fn from_prefs(prefs: &Preferences) -> Self {
        Self {
            key: prefs.ruler_hotkey,
            mode: prefs.ruler_mode,
            tolerance: prefs.ruler_tolerance,
            per_channel: prefs.ruler_per_channel,
            feet: prefs.ruler_feet,
            continuous: prefs.ruler_continuous,
            unit: prefs.ruler_unit,
            color: prefs.ruler_color,
            language: crate::locale::UiLanguage::resolve(prefs.language),
        }
    }
}
struct Ruler {
    app: HWND,
    config: Config,
    hotkey: Option<String>,
    session: Option<Session>,
}
impl Default for Ruler {
    fn default() -> Self {
        Self {
            app: null_mut(),
            config: Config::default(),
            hotkey: None,
            session: None,
        }
    }
}
struct Session {
    app: HWND,
    mode: Mode,
    tolerance: u8,
    per_channel: bool,
    feet: bool,
    continuous: bool,
    unit: Unit,
    color: u32,
    monitors: Vec<Monitor>,
    toolbar: HWND,
}
struct Monitor {
    hwnd: HWND,
    origin_x: i32,
    origin_y: i32,
    width: i32,
    height: i32,
    dpi: f32,
    px_to_mm: f32,
    pixels: Vec<u32>,
    kept: Vec<Kept>,
    drag: Option<Drag>,
    dib: Option<Dib>,
}
struct Kept {
    cursor_x: i32,
    cursor_y: i32,
    span: Span,
}
#[derive(Clone, Copy)]
struct Drag {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}
struct Shot {
    origin_x: i32,
    origin_y: i32,
    width: i32,
    height: i32,
    dpi: f32,
    px_to_mm: f32,
    primary: bool,
    pixels: Vec<u32>,
}
struct Dib {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    width: i32,
    height: i32,
}
impl Dib {
    fn new(width: i32, height: i32) -> io::Result<Self> {
        unsafe {
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return Err(io::Error::other("Ekran aygıt bağlamı alınamadı"));
            }
            let dc = CreateCompatibleDC(screen);
            ReleaseDC(null_mut(), screen);
            if dc.is_null() {
                return Err(io::Error::other("Ölçüm yüzeyi oluşturulamadı"));
            }
            let mut info = std::mem::zeroed::<BITMAPINFO>();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width;
            info.bmiHeader.biHeight = -height;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(dc);
                return Err(io::Error::other("Ölçüm yüzeyi oluşturulamadı"));
            }
            let previous = SelectObject(dc, bitmap);
            Ok(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
                width,
                height,
            })
        }
    }
    fn release(&mut self) {
        unsafe {
            if !self.dc.is_null() {
                SelectObject(self.dc, self.previous);
                DeleteObject(self.bitmap);
                DeleteDC(self.dc);
            }
        }
        self.dc = null_mut();
        self.bitmap = null_mut();
        self.bits = null_mut();
    }
}
impl Drop for Dib {
    fn drop(&mut self) {
        self.release();
    }
}
pub(super) fn apply_prefs(prefs: &Preferences) {
    RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        ruler.config = Config::from_prefs(prefs);
        if let Some(session) = ruler.session.as_mut() {
            session.tolerance = prefs.ruler_tolerance;
            session.per_channel = prefs.ruler_per_channel;
            session.feet = prefs.ruler_feet;
            session.continuous = prefs.ruler_continuous;
            session.unit = prefs.ruler_unit;
            session.color = prefs.ruler_color;
        }
    });
    let app = RULER.with(|ruler| ruler.borrow().app);
    let toolbar = RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .map(|session| session.toolbar)
            .unwrap_or(null_mut())
    });
    let language = RULER.with(|ruler| ruler.borrow().config.language);
    if !toolbar.is_null() {
        ruler_bar::attach_tips(toolbar, tip_labels(language));
    }
    if !app.is_null() {
        install_hotkey(app);
        sync_timer();
        redraw();
    }
}
pub(super) fn listen(app: HWND) {
    RULER.with(|ruler| ruler.borrow_mut().app = app);
    install_hotkey(app);
}
pub(super) fn pause_hotkeys() {
    let app = RULER.with(|ruler| ruler.borrow().app);
    uninstall_hotkey(app);
}
pub(super) fn shutdown() {
    end_session();
    let app = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let app = ruler.app;
        ruler.app = null_mut();
        app
    });
    uninstall_hotkey(app);
}
pub(super) fn active() -> bool {
    RULER.with(|ruler| ruler.borrow().session.is_some())
}
pub(super) fn hotkey_label() -> Option<String> {
    RULER.with(|ruler| ruler.borrow().hotkey.clone())
}
pub(super) fn toggle(app: HWND) -> io::Result<()> {
    if active() {
        end_session();
        return Ok(());
    }
    open(app)
}
pub(super) fn on_timer() {
    if !RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| session.continuous)
    }) {
        return;
    }
    if let Ok(shots) = capture() {
        RULER.with(|ruler| {
            let mut ruler = ruler.borrow_mut();
            let Some(session) = ruler.session.as_mut() else {
                return;
            };
            if session.monitors.len() != shots.len() {
                return;
            }
            for (monitor, shot) in session.monitors.iter_mut().zip(shots) {
                if monitor.width == shot.width
                    && monitor.height == shot.height
                    && monitor.origin_x == shot.origin_x
                    && monitor.origin_y == shot.origin_y
                {
                    monitor.pixels = shot.pixels;
                    monitor.dpi = shot.dpi;
                    monitor.px_to_mm = shot.px_to_mm;
                }
            }
        });
        redraw();
    }
}
fn open(app: HWND) -> io::Result<()> {
    let config = RULER.with(|ruler| ruler.borrow().config.clone());
    let shots = capture()?;
    if shots.is_empty() {
        return Err(io::Error::other("Ölçülecek ekran bulunamadı"));
    }
    register_classes()?;
    let toolbar = create_toolbar(&shots)?;
    let monitors = shots
        .into_iter()
        .map(|shot| Monitor {
            hwnd: null_mut(),
            origin_x: shot.origin_x,
            origin_y: shot.origin_y,
            width: shot.width,
            height: shot.height,
            dpi: shot.dpi,
            px_to_mm: shot.px_to_mm,
            pixels: shot.pixels,
            kept: Vec::new(),
            drag: None,
            dib: None,
        })
        .collect();
    RULER.with(|ruler| {
        ruler.borrow_mut().session = Some(Session {
            app,
            mode: config.mode,
            tolerance: config.tolerance,
            per_channel: config.per_channel,
            feet: config.feet,
            continuous: config.continuous,
            unit: config.unit,
            color: config.color,
            monitors,
            toolbar,
        });
    });
    if config.mode != Mode::None {
        ensure_overlays()?;
    }
    sync_timer();
    unsafe {
        ShowWindow(toolbar, SW_SHOW);
        SetForegroundWindow(toolbar);
    }
    ruler_bar::attach_tips(toolbar, tip_labels(config.language));
    redraw();
    Ok(())
}
fn end_session() {
    let session = RULER.with(|ruler| ruler.borrow_mut().session.take());
    let Some(mut session) = session else {
        return;
    };
    unsafe {
        ClipCursor(null());
        KillTimer(session.app, TIMER);
        if !session.toolbar.is_null() {
            DestroyWindow(session.toolbar);
        }
        for monitor in &session.monitors {
            if !monitor.hwnd.is_null() {
                DestroyWindow(monitor.hwnd);
            }
        }
    }
    for monitor in &mut session.monitors {
        if let Some(dib) = monitor.dib.as_mut() {
            dib.release();
        }
    }
}
fn ensure_overlays() -> io::Result<()> {
    let instance = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()) };
    let pending = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let Some(session) = ruler.session.as_ref() else {
            return Vec::new();
        };
        session
            .monitors
            .iter()
            .enumerate()
            .filter(|(_, monitor)| monitor.hwnd.is_null())
            .map(|(index, monitor)| {
                (
                    index,
                    monitor.origin_x,
                    monitor.origin_y,
                    monitor.width,
                    monitor.height,
                )
            })
            .collect::<Vec<_>>()
    });
    for (index, x, y, width, height) in pending {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                wide(OVERLAY_CLASS).as_ptr(),
                wide("ScreenRuler").as_ptr(),
                WS_POPUP,
                x,
                y,
                width,
                height,
                null_mut(),
                null_mut(),
                instance,
                null(),
            )
        };
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        unsafe {
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
            SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        }
        RULER.with(|ruler| {
            if let Some(session) = ruler.borrow_mut().session.as_mut()
                && let Some(monitor) = session.monitors.get_mut(index)
            {
                monitor.hwnd = hwnd;
            }
        });
    }
    Ok(())
}
fn sync_timer() {
    let (app, on) = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let on = ruler
            .session
            .as_ref()
            .is_some_and(|session| session.continuous);
        (ruler.app, on)
    });
    if app.is_null() {
        return;
    }
    unsafe {
        if on {
            SetTimer(app, TIMER, 33, None);
        } else {
            KillTimer(app, TIMER);
        }
    }
}
fn install_hotkey(app: HWND) {
    uninstall_hotkey(app);
    let key = RULER.with(|ruler| ruler.borrow().config.key);
    let label = if register(app, key) {
        key.label()
    } else if key == Hotkey::ruler_default()
        && let Some(fallback) = fallback_alt(key)
        && register(app, fallback)
    {
        fallback.label()
    } else {
        key.label()
    };
    RULER.with(|ruler| ruler.borrow_mut().hotkey = Some(label));
}
fn fallback_alt(key: Hotkey) -> Option<Hotkey> {
    if key.ctrl && !key.alt {
        Hotkey::new(key.win, false, true, key.shift, key.vk)
    } else {
        None
    }
}
fn register(app: HWND, key: Hotkey) -> bool {
    unsafe { RegisterHotKey(app, HOTKEY, key.win32_mods(), u32::from(key.vk)) != 0 }
}
fn uninstall_hotkey(app: HWND) {
    if !app.is_null() {
        unsafe {
            UnregisterHotKey(app, HOTKEY);
        }
    }
    RULER.with(|ruler| ruler.borrow_mut().hotkey = None);
}
fn register_classes() -> io::Result<()> {
    unsafe {
        let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null());
        let cursor = LoadCursorW(null_mut(), IDC_ARROW);
        for (name, proc) in [
            (
                OVERLAY_CLASS,
                overlay_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            ),
            (TOOLBAR_CLASS, toolbar_proc),
        ] {
            let class_name = wide(name);
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(proc),
                hInstance: instance,
                hCursor: cursor,
                lpszClassName: class_name.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}
fn tip_labels(language: crate::locale::UiLanguage) -> [&'static str; 5] {
    let labels = language.strings();
    [
        labels.ruler_tip_bounds,
        labels.ruler_tip_spacing,
        labels.ruler_tip_horizontal,
        labels.ruler_tip_vertical,
        labels.ruler_tip_close,
    ]
}
fn toolbar_origin(primary: &Shot, width: i32, _height: i32, dpi: i32) -> (i32, i32) {
    let inset = ruler_bar::px(ruler_bar::TOP_DIP, dpi);
    let fallback = (
        primary.origin_x + (primary.width - width) / 2,
        primary.origin_y + inset,
    );
    let point = POINT {
        x: primary.origin_x + primary.width / 2,
        y: primary.origin_y + primary.height / 2,
    };
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return fallback;
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        fallback
    } else {
        let work = info.rcWork;
        (
            work.left + (work.right - work.left - width) / 2,
            work.top + inset,
        )
    }
}
fn create_toolbar(shots: &[Shot]) -> io::Result<HWND> {
    let primary = shots.iter().find(|shot| shot.primary).unwrap_or(&shots[0]);
    let dpi = primary.dpi.max(96.0) as i32;
    let width = ruler_bar::px(ruler_bar::WIDTH_DIP, dpi);
    let height = ruler_bar::px(ruler_bar::HEIGHT_DIP, dpi);
    let (x, y) = toolbar_origin(primary, width, height, dpi);
    let instance = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()) };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            wide(TOOLBAR_CLASS).as_ptr(),
            wide("ScreenRuler").as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    if hwnd.is_null() {
        Err(io::Error::last_os_error())
    } else {
        unsafe {
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
        ruler_bar::polish(hwnd);
        Ok(hwnd)
    }
}
fn capture() -> io::Result<Vec<Shot>> {
    SHOTS.with(|shots| shots.borrow_mut().clear());
    unsafe {
        EnumDisplayMonitors(null_mut(), null(), Some(enum_monitor), 0);
    }
    let shots = SHOTS.with(|shots| std::mem::take(&mut *shots.borrow_mut()));
    if shots.is_empty() {
        Err(io::Error::other("Ölçülecek ekran bulunamadı"))
    } else {
        Ok(shots)
    }
}
unsafe extern "system" fn enum_monitor(
    monitor: HMONITOR,
    _hdc: HDC,
    rect: *mut RECT,
    _data: LPARAM,
) -> i32 {
    if rect.is_null() {
        return 1;
    }
    let rect = unsafe { *rect };
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 3 || height < 3 {
        return 1;
    }
    if let Some(shot) = grab(monitor, rect, width, height) {
        SHOTS.with(|shots| shots.borrow_mut().push(shot));
    }
    1
}
fn grab(monitor: HMONITOR, rect: RECT, width: i32, height: i32) -> Option<Shot> {
    unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return None;
        }
        let Ok(dib) = Dib::new(width, height) else {
            ReleaseDC(null_mut(), screen);
            return None;
        };
        let copied = BitBlt(
            dib.dc, 0, 0, width, height, screen, rect.left, rect.top, CAPTURE,
        );
        ReleaseDC(null_mut(), screen);
        if copied == 0 || dib.bits.is_null() {
            return None;
        }
        let mut pixels = vec![0u32; (width as usize) * (height as usize)];
        std::ptr::copy_nonoverlapping(dib.bits, pixels.as_mut_ptr(), pixels.len());
        for pixel in &mut pixels {
            *pixel &= 0x00FF_FFFF;
            if *pixel == COLOR_KEY {
                *pixel = 0x00FF_00FE;
            }
        }
        let mut info = std::mem::zeroed::<MONITORINFOEXW>();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        let known = GetMonitorInfoW(monitor, (&raw mut info).cast()) != 0;
        let primary = known && info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let mut px_to_mm = 0.0f32;
        if known {
            let device = CreateDCW(info.szDevice.as_ptr(), null(), null(), null());
            if !device.is_null() {
                let mm = GetDeviceCaps(device, HORZSIZE as i32);
                let px = GetDeviceCaps(device, HORZRES as i32);
                if mm > 0 && px > 0 {
                    px_to_mm = mm as f32 / px as f32;
                }
                DeleteDC(device);
            }
        }
        Some(Shot {
            origin_x: rect.left,
            origin_y: rect.top,
            width,
            height,
            dpi: dpi_x.max(96) as f32,
            px_to_mm,
            primary,
            pixels,
        })
    }
}
fn redraw() {
    let snapshot = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        ruler.session.as_ref().map(|session| {
            session
                .monitors
                .iter()
                .enumerate()
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        })
    });
    let Some(indexes) = snapshot else {
        return;
    };
    for index in indexes {
        paint_monitor(index);
    }
    let toolbar = RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .map(|session| session.toolbar)
            .unwrap_or(null_mut())
    });
    if !toolbar.is_null() {
        unsafe {
            InvalidateRect(toolbar, null(), 0);
        }
    }
}
fn paint_monitor(index: usize) {
    let ready = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let Some(session) = ruler.session.as_mut() else {
            return false;
        };
        let Some(monitor) = session.monitors.get_mut(index) else {
            return false;
        };
        if monitor.hwnd.is_null() {
            return false;
        }
        if monitor
            .dib
            .as_ref()
            .is_none_or(|dib| dib.width != monitor.width || dib.height != monitor.height)
        {
            if let Some(dib) = monitor.dib.as_mut() {
                dib.release();
            }
            monitor.dib = Dib::new(monitor.width, monitor.height).ok();
        }
        monitor.dib.is_some()
    });
    if !ready {
        return;
    }
    RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let Some(session) = ruler.session.as_ref() else {
            return;
        };
        let Some(monitor) = session.monitors.get(index) else {
            return;
        };
        let Some(dib) = monitor.dib.as_ref() else {
            return;
        };
        compose(session, monitor, dib);
        let origin = POINT {
            x: monitor.origin_x,
            y: monitor.origin_y,
        };
        let size = SIZE {
            cx: monitor.width,
            cy: monitor.height,
        };
        let zero = POINT { x: 0, y: 0 };
        unsafe {
            UpdateLayeredWindow(
                monitor.hwnd,
                null_mut(),
                &origin,
                &size,
                dib.dc,
                &zero,
                COLOR_KEY,
                null(),
                ULW_COLORKEY,
            );
        }
    });
}
fn compose(session: &Session, monitor: &Monitor, dib: &Dib) {
    let count = (monitor.width as usize) * (monitor.height as usize);
    unsafe {
        if session.continuous {
            std::ptr::write_bytes(dib.bits, 0, count);
            for index in 0..count {
                *dib.bits.add(index) = COLOR_KEY;
            }
        } else {
            std::ptr::copy_nonoverlapping(monitor.pixels.as_ptr(), dib.bits, count);
        }
    }
    let cursor = cursor_on(monitor);
    let over_toolbar = toolbar_contains_cursor();
    if let Some(drag) = monitor.drag {
        draw_bounds(
            dib,
            session,
            monitor,
            Span::bounds(drag.x0, drag.y0, drag.x1, drag.y1),
            Some((drag.x1, drag.y1)),
        );
    }
    for kept in &monitor.kept {
        match session.mode {
            Mode::Bounds => draw_bounds(dib, session, monitor, kept.span, None),
            Mode::Spacing | Mode::Horizontal | Mode::Vertical => draw_cross(
                dib,
                session,
                monitor,
                kept.span,
                kept.cursor_x,
                kept.cursor_y,
            ),
            Mode::None => {}
        }
    }
    if over_toolbar || cursor.is_none() {
        return;
    }
    let (x, y) = cursor.unwrap();
    if session.mode.edges()
        && let Some(span) = detect_edges(
            &monitor.pixels,
            monitor.width,
            monitor.height,
            x,
            y,
            session.per_channel,
            session.tolerance,
        )
    {
        draw_cross(dib, session, monitor, span, x, y);
    }
}
fn cursor_on(monitor: &Monitor) -> Option<(i32, i32)> {
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
    }
    let x = point.x - monitor.origin_x;
    let y = point.y - monitor.origin_y;
    (x >= 0 && y >= 0 && x < monitor.width && y < monitor.height).then_some((x, y))
}
fn toolbar_contains_cursor() -> bool {
    let toolbar = RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .map(|session| session.toolbar)
            .unwrap_or(null_mut())
    });
    if toolbar.is_null() {
        return false;
    }
    let mut cursor = POINT { x: 0, y: 0 };
    let mut rect = RECT::default();
    unsafe {
        GetCursorPos(&mut cursor) != 0
            && GetWindowRect(toolbar, &mut rect) != 0
            && PtInRect(&rect, cursor) != 0
    }
}
fn draw_bounds(
    dib: &Dib,
    session: &Session,
    monitor: &Monitor,
    span: Span,
    cursor: Option<(i32, i32)>,
) {
    let color = line_color(session.color);
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let brush = GetStockObject(NULL_BRUSH);
        let old_pen = SelectObject(dib.dc, pen);
        let old_brush = SelectObject(dib.dc, brush);
        Rectangle(dib.dc, span.left, span.top, span.right + 1, span.bottom + 1);
        SelectObject(dib.dc, old_pen);
        SelectObject(dib.dc, old_brush);
        DeleteObject(pen);
    }
    let anchor = cursor.unwrap_or(((span.left + span.right) / 2, (span.top + span.bottom) / 2));
    draw_label(
        dib,
        monitor,
        session,
        span,
        Mode::Bounds,
        anchor.0,
        anchor.1,
    );
}
fn draw_cross(dib: &Dib, session: &Session, monitor: &Monitor, span: Span, x: i32, y: i32) {
    let (horizontal, vertical) = session.mode.axes();
    let color = line_color(session.color);
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old = SelectObject(dib.dc, pen);
        if horizontal {
            line(dib.dc, span.left, y, span.right, y);
            if session.feet {
                foot(dib.dc, span.left, y, false);
                foot(dib.dc, span.right, y, false);
            }
        }
        if vertical {
            line(dib.dc, x, span.top, x, span.bottom);
            if session.feet {
                foot(dib.dc, x, span.top, true);
                foot(dib.dc, x, span.bottom, true);
            }
        }
        SelectObject(dib.dc, old);
        DeleteObject(pen);
    }
    draw_label(dib, monitor, session, span, session.mode, x, y);
}
fn line(dc: HDC, x0: i32, y0: i32, x1: i32, y1: i32) {
    unsafe {
        MoveToEx(dc, x0, y0, null_mut());
        LineTo(dc, x1 + (x1 - x0).signum(), y1 + (y1 - y0).signum());
    }
}
fn foot(dc: HDC, x: i32, y: i32, horizontal: bool) {
    if horizontal {
        line(dc, x - FEET_HALF, y, x + FEET_HALF, y);
    } else {
        line(dc, x, y - FEET_HALF, x, y + FEET_HALF);
    }
}
fn line_color(color: u32) -> u32 {
    if color == COLOR_KEY {
        0x00FF_00FE
    } else {
        color
    }
}
fn draw_label(
    dib: &Dib,
    monitor: &Monitor,
    session: &Session,
    span: Span,
    mode: Mode,
    x: i32,
    y: i32,
) {
    let text = overlay_label(span, mode, session.unit, monitor.px_to_mm, monitor.dpi);
    let mut paint = wide(&text);
    unsafe {
        let dpi = monitor.dpi as i32;
        let font = CreateFontW(
            -(14 * dpi / 96),
            0,
            0,
            0,
            FW_SEMIBOLD as i32,
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
        let previous = SelectObject(dib.dc, font);
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        DrawTextW(
            dib.dc,
            paint.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_CALCRECT | DT_NOPREFIX,
        );
        let pad = (6 * dpi / 96).max(4);
        let width = bounds.right - bounds.left + pad * 2;
        let height = bounds.bottom - bounds.top + pad * 2;
        let mut left = if x < monitor.width / 2 {
            x + 16
        } else {
            x - width - 16
        };
        let mut top = if y < monitor.height / 2 {
            y + 16
        } else {
            y - height - 16
        };
        left = left.clamp(4, (monitor.width - width - 4).max(4));
        top = top.clamp(4, (monitor.height - height - 4).max(4));
        let background = CreateSolidBrush(0x001E_1E1E);
        let rect = RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        };
        FillRect(dib.dc, &rect, background);
        DeleteObject(background);
        SetBkMode(dib.dc, TRANSPARENT as i32);
        SetTextColor(dib.dc, 0x00FF_FFFF);
        let mut text_rect = RECT {
            left: left + pad,
            top: top + pad,
            right: left + width - pad,
            bottom: top + height - pad,
        };
        DrawTextW(dib.dc, paint.as_mut_ptr(), -1, &mut text_rect, DT_NOPREFIX);
        SelectObject(dib.dc, previous);
        DeleteObject(font);
    }
}
fn copy_measurements(monitor_index: usize, current: Option<Span>) {
    let text = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let Some(session) = ruler.session.as_ref() else {
            return String::new();
        };
        let mut spans = Vec::new();
        for (index, monitor) in session.monitors.iter().enumerate() {
            spans.extend(monitor.kept.iter().map(|kept| kept.span));
            if let Some(drag) = monitor.drag {
                spans.push(Span::bounds(drag.x0, drag.y0, drag.x1, drag.y1));
            }
            if index == monitor_index
                && let Some(span) = current
            {
                spans.push(span);
            }
        }
        clipboard_text(
            &spans,
            session.mode,
            session.unit,
            session
                .monitors
                .get(monitor_index)
                .map(|m| m.px_to_mm)
                .unwrap_or(0.0),
            session
                .monitors
                .get(monitor_index)
                .map(|m| m.dpi)
                .unwrap_or(96.0),
        )
    });
    set_clipboard(&text);
}
fn set_clipboard(text: &str) {
    if text.is_empty() {
        return;
    }
    let wide_text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let handle = GlobalAlloc(GMEM_MOVEABLE, wide_text.len() * 2);
        if !handle.is_null() {
            let locked = GlobalLock(handle).cast::<u16>();
            if !locked.is_null() {
                locked.copy_from_nonoverlapping(wide_text.as_ptr(), wide_text.len());
                GlobalUnlock(handle);
                if SetClipboardData(13, handle).is_null() {
                    GlobalFree(handle);
                }
            } else {
                GlobalFree(handle);
            }
        }
        CloseClipboard();
    }
}
fn monitor_index(hwnd: HWND) -> Option<usize> {
    RULER.with(|ruler| {
        ruler.borrow().session.as_ref().and_then(|session| {
            session
                .monitors
                .iter()
                .position(|monitor| monitor.hwnd == hwnd)
        })
    })
}
fn point_from(l: LPARAM) -> (i32, i32) {
    let x = (l & 0xffff) as i16 as i32;
    let y = ((l >> 16) & 0xffff) as i16 as i32;
    (x, y)
}
fn shift_down() -> bool {
    unsafe { GetKeyState(VK_SHIFT as i32) < 0 }
}
fn ctrl_down() -> bool {
    unsafe { GetKeyState(VK_CONTROL as i32) < 0 }
}
fn on_key(vk: WPARAM) {
    if ctrl_down() {
        let mode = match vk as u16 {
            0x31 => Some(Mode::Bounds),
            0x32 => Some(Mode::Spacing),
            0x33 => Some(Mode::Horizontal),
            0x34 => Some(Mode::Vertical),
            _ => None,
        };
        if let Some(mode) = mode {
            select_tool(mode);
            return;
        }
    }
    if vk as u16 == VK_ESCAPE {
        escape();
    }
}
fn select_tool(mode: Mode) {
    let current = RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .map(|session| session.mode)
            .unwrap_or(Mode::None)
    });
    set_mode(current.toggled(mode));
}
fn set_mode(mode: Mode) {
    let changed = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let Some(session) = ruler.session.as_mut() else {
            return false;
        };
        if session.mode == mode {
            return false;
        }
        session.mode = mode;
        for monitor in &mut session.monitors {
            monitor.kept.clear();
            monitor.drag = None;
        }
        true
    });
    if !changed {
        return;
    }
    if mode == Mode::None {
        redraw();
        return;
    }
    if ensure_overlays().is_ok() {
        redraw();
    }
}
fn escape() {
    let copy = RULER.with(|ruler| {
        ruler
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| escape_copies(session.mode))
    });
    if copy {
        copy_measurements(usize::MAX, None);
    }
    end_session();
}
fn on_down(hwnd: HWND, l: LPARAM) {
    let Some(index) = monitor_index(hwnd) else {
        return;
    };
    let mode = RULER.with(|ruler| ruler.borrow().session.as_ref().map(|s| s.mode));
    if mode != Some(Mode::Bounds) || toolbar_contains_cursor() {
        return;
    }
    let (x, y) = point_from(l);
    RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let Some(monitor) = ruler
            .session
            .as_mut()
            .and_then(|session| session.monitors.get_mut(index))
        else {
            return;
        };
        monitor.drag = Some(Drag {
            x0: x,
            y0: y,
            x1: x,
            y1: y,
        });
    });
    unsafe {
        SetCapture(hwnd);
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect) != 0 {
            ClipCursor(&rect);
        }
    }
    redraw();
}
fn on_move(hwnd: HWND, l: LPARAM) {
    let Some(index) = monitor_index(hwnd) else {
        redraw();
        return;
    };
    let (x, y) = point_from(l);
    RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        if let Some(drag) = ruler
            .session
            .as_mut()
            .and_then(|session| session.monitors.get_mut(index))
            .and_then(|monitor| monitor.drag.as_mut())
        {
            drag.x1 = x;
            drag.y1 = y;
        }
    });
    redraw();
}
fn on_up(hwnd: HWND) {
    let Some(index) = monitor_index(hwnd) else {
        return;
    };
    unsafe {
        ReleaseCapture();
        ClipCursor(null());
    }
    let outcome = RULER.with(|ruler| {
        let mut ruler = ruler.borrow_mut();
        let session = ruler.session.as_mut()?;
        let mode = session.mode;
        let per_channel = session.per_channel;
        let tolerance = session.tolerance;
        let monitor = session.monitors.get_mut(index)?;
        if mode == Mode::Bounds {
            let drag = monitor.drag.take()?;
            return Some((
                mode,
                Span::bounds(drag.x0, drag.y0, drag.x1, drag.y1),
                shift_down(),
                drag.x1,
                drag.y1,
            ));
        }
        if !mode.edges() {
            return None;
        }
        let (x, y) = cursor_on(monitor)?;
        let span = detect_edges(
            &monitor.pixels,
            monitor.width,
            monitor.height,
            x,
            y,
            per_channel,
            tolerance,
        )?;
        Some((
            mode,
            span,
            spacing_after_click(shift_down()) == AfterClick::Keep,
            x,
            y,
        ))
    });
    let Some((mode, span, keep, x, y)) = outcome else {
        redraw();
        return;
    };
    copy_measurements(index, Some(span));
    if keep {
        RULER.with(|ruler| {
            if let Some(monitor) = ruler
                .borrow_mut()
                .session
                .as_mut()
                .and_then(|session| session.monitors.get_mut(index))
            {
                monitor.kept.push(Kept {
                    cursor_x: x,
                    cursor_y: y,
                    span,
                });
            }
        });
    }
    if mode.edges() && !keep {
        end_session();
    } else {
        redraw();
    }
}
fn on_right(hwnd: HWND) {
    let Some(index) = monitor_index(hwnd) else {
        return;
    };
    let action = RULER.with(|ruler| {
        let ruler = ruler.borrow();
        let session = ruler.session.as_ref()?;
        let monitor = session.monitors.get(index)?;
        Some(if session.mode == Mode::Bounds {
            bounds_right(monitor.drag.is_some(), !monitor.kept.is_empty())
        } else {
            BoundsRight::Close
        })
    });
    match action {
        Some(BoundsRight::CancelDrag) => {
            RULER.with(|ruler| {
                if let Some(monitor) = ruler
                    .borrow_mut()
                    .session
                    .as_mut()
                    .and_then(|session| session.monitors.get_mut(index))
                {
                    monitor.drag = None;
                }
            });
            unsafe {
                ReleaseCapture();
                ClipCursor(null());
            }
            redraw();
        }
        Some(BoundsRight::ClearKept) => {
            RULER.with(|ruler| {
                if let Some(monitor) = ruler
                    .borrow_mut()
                    .session
                    .as_mut()
                    .and_then(|session| session.monitors.get_mut(index))
                {
                    monitor.kept.clear();
                }
            });
            redraw();
        }
        _ => end_session(),
    }
}
fn on_wheel(w: WPARAM) {
    let delta = ((w >> 16) & 0xffff) as u16 as i16;
    RULER.with(|ruler| {
        if let Some(session) = ruler.borrow_mut().session.as_mut() {
            session.tolerance = wheel_tolerance(session.tolerance, delta);
        }
    });
    redraw();
}
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_SETCURSOR => {
            unsafe {
                SetCursor(null_mut());
            }
            1
        }
        WM_LBUTTONDOWN => {
            on_down(hwnd, l);
            0
        }
        WM_MOUSEMOVE => {
            on_move(hwnd, l);
            0
        }
        WM_LBUTTONUP => {
            on_up(hwnd);
            0
        }
        WM_RBUTTONUP => {
            on_right(hwnd);
            0
        }
        WM_MOUSEWHEEL => {
            on_wheel(w);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            on_key(w);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}
unsafe extern "system" fn toolbar_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mode = RULER.with(|ruler| {
                ruler
                    .borrow()
                    .session
                    .as_ref()
                    .map(|session| session.mode)
                    .unwrap_or(Mode::None)
            });
            ruler_bar::paint(hwnd, mode);
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = point_from(l);
            ruler_bar::pointer_move(hwnd, x, y);
            0
        }
        WM_MOUSELEAVE => {
            ruler_bar::pointer_leave(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = point_from(l);
            ruler_bar::pointer_down(hwnd, x, y);
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = point_from(l);
            if let Some(hit) = ruler_bar::pointer_up(hwnd, x, y) {
                match hit {
                    Hit::Close => end_session(),
                    Hit::Tool(mode) => select_tool(mode),
                }
            }
            0
        }
        WM_NCDESTROY => {
            ruler_bar::clear();
            0
        }
        WM_MOUSEWHEEL => {
            on_wheel(w);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            on_key(w);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}
