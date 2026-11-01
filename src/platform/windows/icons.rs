//! Branded window and notify-area icons loaded from checked-in ICO bytes.
use crate::always_on::Mode;
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        WindowsAndMessaging::{
            CreateIconFromResourceEx, DestroyIcon, HICON, ICON_BIG, ICON_SMALL, IDI_APPLICATION,
            LR_DEFAULTCOLOR, LoadIconW, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON,
            SendMessageW, WM_SETICON,
        },
    },
};
const APP: &[u8] = include_bytes!("../../../brand/windows/PowerKit.ico");
const TRAY_LIGHT: &[u8] = include_bytes!("../../../brand/windows/tray-light.ico");
const TRAY_DARK: &[u8] = include_bytes!("../../../brand/windows/tray-dark.ico");
const TRAY_LIGHT_PAUSED: &[u8] = include_bytes!("../../../brand/windows/tray-light-paused.ico");
const TRAY_DARK_PAUSED: &[u8] = include_bytes!("../../../brand/windows/tray-dark-paused.ico");
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayAsset {
    Light,
    LightPaused,
    Dark,
    DarkPaused,
    App,
}
#[derive(Clone, Copy, Default)]
pub(super) struct IconSet {
    pub window_big: isize,
    pub window_small: isize,
    pub tray: isize,
}
fn tray_asset(light_taskbar: Option<bool>, paused: bool) -> TrayAsset {
    match (light_taskbar, paused) {
        (Some(true), false) => TrayAsset::Light,
        (Some(true), true) => TrayAsset::LightPaused,
        (Some(false), false) => TrayAsset::Dark,
        (Some(false), true) => TrayAsset::DarkPaused,
        (None, _) => TrayAsset::App,
    }
}
fn tray_bytes(light_taskbar: Option<bool>, paused: bool) -> &'static [u8] {
    match tray_asset(light_taskbar, paused) {
        TrayAsset::Light => TRAY_LIGHT,
        TrayAsset::LightPaused => TRAY_LIGHT_PAUSED,
        TrayAsset::Dark => TRAY_DARK,
        TrayAsset::DarkPaused => TRAY_DARK_PAUSED,
        TrayAsset::App => APP,
    }
}
pub(super) fn theme_setting(name: Option<&str>) -> bool {
    name.is_some_and(|n| n.eq_ignore_ascii_case("ImmersiveColorSet"))
}
pub(super) fn class_icon() -> HICON {
    // Owned for the registered window class lifetime; not released with IconSet.
    load(APP, 32, 32) as HICON
}
pub(super) fn apply_window(hwnd: HWND, previous: IconSet) -> IconSet {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let big = load(APP, metric(SM_CXICON, dpi), metric(SM_CYICON, dpi));
    let small = load(APP, metric(SM_CXSMICON, dpi), metric(SM_CYSMICON, dpi));
    // SAFETY: hwnd is the live UI window; icon handles are valid for WM_SETICON.
    unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small);
    }
    release(previous.window_big);
    release(previous.window_small);
    IconSet {
        window_big: big,
        window_small: small,
        tray: previous.tray,
    }
}
pub(super) fn apply_tray(hwnd: HWND, mode: Mode, previous: IconSet) -> (isize, IconSet) {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let icon = load(
        tray_bytes(super::theme::taskbar_uses_light_theme(), !mode.active()),
        metric(SM_CXSMICON, dpi),
        metric(SM_CYSMICON, dpi),
    );
    release(previous.tray);
    (
        icon,
        IconSet {
            window_big: previous.window_big,
            window_small: previous.window_small,
            tray: icon,
        },
    )
}
pub(super) fn release_all(icons: IconSet) {
    release(icons.window_big);
    release(icons.window_small);
    release(icons.tray);
}
pub(super) fn release_owned(icon: isize) {
    release(icon);
}
fn metric(index: i32, dpi: u32) -> i32 {
    let value = unsafe { GetSystemMetricsForDpi(index, dpi) };
    if value > 0 { value } else { 16 }
}
fn load(bytes: &[u8], cx: i32, cy: i32) -> isize {
    if let Some((offset, size)) = icon_entry(bytes, cx, cy) {
        // SAFETY: offset/size address one ICO image; the API copies pixels into a new HICON.
        let icon = unsafe {
            CreateIconFromResourceEx(
                bytes[offset..].as_ptr(),
                size,
                1,
                0x0003_0000,
                cx,
                cy,
                LR_DEFAULTCOLOR,
            )
        };
        if !icon.is_null() {
            return icon as isize;
        }
    }
    // SAFETY: IDI_APPLICATION is a process-shared stock icon; never destroyed.
    unsafe { LoadIconW(null_mut(), IDI_APPLICATION) as isize }
}
fn release(icon: isize) {
    if icon == 0 {
        return;
    }
    let handle = icon as HICON;
    // SAFETY: stock fallback must not be destroyed; CreateIconFromResourceEx handles are owned.
    let stock = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
    if !handle.is_null() && handle != stock {
        unsafe {
            DestroyIcon(handle);
        }
    }
}
fn icon_entry(bytes: &[u8], cx: i32, cy: i32) -> Option<(usize, u32)> {
    if bytes.len() < 6 {
        return None;
    }
    let reserved = u16::from_le_bytes(bytes[0..2].try_into().ok()?);
    let kind = u16::from_le_bytes(bytes[2..4].try_into().ok()?);
    let count = u16::from_le_bytes(bytes[4..6].try_into().ok()?);
    if reserved != 0 || kind != 1 || count == 0 {
        return None;
    }
    let want_x = cx.max(1) as u32;
    let want_y = cy.max(1) as u32;
    let mut best: Option<(u32, usize, u32)> = None;
    for index in 0..count as usize {
        let base = 6 + index * 16;
        if bytes.len() < base + 16 {
            return None;
        }
        let width = match bytes[base] {
            0 => 256,
            w => w as u32,
        };
        let height = match bytes[base + 1] {
            0 => 256,
            h => h as u32,
        };
        let size = u32::from_le_bytes(bytes[base + 8..base + 12].try_into().ok()?);
        let offset = u32::from_le_bytes(bytes[base + 12..base + 16].try_into().ok()?);
        let end = offset.checked_add(size)? as usize;
        if offset as usize >= bytes.len() || end > bytes.len() {
            continue;
        }
        let score = icon_score(width, height, want_x, want_y);
        if best.is_none_or(|(current, _, _)| score < current) {
            best = Some((score, offset as usize, size));
        }
    }
    best.map(|(_, offset, size)| (offset, size))
}
fn icon_score(width: u32, height: u32, want_x: u32, want_y: u32) -> u32 {
    let undersized = u32::from(width < want_x || height < want_y) * 1_000;
    width.abs_diff(want_x) + height.abs_diff(want_y) + undersized
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ico(entries: &[(u8, u8, u32, u32)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        let mut payload = Vec::new();
        for (width, height, size, tag) in entries {
            let offset = 6 + entries.len() * 16 + payload.len();
            bytes.push(*width);
            bytes.push(*height);
            bytes.extend_from_slice(&[0, 0, 1, 0, 32, 0]);
            bytes.extend_from_slice(&size.to_le_bytes());
            bytes.extend_from_slice(&(offset as u32).to_le_bytes());
            payload.extend(std::iter::repeat_n(*tag as u8, *size as usize));
        }
        bytes.extend(payload);
        bytes
    }
    #[test]
    fn tray_bytes_follow_taskbar_theme_and_paused_state() {
        assert_eq!(tray_asset(Some(true), false), TrayAsset::Light);
        assert_eq!(tray_asset(Some(true), true), TrayAsset::LightPaused);
        assert_eq!(tray_asset(Some(false), false), TrayAsset::Dark);
        assert_eq!(tray_asset(Some(false), true), TrayAsset::DarkPaused);
        assert_eq!(tray_asset(None, true), TrayAsset::App);
        assert_eq!(tray_bytes(Some(true), false), TRAY_LIGHT);
        assert_eq!(tray_bytes(Some(false), true), TRAY_DARK_PAUSED);
    }
    #[test]
    fn icon_directory_rejects_invalid_headers_and_truncated_entries() {
        assert_eq!(icon_entry(&[], 16, 16), None);
        assert_eq!(icon_entry(&[0, 0, 2, 0, 1, 0], 16, 16), None);
        assert_eq!(icon_entry(&[0, 0, 1, 0, 1, 0, 16, 16], 16, 16), None);
    }
    #[test]
    fn icon_directory_prefers_exact_then_larger_sizes() {
        let bytes = ico(&[(16, 16, 4, 1), (32, 32, 4, 2), (0, 0, 4, 3)]);
        assert_eq!(icon_entry(&bytes, 16, 16).map(|(o, _)| bytes[o]), Some(1));
        assert_eq!(icon_entry(&bytes, 24, 24).map(|(o, _)| bytes[o]), Some(2));
        assert_eq!(icon_entry(&bytes, 256, 256).map(|(o, _)| bytes[o]), Some(3));
        assert_eq!(icon_entry(&bytes, 48, 48).map(|(o, _)| bytes[o]), Some(3));
    }
    #[test]
    fn shipped_brand_icos_expose_selectable_images() {
        for bytes in [
            APP,
            TRAY_LIGHT,
            TRAY_DARK,
            TRAY_LIGHT_PAUSED,
            TRAY_DARK_PAUSED,
        ] {
            assert!(icon_entry(bytes, 16, 16).is_some());
            assert!(icon_entry(bytes, 32, 32).is_some());
        }
    }
    #[test]
    fn theme_broadcast_is_case_insensitive() {
        assert!(theme_setting(Some("ImmersiveColorSet")));
        assert!(theme_setting(Some("immersivecolorset")));
        assert!(!theme_setting(Some("Policy")));
        assert!(!theme_setting(None));
    }
}
