//! Tray popup colors and Windows 11 menu chrome follow the taskbar theme.
use std::{
    mem::size_of,
    ptr::{null_mut, without_provenance},
    sync::OnceLock,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Dwm::{
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUNDSMALL,
        DwmSetWindowAttribute,
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
struct MenuThemeApi {
    set_preferred_app_mode: Option<SetPreferredAppMode>,
    flush_menu_themes: Option<FlushMenuThemes>,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub(super) fn taskbar_uses_light_theme() -> Option<bool> {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = wide("SystemUsesLightTheme");
    // SAFETY: stack DWORD and NUL-terminated registry path; no handle ownership.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut value as *mut u32 as *mut _,
            &mut size,
        )
    };
    if status == 0 { Some(value != 0) } else { None }
}
fn high_contrast() -> bool {
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
fn menu_theme_api() -> &'static MenuThemeApi {
    static API: OnceLock<MenuThemeApi> = OnceLock::new();
    API.get_or_init(|| {
        let path = wide("uxtheme.dll");
        // SAFETY: system32-only load of a Windows inbox DLL; the module stays for process life.
        let module = unsafe {
            LoadLibraryExW(path.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32)
        };
        if module.is_null() {
            return MenuThemeApi {
                set_preferred_app_mode: None,
                flush_menu_themes: None,
            };
        }
        MenuThemeApi {
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
    let api = menu_theme_api();
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
    fn menu_window_class_is_win32_popup() {
        assert!(is_menu_class(&MENU_CLASS));
        assert!(!is_menu_class(&[b'#' as u16, b'3' as u16]));
    }
}
