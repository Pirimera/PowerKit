//! The Actions Ring on screen: a layered, never-activated popup centred on
//! the cursor. It lives on the UI thread. The foreground application keeps
//! focus, so the chosen action reaches it; the ring therefore reads the
//! cursor, the mouse buttons and Esc by polling instead of taking input focus.
#![expect(
    unsafe_code,
    reason = "layered window, GDI surface and UpdateLayeredWindow for the ring"
)]
use powerkit_core::power_mouse::ring::{self, Palette, SLOTS, Shade};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
    Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER,
        BLENDFUNCTION, CreateCompatibleDC, CreateDIBSection, CreateFontW, DIB_RGB_COLORS,
        DT_CALCRECT, DT_CENTER, DT_NOPREFIX, DT_WORDBREAK, DeleteDC, DeleteObject, DrawTextW,
        FW_SEMIBOLD, GetDC, GetMonitorInfoW, HBITMAP, HDC, HFONT, HGDIOBJ,
        MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject,
        SetBkMode, SetTextColor, TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, KillTimer, MA_NOACTIVATE,
            RegisterClassExW, SW_SHOWNOACTIVATE, SetTimer, ShowWindow, ULW_ALPHA,
            UpdateLayeredWindow, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_TIMER, WNDCLASSEXW,
            WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
};

const CLASS: &str = "PowerKitMouseRing";
const TIMER: usize = 1;
/// How often the cursor and buttons are read while the ring is open.
const POLL_MS: u32 = 30;
/// The ring closes by itself after this long without a choice.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Ring background and label colours; the hover colour is the Windows accent.
const BACKGROUND: u32 = 0x1F_1F_1F;
const LABEL: COLORREF = 0x00FF_FFFF;

thread_local! {
    static RING: RefCell<Option<Open>> = const { RefCell::new(None) };
    static CLASS_REGISTERED: RefCell<bool> = const { RefCell::new(false) };
}

/// A 32-bit top-down DIB selected into its own memory DC, with the label
/// font. Owned by the open ring and used only on the UI thread.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    font: HFONT,
    size: i32,
}

impl Surface {
    fn new(size: i32, dpi: u32) -> Option<Self> {
        // SAFETY: every handle created here is owned by the returned surface
        // and released in Drop; on a partial failure the ones made so far are
        // released before returning.
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            if screen.is_null() {
                return None;
            }
            let dc = CreateCompatibleDC(screen);
            ReleaseDC(std::ptr::null_mut(), screen);
            if dc.is_null() {
                return None;
            }
            let mut info = BITMAPINFO::default();
            info.bmiHeader.biSize = u32::try_from(size_of::<BITMAPINFOHEADER>()).ok()?;
            info.bmiHeader.biWidth = size;
            info.bmiHeader.biHeight = -size;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                dc,
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                std::ptr::null_mut(),
                0,
            );
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(dc);
                return None;
            }
            let previous = SelectObject(dc, bitmap);
            let face: Vec<u16> = "Segoe UI Variable Text\0".encode_utf16().collect();
            // 10 pt at the monitor's DPI; negative height selects by character height.
            let height = -(10 * i32::try_from(dpi).unwrap_or(96) / 72);
            let font = CreateFontW(
                height,
                0,
                0,
                0,
                FW_SEMIBOLD as i32,
                0,
                0,
                0,
                0,
                0,
                0,
                // Greyscale smoothing: ClearType's colour fringes do not
                // survive a per-pixel-alpha surface.
                u32::from(ANTIALIASED_QUALITY),
                0,
                face.as_ptr(),
            );
            Some(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
                font,
                size,
            })
        }
    }

    fn pixels(&mut self) -> &mut [u32] {
        let len = usize::try_from(self.size * self.size).unwrap_or(0);
        // SAFETY: bits points at the DIB section's size*size 32-bit pixels,
        // which live as long as the bitmap this surface owns.
        unsafe { std::slice::from_raw_parts_mut(self.bits, len) }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: the handles are owned by this surface and released once.
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
            DeleteDC(self.dc);
        }
    }
}

struct Open {
    hwnd: HWND,
    surface: Surface,
    /// Screen position of the window's top-left corner.
    origin: POINT,
    /// Label of each slot; `None` for a slot with no action.
    labels: [Option<String>; SLOTS],
    hover: Option<usize>,
    opened: Instant,
    /// Button and Esc state at the last poll, to act on the press edge only.
    left: bool,
    dismiss: bool,
}

pub fn is_open() -> bool {
    RING.with(|ring| ring.borrow().is_some())
}

/// Opens the ring around the cursor. Returns `false` when it could not be
/// shown; a ring with no bound slot is not opened at all.
pub fn open(labels: [Option<String>; SLOTS]) -> bool {
    close();
    if labels.iter().all(Option::is_none) {
        return false;
    }
    let mut cursor = POINT::default();
    // SAFETY: cursor is a writable POINT.
    if unsafe { GetCursorPos(&raw mut cursor) } == 0 {
        return false;
    }
    let (dpi, work) = monitor_at(cursor);
    let size = ring::DIAMETER_96 * i32::try_from(dpi).unwrap_or(96) / 96;
    // Centred on the cursor, pulled back inside the monitor's work area.
    let origin = POINT {
        x: (cursor.x - size / 2).clamp(work.left, (work.right - size).max(work.left)),
        y: (cursor.y - size / 2).clamp(work.top, (work.bottom - size).max(work.top)),
    };
    let Some(surface) = Surface::new(size, dpi) else {
        return false;
    };
    let Some(hwnd) = create_window(origin, size) else {
        return false;
    };
    let mut ring = Open {
        hwnd,
        surface,
        origin,
        labels,
        hover: None,
        opened: Instant::now(),
        left: key_down(VK_LBUTTON),
        dismiss: dismiss_down(),
    };
    ring.hover = ring.slot_under(cursor);
    ring.paint();
    // SAFETY: hwnd was just created on this thread.
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, TIMER, POLL_MS, None);
    }
    RING.with(|slot| *slot.borrow_mut() = Some(ring));
    true
}

/// Closes the ring without choosing anything.
pub fn close() {
    if let Some(ring) = RING.with(|slot| slot.borrow_mut().take()) {
        // SAFETY: the window belongs to this thread and is destroyed once;
        // the surface is dropped after it.
        unsafe {
            KillTimer(ring.hwnd, TIMER);
            DestroyWindow(ring.hwnd);
        }
    }
}

fn monitor_at(point: POINT) -> (u32, RECT) {
    // SAFETY: MONITOR_DEFAULTTONEAREST always yields a monitor; the out
    // parameters are writable and info has its size set.
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let (mut dpi, mut dpi_y) = (96u32, 96u32);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi, &raw mut dpi_y) < 0 {
            dpi = 96;
        }
        let mut info = MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(0),
            ..MONITORINFO::default()
        };
        let work = if GetMonitorInfoW(monitor, &raw mut info) != 0 {
            info.rcWork
        } else {
            RECT {
                left: point.x - 1000,
                top: point.y - 1000,
                right: point.x + 1000,
                bottom: point.y + 1000,
            }
        };
        (dpi.max(48), work)
    }
}

fn create_window(origin: POINT, size: i32) -> Option<HWND> {
    let class: Vec<u16> = CLASS.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: the class name and window procedure outlive the window; the
    // class is registered once per UI thread lifetime.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        if !CLASS_REGISTERED.with(|done| done.replace(true)) {
            let definition = WNDCLASSEXW {
                cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).ok()?,
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..WNDCLASSEXW::default()
            };
            RegisterClassExW(&raw const definition);
        }
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            origin.x,
            origin.y,
            size,
            size,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        (!hwnd.is_null()).then_some(hwnd)
    }
}

fn key_down(vk: u16) -> bool {
    // SAFETY: reads a key state; the high bit means down right now.
    unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// Esc, or a right or middle click: anything that means "never mind".
fn dismiss_down() -> bool {
    key_down(VK_ESCAPE) || key_down(VK_RBUTTON) || key_down(VK_MBUTTON)
}

impl Open {
    /// The bound slot under a screen point.
    fn slot_under(&self, point: POINT) -> Option<usize> {
        let size = self.surface.size;
        let half = size / 2;
        ring::slot_at(
            point.x - self.origin.x - half,
            point.y - self.origin.y - half,
            size,
        )
        .filter(|slot| self.labels[*slot].is_some())
    }

    fn paint(&mut self) {
        self.compose();
        self.present();
    }

    /// Draws the ring and its labels into the surface.
    fn compose(&mut self) {
        let size = self.surface.size;
        let mut shades = [Shade::Empty; SLOTS];
        for (slot, shade) in shades.iter_mut().enumerate() {
            if self.labels[slot].is_some() {
                *shade = if self.hover == Some(slot) {
                    Shade::Hover
                } else {
                    Shade::Bound
                };
            }
        }
        let palette = Palette {
            bound: BACKGROUND,
            hover: accent_rgb(),
        };
        let rendered = ring::render(size, &shades, palette);
        self.surface.pixels().copy_from_slice(&rendered);
        self.draw_labels();
        // GDI text writes colour but clears alpha. Put the ring's alpha back
        // and keep every channel within it, as premultiplied alpha requires.
        for (pixel, drawn) in self.surface.pixels().iter_mut().zip(&rendered) {
            let alpha = drawn >> 24;
            let colour = *pixel;
            let channel = |shift: u32| (colour >> shift & 0xFF).min(alpha) << shift;
            *pixel = alpha << 24 | channel(16) | channel(8) | channel(0);
        }
    }

    /// Pushes the surface to the screen.
    fn present(&self) {
        let size = self.surface.size;
        let size_px = SIZE { cx: size, cy: size };
        let source = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: the window and the surface DC are live; the structures
        // outlive the call.
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                std::ptr::null_mut(),
                &raw const self.origin,
                &raw const size_px,
                self.surface.dc,
                &raw const source,
                0,
                &raw const blend,
                ULW_ALPHA,
            );
        }
    }

    fn draw_labels(&self) {
        let size = self.surface.size;
        let half = size / 2;
        // A label may use about a third of the ring's width and wraps onto
        // up to three lines.
        let width = size * 33 / 100;
        // SAFETY: the DC and font are owned by the surface; each text buffer
        // is null-terminated and outlives its DrawTextW calls; the previous
        // font is selected back before returning.
        unsafe {
            let previous = SelectObject(self.surface.dc, self.surface.font);
            SetBkMode(self.surface.dc, TRANSPARENT as i32);
            SetTextColor(self.surface.dc, LABEL);
            for (slot, label) in self.labels.iter().enumerate() {
                let Some(label) = label else {
                    continue;
                };
                let mut text: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                let (dx, dy) = ring::label_centre(slot, size);
                let format = DT_CENTER | DT_WORDBREAK | DT_NOPREFIX;
                let mut measured = RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: 0,
                };
                DrawTextW(
                    self.surface.dc,
                    text.as_mut_ptr(),
                    -1,
                    &raw mut measured,
                    format | DT_CALCRECT,
                );
                let height = measured.bottom.min(size * 28 / 100);
                let mut rect = RECT {
                    left: half + dx - width / 2,
                    top: half + dy - height / 2,
                    right: half + dx + width / 2,
                    bottom: half + dy + height / 2,
                };
                DrawTextW(
                    self.surface.dc,
                    text.as_mut_ptr(),
                    -1,
                    &raw mut rect,
                    format,
                );
            }
            SelectObject(self.surface.dc, previous);
        }
    }
}

/// The Windows accent colour as `0xRRGGBB`.
fn accent_rgb() -> u32 {
    // The frame module reports a COLORREF (0x00BBGGRR).
    let [red, green, blue, _] = super::super::frame::accent_color().to_le_bytes();
    u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
}

/// What a poll or a click decided.
enum Outcome {
    Stay,
    Close,
    Choose(usize),
}

fn poll() -> Outcome {
    RING.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(ring) = slot.as_mut() else {
            return Outcome::Stay;
        };
        if ring.opened.elapsed() >= TIMEOUT {
            return Outcome::Close;
        }
        let mut cursor = POINT::default();
        // SAFETY: cursor is a writable POINT.
        if unsafe { GetCursorPos(&raw mut cursor) } == 0 {
            return Outcome::Close;
        }
        let under = ring.slot_under(cursor);
        let left = key_down(VK_LBUTTON);
        let dismiss = dismiss_down();
        let (was_left, was_dismiss) = (ring.left, ring.dismiss);
        ring.left = left;
        ring.dismiss = dismiss;
        if dismiss && !was_dismiss {
            return Outcome::Close;
        }
        if left && !was_left {
            // A click on a bound slot chooses it; anywhere else it cancels.
            return under.map_or(Outcome::Close, Outcome::Choose);
        }
        if under != ring.hover {
            ring.hover = under;
            ring.paint();
        }
        Outcome::Stay
    })
}

/// A click the ring window itself received, in client coordinates. A fast
/// click can fall between two polls, so it is handled here as well.
fn click(lparam: LPARAM) -> Outcome {
    RING.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(ring) = slot.as_mut() else {
            return Outcome::Stay;
        };
        // The poll must not see this same press as a new edge.
        ring.left = true;
        let x = i32::from(lparam as u16 as i16);
        let y = i32::from((lparam >> 16) as u16 as i16);
        let point = POINT {
            x: ring.origin.x + x,
            y: ring.origin.y + y,
        };
        ring.slot_under(point)
            .map_or(Outcome::Close, Outcome::Choose)
    })
}

fn act(outcome: Outcome) {
    match outcome {
        Outcome::Stay => {}
        Outcome::Close => close(),
        Outcome::Choose(slot) => {
            // Close first: the action must reach the application underneath.
            close();
            super::ring_chosen(slot);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TIMER if wparam == TIMER => {
            act(poll());
            0
        }
        WM_LBUTTONDOWN => {
            act(click(lparam));
            0
        }
        // Never take focus from the application the action is meant for.
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        // SAFETY: forwards the unmodified message for default handling.
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws the ring off screen, with no window, and writes it as a 24-bit
    /// BMP blended over mid-grey to the path in `POWERKIT_RING_IMAGE`, for
    /// looking at the result. Nothing is shown on the desktop.
    #[test]
    #[ignore = "writes an image file named by POWERKIT_RING_IMAGE"]
    fn probe_writes_the_ring_image() {
        let Ok(path) = std::env::var("POWERKIT_RING_IMAGE") else {
            return;
        };
        let size = ring::DIAMETER_96 * 144 / 96;
        let labels = [
            Some("Kopyala".to_string()),
            Some("Yapıştır".to_string()),
            Some("Sonraki sanal masaüstü".to_string()),
            None,
            Some("Ctrl+Shift+M".to_string()),
            Some("İş akışı".to_string()),
            None,
            Some("Easy-Switch 2".to_string()),
        ];
        let mut ring = Open {
            hwnd: std::ptr::null_mut(),
            surface: Surface::new(size, 144).expect("surface"),
            origin: POINT::default(),
            labels,
            hover: Some(1),
            opened: Instant::now(),
            left: false,
            dismiss: false,
        };
        ring.compose();
        let side = usize::try_from(size).unwrap();
        let row = (side * 3).next_multiple_of(4);
        let mut file = Vec::with_capacity(54 + row * side);
        let total = u32::try_from(54 + row * side).unwrap();
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&total.to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&54u32.to_le_bytes());
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&size.to_le_bytes());
        // Negative height: rows are stored top first, as in the surface.
        file.extend_from_slice(&(-size).to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&24u16.to_le_bytes());
        file.extend_from_slice(&[0; 24]);
        for line in ring.surface.pixels().chunks(side) {
            let start = file.len();
            for pixel in line {
                let alpha = pixel >> 24;
                for shift in [0, 8, 16] {
                    // Premultiplied source over a 0x80 grey background.
                    let over = (pixel >> shift & 0xFF) + 0x80 * (255 - alpha) / 255;
                    file.push(u8::try_from(over.min(255)).unwrap());
                }
            }
            file.resize(start + row, 0);
        }
        std::fs::write(path, file).expect("write image");
    }
}
