//! Screen capture: per-monitor GDI surfaces, the first capture of every monitor
//! and in-place recapture for continuous mode and tool changes.
use super::*;

/// `SRCCOPY | CAPTUREBLT`: include layered windows the user sees; the ruler's own
/// windows are kept out by `WDA_EXCLUDEFROMCAPTURE`.
const CAPTURE: u32 = SRCCOPY | 0x4000_0000;

thread_local! {
    /// Filled by `enum_monitor` during one `capture` call on this UI thread.
    static SHOTS: RefCell<Vec<Shot>> = const { RefCell::new(Vec::new()) };
}

/// One monitor as captured when a session opens.
pub(super) struct Shot {
    pub(super) origin_x: i32,
    pub(super) origin_y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) dpi: f32,
    pub(super) px_to_mm: f32,
    pub(super) primary: bool,
    pub(super) pixels: Vec<u32>,
    pub(super) surface: Dib,
}

/// A 32-bit top-down DIB section selected into its own memory DC. Owned by one
/// monitor and used only on the UI thread that created it.
pub(super) struct Dib {
    pub(super) dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pub(super) bits: *mut u32,
    pub(super) width: i32,
    pub(super) height: i32,
    /// Label font and the DPI it was made for. Owned by this surface, never left
    /// selected into `dc`, and deleted in `release`.
    pub(super) font: Cell<(HFONT, i32)>,
}
impl Dib {
    pub(super) fn new(width: i32, height: i32) -> io::Result<Self> {
        unsafe {
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return Err(Fault::ScreenContext.into());
            }
            let dc = CreateCompatibleDC(screen);
            ReleaseDC(null_mut(), screen);
            if dc.is_null() {
                return Err(Fault::MeasureSurface.into());
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
                return Err(Fault::MeasureSurface.into());
            }
            let previous = SelectObject(dc, bitmap);
            Ok(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
                width,
                height,
                font: Cell::new((null_mut(), 0)),
            })
        }
    }
    pub(super) fn release(&mut self) {
        unsafe {
            let (font, _) = self.font.replace((null_mut(), 0));
            if !font.is_null() {
                DeleteObject(font);
            }
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

/// Captures every monitor once, in `EnumDisplayMonitors` order.
pub(super) fn capture() -> io::Result<Vec<Shot>> {
    SHOTS.with(|shots| shots.borrow_mut().clear());
    unsafe {
        EnumDisplayMonitors(null_mut(), null(), Some(enum_monitor), 0);
    }
    let shots = SHOTS.with(|shots| std::mem::take(&mut *shots.borrow_mut()));
    if shots.is_empty() {
        Err(Fault::NoDisplay.into())
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
    // SAFETY: EnumDisplayMonitors passes a valid monitor RECT for this call.
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
/// Copies the screen rectangle at `(x, y)` into `surface`, which must already have
/// the rectangle's size.
fn blit_screen(surface: &Dib, x: i32, y: i32) -> bool {
    unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return false;
        }
        let copied = BitBlt(
            surface.dc,
            0,
            0,
            surface.width,
            surface.height,
            screen,
            x,
            y,
            CAPTURE,
        );
        ReleaseDC(null_mut(), screen);
        GdiFlush();
        copied != 0 && !surface.bits.is_null()
    }
}
/// Copies a captured surface into `pixels` (reusing its allocation), dropping alpha
/// and moving the overlay color key off real pixels so captured magenta never
/// turns transparent.
fn read_surface(surface: &Dib, pixels: &mut Vec<u32>) {
    let count = (surface.width as usize) * (surface.height as usize);
    pixels.resize(count, 0);
    // SAFETY: `bits` addresses the DIB section's width*height 32-bit pixels. The
    // section is owned by `surface`, lives until `release`, and GDI finished
    // writing it (`blit_screen` flushed the batch). Only this UI thread uses it.
    let source = unsafe { std::slice::from_raw_parts(surface.bits, count) };
    for (pixel, &value) in pixels.iter_mut().zip(source) {
        let value = value & 0x00FF_FFFF;
        *pixel = if value == COLOR_KEY { KEY_SAFE } else { value };
    }
}
/// Recaptures one monitor in place, reusing its capture surface.
pub(super) fn regrab(monitor: &mut Monitor) -> bool {
    if monitor
        .capture
        .as_ref()
        .is_none_or(|surface| surface.width != monitor.width || surface.height != monitor.height)
    {
        monitor.capture = Dib::new(monitor.width, monitor.height).ok();
    }
    let Some(surface) = monitor.capture.as_ref() else {
        return false;
    };
    if !blit_screen(surface, monitor.origin_x, monitor.origin_y) {
        return false;
    }
    read_surface(surface, &mut monitor.pixels);
    true
}
fn grab(monitor: HMONITOR, rect: RECT, width: i32, height: i32) -> Option<Shot> {
    unsafe {
        let surface = Dib::new(width, height).ok()?;
        if !blit_screen(&surface, rect.left, rect.top) {
            return None;
        }
        let mut pixels = Vec::new();
        read_surface(&surface, &mut pixels);
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
            surface,
        })
    }
}
