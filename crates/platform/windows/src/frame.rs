//! PowerToys FrameDrawer port: D2D hollow rounded ring on the AlwaysTop border HWND.
use std::cell::RefCell;
use windows::{
    Win32::{
        Foundation::{D2DERR_RECREATE_TARGET, HWND as D2D_HWND, RECT as D2D_RECT},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
                    D2D1_FILL_MODE_ALTERNATE, D2D1_PIXEL_FORMAT,
                },
                D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_FACTORY_TYPE_MULTI_THREADED,
                D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
                D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_ROUNDED_RECT,
                D2D1CreateFactory, ID2D1Factory, ID2D1Geometry, ID2D1HwndRenderTarget,
                ID2D1SolidColorBrush,
            },
            Dxgi::Common::DXGI_FORMAT_UNKNOWN,
        },
    },
    core::Interface,
};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Dwm::{
        DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_ROUND,
        DWMWCP_ROUNDSMALL, DwmGetColorizationColor, DwmGetWindowAttribute,
    },
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::GetClientRect},
};
thread_local! {
    static FACTORY: RefCell<Option<ID2D1Factory>> = const { RefCell::new(None) };
}
pub struct FrameDrawer {
    window: HWND,
    target: Option<ID2D1HwndRenderTarget>,
    brush: Option<ID2D1SolidColorBrush>,
    size: D2D_SIZE_U,
    /// What the window currently shows; `None` after the target was recreated or the ring was shown again.
    painted: Option<PaintKey>,
}
/// Every input that changes the ring's pixels. Equal keys draw the same frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PaintKey {
    width: i32,
    height: i32,
    rgb: u32,
    alpha: u32,
    thickness: i32,
    radius: u32,
}
impl PaintKey {
    fn new(frame: RECT, rgb: u32, alpha: f32, thickness: i32, radius: f32) -> Self {
        Self {
            width: frame.right - frame.left,
            height: frame.bottom - frame.top,
            rgb,
            alpha: alpha.to_bits(),
            thickness,
            radius: radius.to_bits(),
        }
    }
}
/// Whether a paint with `next` must reach D2D, given what the window last showed.
fn needs_paint(last: Option<PaintKey>, next: PaintKey) -> bool {
    last != Some(next)
}
impl FrameDrawer {
    pub fn create(window: HWND) -> Option<Self> {
        let mut drawer = Self {
            window,
            target: None,
            brush: None,
            size: D2D_SIZE_U::default(),
            painted: None,
        };
        if drawer.ensure_targets(pixel_size(window)?) {
            Some(drawer)
        } else {
            None
        }
    }
    pub fn paint(&mut self, frame: RECT, rgb: u32, alpha: f32, thickness: i32, radius: f32) {
        let Some(size) = pixel_size(self.window).or_else(|| Some(rect_size(frame))) else {
            return;
        };
        if !self.ensure_targets(size) {
            return;
        }
        let key = PaintKey::new(frame, rgb, alpha, thickness, radius);
        if !needs_paint(self.painted, key) {
            return;
        }
        let color = convert_color(rgb, alpha);
        if let Some(brush) = &self.brush {
            unsafe {
                brush.SetColor(&color);
            }
        }
        self.render(scene(frame, color, thickness, radius));
        self.painted = Some(key);
    }
    /// Forces the next paint to reach D2D, for a ring that was hidden and shown again.
    pub fn invalidate(&mut self) {
        self.painted = None;
    }
    fn ensure_targets(&mut self, size: D2D_SIZE_U) -> bool {
        if self.target.is_some() && self.size.width == size.width && self.size.height == size.height
        {
            return true;
        }
        // Resizing or recreating the target clears what the window showed.
        self.painted = None;
        if let Some(target) = &self.target
            && unsafe { target.Resize(&size) }.is_ok()
        {
            self.size = size;
            return true;
        }
        self.target = None;
        self.brush = None;
        let Some(factory) = factory() else {
            return false;
        };
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_UNKNOWN,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd: D2D_HWND(self.window),
            pixelSize: size,
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        // SAFETY: hwnd is our live layered border window; factory is created on this UI thread.
        let target = unsafe { factory.CreateHwndRenderTarget(&props, &hwnd_props) }.ok();
        let Some(target) = target else {
            return false;
        };
        unsafe {
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
        let brush = unsafe { target.CreateSolidColorBrush(&D2D1_COLOR_F::default(), None) }.ok();
        let Some(brush) = brush else {
            return false;
        };
        self.target = Some(target);
        self.brush = Some(brush);
        self.size = size;
        true
    }
    fn render(&mut self, scene: Scene) {
        let (Some(target), Some(brush)) = (&self.target, &self.brush) else {
            return;
        };
        let Some(factory) = factory() else {
            return;
        };
        unsafe {
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F::default()));
        }
        if let Some(geometry) = border_geometry(&factory, &scene) {
            unsafe {
                target.FillGeometry(&geometry, brush, None);
            }
        }
        match unsafe { target.EndDraw(None, None) } {
            Err(error) if error.code() == D2DERR_RECREATE_TARGET => {
                self.target = None;
                self.brush = None;
                self.size = D2D_SIZE_U::default();
            }
            _ => {}
        }
    }
}
#[derive(Clone, Copy)]
struct Scene {
    rect: Option<D2D_RECT_F>,
    rounded: Option<D2D1_ROUNDED_RECT>,
    thickness: i32,
}
fn scene(frame: RECT, _color: D2D1_COLOR_F, thickness: i32, radius: f32) -> Scene {
    if radius != 0.0 {
        Scene {
            rect: None,
            rounded: Some(convert_rounded(frame, thickness, radius)),
            thickness,
        }
    } else {
        Scene {
            rect: Some(convert_rect(frame, thickness)),
            rounded: None,
            thickness,
        }
    }
}
/// Releases the cached factory on the UI thread during shutdown, after every ring
/// is gone. Left to the thread-local destructor at process exit, the release
/// raised exception 0x87A in KERNELBASE after a session that had drawn a ring.
pub fn release_factory() {
    let factory = FACTORY.with(|slot| slot.borrow_mut().take());
    drop(factory);
}
fn factory() -> Option<ID2D1Factory> {
    FACTORY.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = unsafe {
                D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_MULTI_THREADED, None)
            }
            .ok();
        }
        slot.clone()
    })
}
fn pixel_size(hwnd: HWND) -> Option<D2D_SIZE_U> {
    let mut rect = D2D_RECT::default();
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&raw mut rect).cast(),
            std::mem::size_of::<D2D_RECT>() as u32,
        )
    } == 0
    {
        return Some(rect_size(RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }));
    }
    let mut client = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client) } != 0 {
        return Some(rect_size(client));
    }
    None
}
fn rect_size(rect: RECT) -> D2D_SIZE_U {
    D2D_SIZE_U {
        width: (rect.right - rect.left).max(0) as u32,
        height: (rect.bottom - rect.top).max(0) as u32,
    }
}
fn convert_color(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: (rgb & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: ((rgb >> 16) & 0xff) as f32 / 255.0,
        a: alpha,
    }
}
pub fn convert_rect(rect: RECT, thickness: i32) -> D2D_RECT_F {
    let half = thickness as f32 / 2.0;
    D2D_RECT_F {
        left: rect.left as f32 + half + 1.0,
        top: rect.top as f32 + half + 1.0,
        right: rect.right as f32 - half - 1.0,
        bottom: rect.bottom as f32 - half - 1.0,
    }
}
pub fn convert_rounded(rect: RECT, thickness: i32, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: convert_rect(rect, thickness),
        radiusX: radius,
        radiusY: radius,
    }
}
fn border_geometry(factory: &ID2D1Factory, scene: &Scene) -> Option<ID2D1Geometry> {
    let half = scene.thickness as f32 / 2.0;
    let (outer, inner) = if let Some(border) = scene.rounded {
        let outer_rect = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: border.rect.left - half,
                top: border.rect.top - half,
                right: border.rect.right + half,
                bottom: border.rect.bottom + half,
            },
            radiusX: border.radiusX + half,
            radiusY: border.radiusY + half,
        };
        let inner_rect = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: border.rect.left + half,
                top: border.rect.top + half,
                right: border.rect.right - half,
                bottom: border.rect.bottom - half,
            },
            radiusX: (border.radiusX - half).max(0.0),
            radiusY: (border.radiusY - half).max(0.0),
        };
        let outer = unsafe { factory.CreateRoundedRectangleGeometry(&outer_rect) }
            .ok()?
            .cast::<ID2D1Geometry>()
            .ok()?;
        if inner_rect.rect.left >= inner_rect.rect.right
            || inner_rect.rect.top >= inner_rect.rect.bottom
        {
            return Some(outer);
        }
        let inner = unsafe { factory.CreateRoundedRectangleGeometry(&inner_rect) }
            .ok()?
            .cast::<ID2D1Geometry>()
            .ok()?;
        (outer, inner)
    } else {
        let border = scene.rect?;
        let outer_rect = D2D_RECT_F {
            left: border.left - half,
            top: border.top - half,
            right: border.right + half,
            bottom: border.bottom + half,
        };
        let inner_rect = D2D_RECT_F {
            left: border.left + half,
            top: border.top + half,
            right: border.right - half,
            bottom: border.bottom - half,
        };
        let outer = unsafe { factory.CreateRectangleGeometry(&outer_rect) }
            .ok()?
            .cast::<ID2D1Geometry>()
            .ok()?;
        if inner_rect.left >= inner_rect.right || inner_rect.top >= inner_rect.bottom {
            return Some(outer);
        }
        let inner = unsafe { factory.CreateRectangleGeometry(&inner_rect) }
            .ok()?
            .cast::<ID2D1Geometry>()
            .ok()?;
        (outer, inner)
    };
    unsafe { factory.CreateGeometryGroup(D2D1_FILL_MODE_ALTERNATE, &[Some(outer), Some(inner)]) }
        .ok()?
        .cast()
        .ok()
}
pub fn scale_factor(hwnd: HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    dpi as f32 / 96.0
}
pub fn corners_radius(preference: i32) -> i32 {
    match preference {
        x if x == DWMWCP_ROUND || x == DWMWCP_DEFAULT => 8,
        x if x == DWMWCP_ROUNDSMALL => 4,
        _ => 0,
    }
}
pub fn corner_radius(hwnd: HWND) -> f32 {
    let mut preference = -1i32;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw mut preference).cast(),
            4,
        );
    }
    corners_radius(preference) as f32 * scale_factor(hwnd)
}
pub fn accent_color() -> u32 {
    if let Ok(settings) = windows::UI::ViewManagement::UISettings::new()
        && let Ok(color) = settings.GetColorValue(windows::UI::ViewManagement::UIColorType::Accent)
    {
        return u32::from(color.R) | (u32::from(color.G) << 8) | (u32::from(color.B) << 16);
    }
    let mut color = 0u32;
    let mut opaque = 0i32;
    if unsafe { DwmGetColorizationColor(&mut color, &mut opaque) } == 0 {
        let r = (color >> 16) & 0xff;
        let g = (color >> 8) & 0xff;
        let b = color & 0xff;
        return r | (g << 8) | (b << 16);
    }
    0x00d4_7800
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn powertoys_inset_leaves_a_one_pixel_gap_killer() {
        let inset = convert_rect(
            RECT {
                left: 0,
                top: 0,
                right: 200,
                bottom: 100,
            },
            4,
        );
        assert_eq!(inset.left, 3.0);
        assert_eq!(inset.top, 3.0);
        assert_eq!(inset.right, 197.0);
        assert_eq!(inset.bottom, 97.0);
    }
    #[test]
    fn corner_preference_matches_powertoys_radii() {
        assert_eq!(corners_radius(DWMWCP_DEFAULT), 8);
        assert_eq!(corners_radius(DWMWCP_ROUND), 8);
        assert_eq!(corners_radius(DWMWCP_ROUNDSMALL), 4);
        assert_eq!(corners_radius(-1), 0);
        assert_eq!(corners_radius(1), 0);
    }
    fn key(width: i32, alpha: f32) -> PaintKey {
        PaintKey::new(
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: 50,
            },
            0x00d4_7800,
            alpha,
            4,
            8.0,
        )
    }
    #[test]
    fn first_paint_and_any_changed_input_reach_d2d() {
        let first = key(200, 1.0);
        assert!(needs_paint(None, first));
        assert!(needs_paint(Some(first), key(201, 1.0)), "size");
        assert!(needs_paint(Some(first), key(200, 0.5)), "alpha");
        let recolored = PaintKey::new(
            RECT {
                left: 0,
                top: 0,
                right: 200,
                bottom: 50,
            },
            0x0000_ff00,
            1.0,
            4,
            8.0,
        );
        assert!(needs_paint(Some(first), recolored), "color");
        let thicker = PaintKey::new(
            RECT {
                left: 0,
                top: 0,
                right: 200,
                bottom: 50,
            },
            0x00d4_7800,
            1.0,
            6,
            8.0,
        );
        assert!(needs_paint(Some(first), thicker), "thickness");
    }
    #[test]
    fn identical_inputs_skip_the_repaint() {
        let first = key(200, 1.0);
        assert!(!needs_paint(Some(first), key(200, 1.0)));
    }
}
