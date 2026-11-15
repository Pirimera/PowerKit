//! ScreenRuler toolbar, matched to the PowerToys Measure Tool bar.
//! WinUI stays out of this overlay: the same 216×50 DIP layout, icon slots and
//! toggle behavior are drawn with GDI.

use super::theme::{self, WindowTheme};
use super::wide;
use crate::screen_ruler::Mode;
use std::{
    cell::RefCell,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::OnceLock,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{
        Dwm::{
            DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            DwmSetWindowAttribute,
        },
        Gdi::*,
    },
    UI::{
        Controls::{
            ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TTF_SUBCLASS,
            TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
        },
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
        },
        WindowsAndMessaging::*,
    },
};

pub(super) const WIDTH_DIP: i32 = 216;
pub(super) const HEIGHT_DIP: i32 = 50;
pub(super) const TOP_DIP: i32 = 12;
const BUTTON_DIP: i32 = 32;
const GAP_DIP: i32 = 8;
const SEPARATOR_DIP: i32 = 1;
const CORNER_DIP: i32 = 4;
const ICON_DIP: i32 = 16;
const CLOSE_DIP: i32 = 12;
const BOUNDS_GLYPH: u16 = 0xEF20;
const CLOSE_GLYPH: u16 = 0xE8BB;

const SPACING_PATH: &str = "M12 16.5C12 16.2239 11.7761 16 11.5 16L9 16V9H16V11.5C16 11.7761 16.2239 12 16.5 12C16.7761 12 17 11.7761 17 11.5V8.5V5.5C17 5.22386 16.7761 5 16.5 5C16.2239 5 16 5.22386 16 5.5V8H9V1L11.5 1C11.7761 1 12 0.776142 12 0.5C12 0.223858 11.7761 0 11.5 0H8.5014H8.5H8.4986H5.5C5.22386 0 5 0.223858 5 0.5C5 0.776142 5.22386 1 5.5 1H8L8 8H1L1 5.5C1 5.22386 0.776142 5 0.5 5C0.223858 5 0 5.22386 0 5.5V8.5V11.5C0 11.7761 0.223858 12 0.500001 12C0.776143 12 1 11.7761 1 11.5L1 9H8L8 16H5.5C5.22386 16 5 16.2239 5 16.5C5 16.7761 5.22386 17 5.5 17H8.5H11.5C11.7761 17 12 16.7761 12 16.5Z";
const HORIZONTAL_PATH: &str = "M1 10.5C1 10.7761 0.776143 11 0.500001 11C0.223858 11 0 10.7761 0 10.5V7.5V4.5C0 4.22386 0.223858 4 0.5 4C0.776142 4 1 4.22386 1 4.5L1 7L15 7V4.5C15 4.22386 15.2239 4 15.5 4C15.7761 4 16 4.22386 16 4.5V7.4993C16 7.49953 16 7.49977 16 7.5C16 7.50023 16 7.50047 16 7.5007V10.5C16 10.7761 15.7761 11 15.5 11C15.2239 11 15 10.7761 15 10.5V8L1 8L1 10.5Z";
const VERTICAL_PATH: &str = "M10.5 0C10.7761 0 11 0.223858 11 0.5C11 0.776142 10.7761 1 10.5 1L8 1V15H10.5C10.7761 15 11 15.2239 11 15.5C11 15.7761 10.7761 16 10.5 16H7.5H4.5C4.22386 16 4 15.7761 4 15.5C4 15.2239 4.22386 15 4.5 15H7V1L4.5 1C4.22386 1 4 0.776143 4 0.500001C4 0.223858 4.22386 0 4.5 0H7.5H10.5Z";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hit {
    Tool(Mode),
    Close,
}

#[derive(Clone, Copy)]
enum DrawOp {
    Move(f32, f32),
    Line(f32, f32),
    Curve(f32, f32, f32, f32, f32, f32),
    Close,
}

struct Icon {
    ops: Vec<DrawOp>,
    shift_x: f32,
    shift_y: f32,
}

struct Pointer {
    hot: Option<Hit>,
    down: Option<Hit>,
    tips: HWND,
    text: Vec<Vec<u16>>,
}

impl Default for Pointer {
    fn default() -> Self {
        Self {
            hot: None,
            down: None,
            tips: null_mut(),
            text: Vec::new(),
        }
    }
}

thread_local! {
    static POINTER: RefCell<Pointer> = RefCell::new(Pointer::default());
}

pub(super) fn px(dip: i32, dpi: i32) -> i32 {
    dip * dpi.max(96) / 96
}

pub(super) fn hit_test(x: i32, y: i32, width: i32, height: i32, dpi: i32) -> Option<Hit> {
    const HITS: [Option<Hit>; 6] = [
        Some(Hit::Tool(Mode::Bounds)),
        Some(Hit::Tool(Mode::Spacing)),
        Some(Hit::Tool(Mode::Horizontal)),
        Some(Hit::Tool(Mode::Vertical)),
        None,
        Some(Hit::Close),
    ];
    slots(width, height, dpi)
        .iter()
        .zip(HITS)
        .find(|(rect, _)| x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom)
        .and_then(|(_, hit)| hit)
}

pub(super) fn polish(hwnd: HWND) {
    let theme = theme::window_theme();
    let round = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const round).cast(),
            size_of::<i32>() as u32,
        );
        if !theme.high_contrast {
            let dark = u32::from(!theme.light);
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                (&raw const dark).cast(),
                size_of::<u32>() as u32,
            );
        }
    }
}

pub(super) fn paint(hwnd: HWND, mode: Mode) {
    unsafe {
        let mut paint = zeroed::<PAINTSTRUCT>();
        let dc = BeginPaint(hwnd, &mut paint);
        if dc.is_null() {
            return;
        }
        let (width, height, dpi) = client(hwnd);
        let theme = theme::window_theme();
        let (hot, down) = POINTER.with(|pointer| {
            let pointer = pointer.borrow();
            (pointer.hot, pointer.down)
        });
        let background = CreateSolidBrush(bar_color(&theme));
        let client_rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        FillRect(dc, &client_rect, background);
        DeleteObject(background);
        let rects = slots(width, height, dpi);
        let buttons = [
            (rects[0], Some(Mode::Bounds), IconKind::Bounds),
            (rects[1], Some(Mode::Spacing), IconKind::Spacing),
            (rects[2], Some(Mode::Horizontal), IconKind::Horizontal),
            (rects[3], Some(Mode::Vertical), IconKind::Vertical),
            (rects[5], None, IconKind::Close),
        ];
        for (rect, tool, kind) in buttons {
            let hit = tool.map(Hit::Tool).unwrap_or(Hit::Close);
            let selected = tool.is_some_and(|tool| tool == mode);
            let over = hot == Some(hit);
            let pressed = down == Some(hit) && over;
            if let Some(fill) = button_fill(&theme, selected, over, pressed) {
                fill_round(dc, rect, px(CORNER_DIP, dpi), fill);
            }
            draw_icon(dc, rect, dpi, kind, icon_color(&theme, selected, kind));
        }
        draw_separator(dc, rects[4], dpi, separator_color(&theme));
        EndPaint(hwnd, &paint);
    }
}

pub(super) fn pointer_move(hwnd: HWND, x: i32, y: i32) {
    let (width, height, dpi) = client(hwnd);
    let hit = hit_test(x, y, width, height, dpi);
    let changed = POINTER.with(|pointer| {
        let mut pointer = pointer.borrow_mut();
        if pointer.hot == hit {
            false
        } else {
            pointer.hot = hit;
            true
        }
    });
    if changed {
        unsafe {
            InvalidateRect(hwnd, null(), 0);
        }
    }
    let mut event = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    unsafe {
        TrackMouseEvent(&mut event);
    }
}

pub(super) fn pointer_leave(hwnd: HWND) {
    let changed = POINTER.with(|pointer| {
        let mut pointer = pointer.borrow_mut();
        if pointer.hot.is_none() {
            false
        } else {
            pointer.hot = None;
            true
        }
    });
    if changed {
        unsafe {
            InvalidateRect(hwnd, null(), 0);
        }
    }
}

pub(super) fn pointer_down(hwnd: HWND, x: i32, y: i32) {
    let (width, height, dpi) = client(hwnd);
    POINTER.with(|pointer| {
        pointer.borrow_mut().down = hit_test(x, y, width, height, dpi);
    });
    unsafe {
        SetCapture(hwnd);
        InvalidateRect(hwnd, null(), 0);
    }
}

pub(super) fn pointer_up(hwnd: HWND, x: i32, y: i32) -> Option<Hit> {
    let (width, height, dpi) = client(hwnd);
    let hit = hit_test(x, y, width, height, dpi);
    let down = POINTER.with(|pointer| pointer.borrow_mut().down.take());
    unsafe {
        ReleaseCapture();
        InvalidateRect(hwnd, null(), 0);
    }
    if hit.is_some() && hit == down {
        hit
    } else {
        None
    }
}

pub(super) fn attach_tips(hwnd: HWND, labels: [&str; 5]) {
    let previous =
        POINTER.with(|pointer| std::mem::replace(&mut pointer.borrow_mut().tips, null_mut()));
    if !previous.is_null() {
        unsafe {
            DestroyWindow(previous);
        }
    }
    POINTER.with(|pointer| pointer.borrow_mut().text.clear());
    ensure_comctl();
    let text: Vec<Vec<u16>> = labels.into_iter().map(wide).collect();
    let instance = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()) };
    let tips = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            wide("tooltips_class32").as_ptr(),
            null(),
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            hwnd,
            null_mut(),
            instance,
            null(),
        )
    };
    if tips.is_null() {
        return;
    }
    unsafe {
        SendMessageW(tips, TTM_SETMAXTIPWIDTH, 0, 400);
        SetWindowPos(
            tips,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    let (width, height, dpi) = client(hwnd);
    let rects = slots(width, height, dpi);
    let tip_slots = [0, 1, 2, 3, 5];
    for (index, slot) in tip_slots.into_iter().enumerate() {
        let mut info = unsafe { zeroed::<TTTOOLINFOW>() };
        info.cbSize = size_of::<TTTOOLINFOW>() as u32;
        info.uFlags = TTF_SUBCLASS;
        info.hwnd = hwnd;
        info.uId = index + 1;
        info.rect = rects[slot];
        info.lpszText = text[index].as_ptr().cast_mut();
        unsafe {
            SendMessageW(tips, TTM_ADDTOOLW, 0, (&raw mut info) as LPARAM);
        }
    }
    POINTER.with(|pointer| {
        let mut pointer = pointer.borrow_mut();
        pointer.text = text;
        pointer.tips = tips;
    });
}

pub(super) fn clear() {
    POINTER.with(|pointer| *pointer.borrow_mut() = Pointer::default());
}

fn slots(width: i32, height: i32, dpi: i32) -> [RECT; 6] {
    let button = px(BUTTON_DIP, dpi);
    let gap = px(GAP_DIP, dpi);
    let separator = px(SEPARATOR_DIP, dpi).max(1);
    let content = button * 5 + gap * 5 + separator;
    let mut x = (width - content) / 2;
    let y = (height - button) / 2;
    let mut rects = [RECT::default(); 6];
    for rect in rects.iter_mut().take(4) {
        *rect = RECT {
            left: x,
            top: y,
            right: x + button,
            bottom: y + button,
        };
        x += button + gap;
    }
    rects[4] = RECT {
        left: x,
        top: y,
        right: x + separator,
        bottom: y + button,
    };
    x += separator + gap;
    rects[5] = RECT {
        left: x,
        top: y,
        right: x + button,
        bottom: y + button,
    };
    rects
}

fn client(hwnd: HWND) -> (i32, i32, i32) {
    let mut rect = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut rect);
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    (
        rect.right,
        rect.bottom,
        if dpi == 0 { 96 } else { dpi as i32 },
    )
}

fn ensure_comctl() {
    static READY: OnceLock<()> = OnceLock::new();
    READY.get_or_init(|| {
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        };
        unsafe {
            InitCommonControlsEx(&controls);
        }
    });
}

#[derive(Clone, Copy)]
enum IconKind {
    Bounds,
    Spacing,
    Horizontal,
    Vertical,
    Close,
}

fn draw_icon(dc: HDC, rect: RECT, dpi: i32, kind: IconKind, color: u32) {
    match kind {
        IconKind::Bounds => draw_glyph(dc, rect, dpi, BOUNDS_GLYPH, ICON_DIP, color),
        IconKind::Close => draw_glyph(dc, rect, dpi, CLOSE_GLYPH, CLOSE_DIP, color),
        IconKind::Spacing => fill_icon(dc, rect, dpi, icon(0), color),
        IconKind::Horizontal => fill_icon(dc, rect, dpi, icon(1), color),
        IconKind::Vertical => fill_icon(dc, rect, dpi, icon(2), color),
    }
}

fn draw_glyph(dc: HDC, rect: RECT, dpi: i32, glyph: u16, dip: i32, color: u32) {
    unsafe {
        let font = CreateFontW(
            -px(dip, dpi),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe Fluent Icons").as_ptr(),
        );
        if font.is_null() {
            return;
        }
        let previous = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        let mut text = [glyph, 0];
        let mut bounds = rect;
        DrawTextW(
            dc,
            text.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(dc, previous);
        DeleteObject(font);
    }
}

fn fill_icon(dc: HDC, rect: RECT, dpi: i32, icon: &Icon, color: u32) {
    let scale = dpi.max(96) as f32 / 96.0;
    let (min_x, min_y, max_x, max_y) = icon_bounds(icon);
    let origin_x = (rect.left + rect.right) as f32 / 2.0 - (min_x + max_x) / 2.0 * scale;
    let origin_y = (rect.top + rect.bottom) as f32 / 2.0 - (min_y + max_y) / 2.0 * scale;
    let place = |x: f32, y: f32| POINT {
        x: (origin_x + (x + icon.shift_x) * scale).round() as i32,
        y: (origin_y + (y + icon.shift_y) * scale).round() as i32,
    };
    unsafe {
        BeginPath(dc);
        for op in &icon.ops {
            match *op {
                DrawOp::Move(x, y) => {
                    let point = place(x, y);
                    MoveToEx(dc, point.x, point.y, null_mut());
                }
                DrawOp::Line(x, y) => {
                    let point = place(x, y);
                    LineTo(dc, point.x, point.y);
                }
                DrawOp::Curve(x1, y1, x2, y2, x, y) => {
                    let points = [place(x1, y1), place(x2, y2), place(x, y)];
                    PolyBezierTo(dc, points.as_ptr(), 3);
                }
                DrawOp::Close => {
                    CloseFigure(dc);
                }
            }
        }
        EndPath(dc);
        let brush = CreateSolidBrush(color);
        let previous = SelectObject(dc, brush);
        FillPath(dc);
        SelectObject(dc, previous);
        DeleteObject(brush);
    }
}

fn icon(index: usize) -> &'static Icon {
    static ICONS: OnceLock<[Icon; 3]> = OnceLock::new();
    &ICONS.get_or_init(|| {
        [
            Icon {
                ops: compile(SPACING_PATH),
                shift_x: 0.0,
                shift_y: 0.0,
            },
            Icon {
                ops: compile(HORIZONTAL_PATH),
                shift_x: 0.0,
                shift_y: -2.0,
            },
            Icon {
                ops: compile(VERTICAL_PATH),
                shift_x: -3.0,
                shift_y: 0.0,
            },
        ]
    })[index]
}

fn icon_bounds(icon: &Icon) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut consider = |x: f32, y: f32| {
        let x = x + icon.shift_x;
        let y = y + icon.shift_y;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    for op in &icon.ops {
        match *op {
            DrawOp::Move(x, y) | DrawOp::Line(x, y) => consider(x, y),
            DrawOp::Curve(x1, y1, x2, y2, x, y) => {
                consider(x1, y1);
                consider(x2, y2);
                consider(x, y);
            }
            DrawOp::Close => {}
        }
    }
    (min_x, min_y, max_x, max_y)
}

fn compile(path: &str) -> Vec<DrawOp> {
    let bytes = path.as_bytes();
    let mut index = 0;
    let mut command = 0u8;
    let mut current_x = 0.0;
    let mut current_y = 0.0;
    let mut ops = Vec::new();
    while index < bytes.len() {
        if matches!(bytes[index], b' ' | b',' | b'\n' | b'\r' | b'\t') {
            index += 1;
            continue;
        }
        if bytes[index].is_ascii_alphabetic() {
            command = bytes[index];
            index += 1;
            if command == b'Z' || command == b'z' {
                ops.push(DrawOp::Close);
            }
            continue;
        }
        match command {
            b'M' | b'L' => {
                let Some((x, next)) = number_at(bytes, index) else {
                    break;
                };
                let Some((y, next)) = number_at(bytes, next) else {
                    break;
                };
                index = next;
                if command == b'M' {
                    ops.push(DrawOp::Move(x, y));
                    command = b'L';
                } else {
                    ops.push(DrawOp::Line(x, y));
                }
                current_x = x;
                current_y = y;
            }
            b'H' => {
                let Some((x, next)) = number_at(bytes, index) else {
                    break;
                };
                index = next;
                ops.push(DrawOp::Line(x, current_y));
                current_x = x;
            }
            b'V' => {
                let Some((y, next)) = number_at(bytes, index) else {
                    break;
                };
                index = next;
                ops.push(DrawOp::Line(current_x, y));
                current_y = y;
            }
            b'C' => {
                let mut values = [0.0; 6];
                for value in &mut values {
                    let Some((parsed, next)) = number_at(bytes, index) else {
                        return ops;
                    };
                    *value = parsed;
                    index = next;
                }
                ops.push(DrawOp::Curve(
                    values[0], values[1], values[2], values[3], values[4], values[5],
                ));
                current_x = values[4];
                current_y = values[5];
            }
            _ => break,
        }
    }
    ops
}

fn number_at(bytes: &[u8], mut index: usize) -> Option<(f32, usize)> {
    while index < bytes.len() && matches!(bytes[index], b' ' | b',') {
        index += 1;
    }
    let start = index;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }
    let mut saw_digit = false;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        saw_digit = true;
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            saw_digit = true;
            index += 1;
        }
    }
    if !saw_digit {
        return None;
    }
    let text = std::str::from_utf8(&bytes[start..index]).ok()?;
    Some((text.parse().ok()?, index))
}

fn fill_round(dc: HDC, rect: RECT, radius: i32, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, color);
        let previous_brush = SelectObject(dc, brush);
        let previous_pen = SelectObject(dc, pen);
        let curve = radius.saturating_mul(2);
        RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            curve,
            curve,
        );
        SelectObject(dc, previous_pen);
        SelectObject(dc, previous_brush);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

fn draw_separator(dc: HDC, rect: RECT, dpi: i32, color: u32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, px(SEPARATOR_DIP, dpi).max(1), color);
        let previous = SelectObject(dc, pen);
        let x = rect.left + (rect.right - rect.left) / 2;
        let inset = px(4, dpi);
        MoveToEx(dc, x, rect.top + inset, null_mut());
        LineTo(dc, x, rect.bottom - inset);
        SelectObject(dc, previous);
        DeleteObject(pen);
    }
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

fn bar_color(theme: &WindowTheme) -> u32 {
    if theme.high_contrast {
        theme.card
    } else if theme.light {
        rgb(0xF3, 0xF3, 0xF3)
    } else {
        rgb(0x20, 0x20, 0x20)
    }
}

fn button_fill(theme: &WindowTheme, selected: bool, over: bool, pressed: bool) -> Option<u32> {
    if selected {
        Some(theme.accent)
    } else if pressed {
        Some(if theme.high_contrast {
            theme.button
        } else if theme.light {
            rgb(0xEB, 0xEB, 0xEB)
        } else {
            rgb(0x3A, 0x3A, 0x3A)
        })
    } else if over {
        Some(if theme.high_contrast {
            theme.button_hot
        } else if theme.light {
            rgb(0xFF, 0xFF, 0xFF)
        } else {
            rgb(0x45, 0x45, 0x45)
        })
    } else {
        None
    }
}

fn icon_color(theme: &WindowTheme, selected: bool, kind: IconKind) -> u32 {
    if selected {
        on_accent(theme.accent)
    } else if matches!(kind, IconKind::Close) && !theme.high_contrast {
        rgb(0xC4, 0x2B, 0x1C)
    } else {
        theme.text
    }
}

fn on_accent(color: u32) -> u32 {
    let red = color & 0xff;
    let green = (color >> 8) & 0xff;
    let blue = (color >> 16) & 0xff;
    if red * 30 + green * 59 + blue * 11 > 14_000 {
        0
    } else {
        0x00FF_FFFF
    }
}

fn separator_color(theme: &WindowTheme) -> u32 {
    if theme.high_contrast {
        theme.stroke
    } else if theme.light {
        rgb(0xC8, 0xC8, 0xC8)
    } else {
        rgb(0x5A, 0x5A, 0x5A)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_match_the_powertoys_toolbar() {
        assert_eq!(hit_test(23, 25, 216, 50, 96), Some(Hit::Tool(Mode::Bounds)));
        assert_eq!(
            hit_test(63, 25, 216, 50, 96),
            Some(Hit::Tool(Mode::Spacing))
        );
        assert_eq!(
            hit_test(103, 25, 216, 50, 96),
            Some(Hit::Tool(Mode::Horizontal))
        );
        assert_eq!(
            hit_test(143, 25, 216, 50, 96),
            Some(Hit::Tool(Mode::Vertical))
        );
        assert_eq!(hit_test(167, 25, 216, 50, 96), None);
        assert_eq!(hit_test(192, 25, 216, 50, 96), Some(Hit::Close));
        assert_eq!(hit_test(23, 8, 216, 50, 96), None);
        assert_eq!(hit_test(43, 25, 216, 50, 96), None);
        assert_eq!(
            hit_test(47, 50, 432, 100, 192),
            Some(Hit::Tool(Mode::Bounds))
        );
        assert_eq!(
            hit_test(127, 50, 432, 100, 192),
            Some(Hit::Tool(Mode::Spacing))
        );
        assert_eq!(hit_test(385, 50, 432, 100, 192), Some(Hit::Close));
    }

    #[test]
    fn path_icons_keep_the_powertoys_geometry() {
        let spacing = compile(SPACING_PATH);
        let horizontal = compile(HORIZONTAL_PATH);
        let vertical = compile(VERTICAL_PATH);
        assert!(matches!(spacing[0], DrawOp::Move(_, _)));
        assert!(spacing.iter().any(|op| matches!(op, DrawOp::Close)));
        assert!(horizontal.iter().any(|op| matches!(op, DrawOp::Close)));
        assert!(vertical.iter().any(|op| matches!(op, DrawOp::Close)));
        let horizontal = Icon {
            ops: horizontal,
            shift_x: 0.0,
            shift_y: -2.0,
        };
        let (min_x, min_y, max_x, max_y) = icon_bounds(&horizontal);
        assert!(min_x <= 0.5 && max_x >= 15.5);
        assert!(min_y < 4.0 && max_y > 8.0);
    }
}
