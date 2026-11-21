//! Taskbar theme for the tray popup; app light/dark colors for Win32 overlays.
use crate::settings::AppTheme;
use std::{
    mem::size_of,
    ptr::{null_mut, without_provenance},
    sync::OnceLock,
};
use windows::UI::ViewManagement::{UIColorType, UISettings};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::{
        Dwm::{
            DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUNDSMALL,
            DwmSetWindowAttribute,
        },
        Gdi::{
            COLOR_3DFACE, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_MENUTEXT, COLOR_WINDOW,
            COLOR_WINDOWTEXT, GetSysColor,
        },
    },
    System::{
        LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
        Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        Threading::GetCurrentThreadId,
    },
    UI::WindowsAndMessaging::{
        CallNextHookEx, GetClassNameW, HCBT_CREATEWND, HHOOK, SPI_GETHIGHCONTRAST,
        SetWindowsHookExW, SystemParametersInfoW, UnhookWindowsHookEx, WH_CBT,
    },
};
const FORCE_DARK: i32 = 2;
const FORCE_LIGHT: i32 = 3;
const HCF_HIGHCONTRASTON: u32 = 1;
#[repr(C)]
struct HighContrast {
    cb_size: u32,
    dw_flags: u32,
    default_scheme: *mut u16,
}
const MENU_CLASS: [u16; 6] = [
    b'#' as u16,
    b'3' as u16,
    b'2' as u16,
    b'7' as u16,
    b'6' as u16,
    b'8' as u16,
];
type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
type FlushMenuThemes = unsafe extern "system" fn();
struct ThemeApi {
    set_preferred_app_mode: Option<SetPreferredAppMode>,
    flush_menu_themes: Option<FlushMenuThemes>,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn personalize_light(name: &str) -> Option<bool> {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let key = wide(name);
    // SAFETY: stack DWORD and NUL-terminated registry path; no handle ownership.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            key.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut value as *mut u32 as *mut _,
            &mut size,
        )
    };
    if status == 0 { Some(value != 0) } else { None }
}
pub(super) fn taskbar_uses_light_theme() -> Option<bool> {
    personalize_light("SystemUsesLightTheme")
}
pub(super) fn apps_use_light_theme() -> Option<bool> {
    personalize_light("AppsUseLightTheme")
}
pub(super) fn high_contrast() -> bool {
    let mut info = HighContrast {
        cb_size: size_of::<HighContrast>() as u32,
        dw_flags: 0,
        default_scheme: null_mut(),
    };
    // SAFETY: cb_size matches this HIGHCONTRASTW layout; Windows writes flags only.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cb_size,
            &mut info as *mut HighContrast as *mut _,
            0,
        )
    };
    ok != 0 && info.dw_flags & HCF_HIGHCONTRASTON != 0
}
fn popup_app_mode(light_taskbar: Option<bool>, high_contrast: bool) -> Option<i32> {
    if high_contrast {
        None
    } else if light_taskbar == Some(false) {
        Some(FORCE_DARK)
    } else {
        Some(FORCE_LIGHT)
    }
}
/// COLORREF for glyphs drawn beside tray menu text, matching the popup's mode.
pub(super) fn popup_text_color() -> u32 {
    match popup_app_mode(taskbar_uses_light_theme(), high_contrast()) {
        None => sys_color(COLOR_MENUTEXT),
        Some(FORCE_DARK) => rgb(0xFF, 0xFF, 0xFF),
        Some(_) => rgb(0x1A, 0x1A, 0x1A),
    }
}
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}
fn sys_color(index: i32) -> u32 {
    // SAFETY: GetSysColor reads a system color index; no handle ownership.
    unsafe { GetSysColor(index) }
}
fn accent_rgb() -> u32 {
    if let Ok(settings) = windows::UI::ViewManagement::UISettings::new()
        && let Ok(color) = settings.GetColorValue(windows::UI::ViewManagement::UIColorType::Accent)
    {
        return rgb(color.R, color.G, color.B);
    }
    sys_color(COLOR_HIGHLIGHT)
}
/// Fluent `AccentFillColorDefault` for the WinUI shell (AccentLight2 on dark, AccentDark1
/// on light); `None` under high contrast so WinUI keeps system colors.
pub(super) fn shell_accent(theme: AppTheme) -> Option<u32> {
    if high_contrast() {
        return None;
    }
    let dark = match theme {
        AppTheme::Dark => true,
        AppTheme::Light => false,
        AppTheme::System => apps_use_light_theme() == Some(false),
    };
    let kind = if dark {
        UIColorType::AccentLight2
    } else {
        UIColorType::AccentDark1
    };
    let color = UISettings::new().ok()?.GetColorValue(kind).ok()?;
    Some(rgb(color.R, color.G, color.B))
}
fn mix(base: u32, tint: u32, tint_weight: u8) -> u32 {
    let w = u32::from(tint_weight);
    let inv = 255 - w;
    let channel = |shift: u32| {
        let a = (base >> shift) & 0xff;
        let b = (tint >> shift) & 0xff;
        ((a * inv + b * w) / 255) & 0xff
    };
    channel(0) | (channel(8) << 8) | (channel(16) << 16)
}
/// Fluent / Windows Settings surfaces. High contrast uses system colors only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WindowTheme {
    pub light: bool,
    pub high_contrast: bool,
    pub background: u32,
    pub pane: u32,
    pub card: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub accent_soft: u32,
    pub hover: u32,
    pub selected: u32,
    pub stroke: u32,
    pub button: u32,
    pub button_hot: u32,
    pub toggle: u32,
    pub thumb: u32,
}
pub(super) fn window_theme() -> WindowTheme {
    window_theme_from(apps_use_light_theme(), high_contrast(), accent_rgb())
}
fn window_theme_from(apps_light: Option<bool>, high_contrast: bool, accent: u32) -> WindowTheme {
    if high_contrast {
        return WindowTheme {
            light: sys_color(COLOR_WINDOW) & 0xff > 0x80,
            high_contrast: true,
            background: sys_color(COLOR_WINDOW),
            pane: sys_color(COLOR_3DFACE),
            card: sys_color(COLOR_WINDOW),
            text: sys_color(COLOR_WINDOWTEXT),
            muted: sys_color(COLOR_GRAYTEXT),
            accent: sys_color(COLOR_HIGHLIGHT),
            accent_soft: sys_color(COLOR_HIGHLIGHT),
            hover: sys_color(COLOR_HIGHLIGHT),
            selected: sys_color(COLOR_HIGHLIGHT),
            stroke: sys_color(COLOR_WINDOWTEXT),
            button: sys_color(COLOR_3DFACE),
            button_hot: sys_color(COLOR_HIGHLIGHT),
            toggle: sys_color(COLOR_3DFACE),
            thumb: sys_color(COLOR_WINDOW),
        };
    }
    let light = apps_light != Some(false);
    if light {
        let card = rgb(0xFF, 0xFF, 0xFF);
        let pane = rgb(0xF3, 0xF3, 0xF3);
        WindowTheme {
            light: true,
            high_contrast: false,
            background: rgb(0xF3, 0xF3, 0xF3),
            pane,
            card,
            text: rgb(0x1A, 0x1A, 0x1A),
            muted: rgb(0x5C, 0x5C, 0x5C),
            accent,
            accent_soft: mix(pane, accent, 28),
            hover: rgb(0xF0, 0xF0, 0xF0),
            selected: mix(pane, accent, 36),
            stroke: rgb(0xE0, 0xE0, 0xE0),
            button: card,
            button_hot: rgb(0xF5, 0xF5, 0xF5),
            toggle: rgb(0x8A, 0x8A, 0x8A),
            thumb: card,
        }
    } else {
        let card = rgb(0x2C, 0x2C, 0x2C);
        let pane = rgb(0x20, 0x20, 0x20);
        WindowTheme {
            light: false,
            high_contrast: false,
            background: pane,
            pane,
            card,
            text: rgb(0xFF, 0xFF, 0xFF),
            muted: rgb(0xC8, 0xC8, 0xC8),
            accent,
            accent_soft: mix(pane, accent, 48),
            hover: rgb(0x2A, 0x2A, 0x2A),
            selected: mix(pane, accent, 56),
            stroke: rgb(0x3F, 0x3F, 0x3F),
            button: rgb(0x33, 0x33, 0x33),
            button_hot: rgb(0x3A, 0x3A, 0x3A),
            toggle: rgb(0x6B, 0x6B, 0x6B),
            thumb: rgb(0xFF, 0xFF, 0xFF),
        }
    }
}
fn theme_api() -> &'static ThemeApi {
    static API: OnceLock<ThemeApi> = OnceLock::new();
    API.get_or_init(|| {
        let path = wide("uxtheme.dll");
        // SAFETY: system32-only load of a Windows inbox DLL; the module stays for process life.
        let module = unsafe {
            LoadLibraryExW(path.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32)
        };
        if module.is_null() {
            return ThemeApi {
                set_preferred_app_mode: None,
                flush_menu_themes: None,
            };
        }
        ThemeApi {
            set_preferred_app_mode: unsafe { GetProcAddress(module, without_provenance(135)) }.map(
                |function| unsafe {
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, SetPreferredAppMode>(
                        function,
                    )
                },
            ),
            flush_menu_themes: unsafe { GetProcAddress(module, without_provenance(136)) }.map(
                |function| unsafe {
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, FlushMenuThemes>(
                        function,
                    )
                },
            ),
        }
    })
}
fn apply_preferred_app_mode() {
    let Some(mode) = popup_app_mode(taskbar_uses_light_theme(), high_contrast()) else {
        return;
    };
    let api = theme_api();
    if let Some(set_preferred_app_mode) = api.set_preferred_app_mode {
        // SAFETY: uxtheme ordinal 135 is SetPreferredAppMode on Windows 10 1903+ / Windows 11.
        unsafe {
            set_preferred_app_mode(mode);
        }
    }
    if let Some(flush_menu_themes) = api.flush_menu_themes {
        // SAFETY: ordinal 136 flushes cached menu colors after a preferred-mode change.
        unsafe {
            flush_menu_themes();
        }
    }
}
fn is_menu_class(name: &[u16]) -> bool {
    name == MENU_CLASS
}
fn popup_menu_window(hwnd: HWND) -> bool {
    let mut class = [0u16; 8];
    // SAFETY: hwnd is the CBT CREATEWND window; the buffer is a stack array.
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    len > 0 && is_menu_class(&class[..len as usize])
}
fn apply_popup_chrome(hwnd: HWND) {
    let preference = DWMWCP_ROUNDSMALL;
    // SAFETY: hwnd is the live #32768 popup; DWM attributes are hints and may no-op.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const _ as *const _,
            size_of::<i32>() as u32,
        );
    }
    let dark =
        u32::from(popup_app_mode(taskbar_uses_light_theme(), high_contrast()) == Some(FORCE_DARK));
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark as *const u32 as *const _,
            size_of::<u32>() as u32,
        );
    }
}
unsafe extern "system" fn popup_cbt_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HCBT_CREATEWND as i32 {
        let hwnd = wparam as HWND;
        if popup_menu_window(hwnd) {
            apply_popup_chrome(hwnd);
        }
    }
    // SAFETY: thread hook installed around TrackPopupMenu; pass the event along.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}
pub(super) struct PopupChrome(Option<HHOOK>);
impl PopupChrome {
    fn install() -> Self {
        // SAFETY: WH_CBT on this UI thread only; the procedure lives for the process.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_CBT,
                Some(popup_cbt_hook),
                null_mut(),
                GetCurrentThreadId(),
            )
        };
        Self((!hook.is_null()).then_some(hook))
    }
}
impl Drop for PopupChrome {
    fn drop(&mut self) {
        if let Some(hook) = self.0.take() {
            // SAFETY: hook is the handle we installed on this thread, unhooked once.
            unsafe {
                UnhookWindowsHookEx(hook);
            }
        }
    }
}
pub(super) fn prepare_popup() -> PopupChrome {
    apply_preferred_app_mode();
    PopupChrome::install()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_follows_taskbar_unless_high_contrast() {
        assert_eq!(popup_app_mode(Some(false), false), Some(FORCE_DARK));
        assert_eq!(popup_app_mode(Some(true), false), Some(FORCE_LIGHT));
        assert_eq!(popup_app_mode(None, false), Some(FORCE_LIGHT));
        assert_eq!(popup_app_mode(Some(false), true), None);
        assert_eq!(popup_app_mode(Some(true), true), None);
    }
    #[test]
    fn settings_surfaces_stay_neutral_outside_high_contrast() {
        let accent = rgb(0, 0x78, 0xD4);
        let light = window_theme_from(Some(true), false, accent);
        assert!(light.light && !light.high_contrast);
        assert_eq!(light.background, rgb(0xF3, 0xF3, 0xF3));
        assert_eq!(light.card, rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(light.selected, mix(light.pane, accent, 36));
        assert_eq!(light.accent_soft, mix(light.pane, accent, 28));
        let dark = window_theme_from(Some(false), false, accent);
        assert!(!dark.light && dark.card != dark.background);
        assert_ne!(dark.button, rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(light.button, rgb(0xFF, 0xFF, 0xFF));
        assert_ne!(light.toggle, light.accent);
        assert_eq!(light.thumb, rgb(0xFF, 0xFF, 0xFF));
        assert_ne!(dark.selected, dark.pane);
        let contrast = window_theme_from(Some(false), true, rgb(0, 0, 0));
        assert!(contrast.high_contrast);
        assert_eq!(contrast.accent, sys_color(COLOR_HIGHLIGHT));
        assert_eq!(contrast.accent_soft, contrast.accent);
    }
    #[test]
    fn menu_window_class_is_win32_popup() {
        assert!(is_menu_class(&MENU_CLASS));
        assert!(!is_menu_class(&[b'#' as u16, b'3' as u16]));
    }
}
