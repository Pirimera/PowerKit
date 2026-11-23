//! Segoe Fluent glyphs rendered to 32-bit bitmaps for native tray menu items.
use super::wide;
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
        CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS,
        DT_CENTER, DT_NOCLIP, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
        DrawTextW, FW_NORMAL, GdiFlush, GetTextFaceW, HBITMAP, HDC, HFONT, HGDIOBJ,
        OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    },
    UI::{
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        WindowsAndMessaging::SM_CXSMICON,
    },
};
/// Windows 11 icon font, then the Windows 10 one that shares its code points.
const ICON_FACES: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];
const WHITE: u32 = 0x00FF_FFFF;
/// Transparent height added to each item bitmap, in eighths of the icon side
/// (6 px at 100%), to space the menu rows out.
const ROW_PADDING_EIGHTHS: i32 = 3;

/// Bitmaps for one popup. Menus do not own `hbmpItem` bitmaps, so this must
/// outlive `TrackPopupMenu` and is dropped after `DestroyMenu`.
pub struct MenuIcons {
    side: i32,
    height: i32,
    color: u32,
    blank: Option<HBITMAP>,
    bitmaps: Vec<HBITMAP>,
}
impl MenuIcons {
    /// `color` is the menu text COLORREF; the side is the small-icon metric at the
    /// app window's DPI.
    pub fn new(hwnd: HWND, color: u32) -> Self {
        // SAFETY: hwnd is the live app window on this UI thread; both calls only read.
        let side = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(hwnd)) };
        let side = side.max(16);
        Self {
            side,
            height: side + side * ROW_PADDING_EIGHTHS / 8,
            color,
            blank: None,
            bitmaps: Vec::new(),
        }
    }
    /// Bitmap for one menu item: its glyph, or a transparent one of the same size
    /// for an item without an icon (or when no icon font is installed). A menu row
    /// is as tall as its bitmap, so every item gets one to keep rows evenly spaced.
    /// `None` only when GDI fails; the item then keeps the default height.
    pub fn bitmap(&mut self, glyph: Option<&str>) -> Option<HBITMAP> {
        if let Some(glyph) = glyph
            && let Some(bitmap) = render(Some(glyph), self.side, self.height, self.color)
        {
            self.bitmaps.push(bitmap);
            return Some(bitmap);
        }
        if self.blank.is_none() {
            self.blank = render(None, self.side, self.height, self.color);
            self.bitmaps.extend(self.blank);
        }
        self.blank
    }
}
impl Drop for MenuIcons {
    fn drop(&mut self) {
        for bitmap in self.bitmaps.drain(..) {
            // SAFETY: each handle was created by `render`, is owned only here and is
            // no longer selected into any DC.
            unsafe {
                DeleteObject(bitmap);
            }
        }
    }
}
/// Premultiplied BGRA pixel for a coverage value and a COLORREF (0x00BBGGRR).
fn premultiplied(coverage: u8, color: u32) -> u32 {
    let alpha = u32::from(coverage);
    let channel = |shift: u32| ((color >> shift) & 0xff) * alpha / 255;
    (alpha << 24) | (channel(0) << 16) | (channel(8) << 8) | channel(16)
}
/// Selects the first installed icon font into `dc`. GDI silently substitutes a
/// missing face, so the selected face name is compared with the requested one.
/// Returns the font to delete after it is deselected.
unsafe fn select_icon_font(dc: HDC, side: i32) -> Option<(HFONT, HGDIOBJ)> {
    for face in ICON_FACES {
        let name = wide(face);
        // SAFETY: name is NUL-terminated and outlives the call.
        let font = unsafe {
            CreateFontW(
                -side,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_DEFAULT_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(ANTIALIASED_QUALITY),
                0,
                name.as_ptr(),
            )
        };
        if font.is_null() {
            continue;
        }
        let mut selected = [0u16; 64];
        // SAFETY: dc and font are live; the buffer length is passed with it.
        let (previous, len) = unsafe {
            let previous = SelectObject(dc, font);
            let len = GetTextFaceW(dc, selected.len() as i32, selected.as_mut_ptr());
            (previous, len)
        };
        // The length counts the terminating NUL.
        let matches = len > 1 && selected[..len as usize - 1] == name[..name.len() - 1];
        if matches {
            return Some((font, previous));
        }
        // SAFETY: restore the DC before deleting the font we created.
        unsafe {
            SelectObject(dc, previous);
            DeleteObject(font);
        }
    }
    None
}
/// A `side` x `height` bitmap with `glyph` centered in it, or fully transparent
/// for `None`. `None` is returned when GDI fails or no icon font has the glyph.
fn render(glyph: Option<&str>, side: i32, height: i32, color: u32) -> Option<HBITMAP> {
    let header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: side,
        // Negative height: top-down rows.
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..Default::default()
    };
    let info = BITMAPINFO {
        bmiHeader: header,
        ..Default::default()
    };
    // SAFETY: a memory DC, DIB section and font created, used and released on this
    // thread. `bits` points at side*height zeroed u32 pixels owned by the DIB, which
    // stays alive until the caller deletes the returned bitmap. Every selected
    // object is deselected before it or the DC is deleted.
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() {
            return None;
        }
        let mut bits: *mut core::ffi::c_void = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            DeleteDC(dc);
            return None;
        }
        let Some(glyph) = glyph else {
            DeleteDC(dc);
            return Some(bitmap);
        };
        let text = wide(glyph);
        let previous_bitmap = SelectObject(dc, bitmap);
        let Some((font, previous_font)) = select_icon_font(dc, side) else {
            SelectObject(dc, previous_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
            return None;
        };
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, WHITE);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: side,
            bottom: height,
        };
        // White on the zeroed (black) surface: each channel is the glyph coverage.
        DrawTextW(
            dc,
            text.as_ptr(),
            -1,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP,
        );
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), (side * height) as usize);
        for pixel in pixels {
            *pixel = premultiplied(((*pixel >> 8) & 0xff) as u8, color);
        }
        SelectObject(dc, previous_font);
        DeleteObject(font);
        SelectObject(dc, previous_bitmap);
        DeleteDC(dc);
        Some(bitmap)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pixels_are_premultiplied_bgra_from_a_colorref() {
        // COLORREF 0x00BBGGRR: blue 0x20, green 0x80, red 0xFF.
        let color = 0x0020_80FF;
        assert_eq!(premultiplied(0, color), 0);
        assert_eq!(premultiplied(255, color), 0xFFFF_8020);
        let half = premultiplied(128, color);
        assert_eq!(half >> 24, 128);
        assert_eq!((half >> 16) & 0xff, 128);
        assert_eq!((half >> 8) & 0xff, 64);
        assert_eq!(half & 0xff, 16);
    }
    #[test]
    fn a_rendered_glyph_has_ink_in_the_requested_color() {
        // Power glyph. Only a machine with neither icon font may skip.
        let fonts = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("Fonts");
        let installed = ["SegoeIcons.ttf", "segmdl2.ttf"]
            .iter()
            .any(|file| fonts.join(file).exists());
        let Some(bitmap) = render(Some("\u{E7E8}"), 16, 22, WHITE) else {
            assert!(
                !installed,
                "an icon font is installed but no glyph rendered"
            );
            return;
        };
        let mut dib = windows_sys::Win32::Graphics::Gdi::DIBSECTION::default();
        // SAFETY: bitmap is the live DIB section just created; the struct size is passed.
        let pixels = unsafe {
            windows_sys::Win32::Graphics::Gdi::GetObjectW(
                bitmap,
                size_of::<windows_sys::Win32::Graphics::Gdi::DIBSECTION>() as i32,
                (&mut dib as *mut windows_sys::Win32::Graphics::Gdi::DIBSECTION).cast(),
            );
            std::slice::from_raw_parts(dib.dsBm.bmBits.cast::<u32>(), 16 * 22).to_vec()
        };
        // SAFETY: the test owns the bitmap.
        unsafe {
            DeleteObject(bitmap);
        }
        let inked = pixels.iter().filter(|pixel| *pixel >> 24 > 0).count();
        assert!(inked > 8, "{inked} inked pixels");
        assert!(pixels.iter().any(|pixel| *pixel >> 24 > 128));
        // Premultiplied: no channel exceeds its alpha.
        assert!(pixels.iter().all(|pixel| {
            let alpha = pixel >> 24;
            (pixel & 0xff) <= alpha && ((pixel >> 8) & 0xff) <= alpha
        }));
    }
}
