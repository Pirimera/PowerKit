//! Hover hints for the native tray menu. When the pointer rests on an entry that
//! a global shortcut also triggers, a tracking tooltip shows the shortcut keys.
//!
//! `TrackPopupMenu` runs a modal loop that still sends `WM_MENUSELECT` and
//! `WM_TIMER` to the owner (the hidden app window), so the app window procedure
//! forwards both here. Everything runs on the app UI thread.
use super::wide;
use std::{
    cell::RefCell,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, WPARAM},
    UI::{
        Controls::{
            TTF_ABSOLUTE, TTF_TRACK, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH, TTM_TRACKACTIVATE,
            TTM_TRACKPOSITION, TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
        },
        WindowsAndMessaging::{
            CW_USEDEFAULT, CreateWindowExW, GetCursorPos, GetSubMenu, HMENU, HWND_TOPMOST,
            KillTimer, MF_POPUP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW, SetTimer,
            SetWindowPos, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
};

/// Owner-window timer id; distinct from the other app window timers.
pub const TIMER: usize = 5;
/// How long the pointer rests on an entry before the hint appears.
const DELAY_MS: u32 = 700;
/// Offset from the cursor so the hint does not cover the hovered label.
const OFFSET: (i32, i32) = (16, 20);
const TOOL_ID: usize = 1;

/// Shortcut text per menu entry, collected while the popup is built.
#[derive(Default)]
pub struct Hints {
    /// Command id of a plain item and its shortcut.
    pub items: Vec<(usize, String)>,
    /// Submenu handle of a popup entry and its shortcut. The handles are valid
    /// only while the menu is tracked; `Guard` drops them when tracking ends.
    pub popups: Vec<(HMENU, String)>,
}

struct State {
    owner: HWND,
    /// Tooltip window owned by `owner`; created on first use and reused.
    tips: HWND,
    hints: Hints,
    pending: Option<String>,
    shown: bool,
}

thread_local! {
    static STATE: RefCell<State> = const {
        RefCell::new(State {
            owner: null_mut(),
            tips: null_mut(),
            hints: Hints { items: Vec::new(), popups: Vec::new() },
            pending: None,
            shown: false,
        })
    };
}

/// Ends hinting when the tracked menu closes.
pub struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        reset();
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.hints = Hints::default();
        });
    }
}

/// Starts hinting for a menu about to be tracked by `owner`.
pub fn begin(owner: HWND, hints: Hints) -> Guard {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.owner = owner;
        state.hints = hints;
        state.pending = None;
    });
    Guard
}

/// Handles `WM_MENUSELECT`: hides any hint and, when the newly highlighted
/// entry has a shortcut, arms the rest timer.
pub fn on_menu_select(wparam: WPARAM, lparam: LPARAM) {
    reset();
    let item = wparam & 0xFFFF;
    let flags = ((wparam >> 16) & 0xFFFF) as u32;
    let menu = lparam as HMENU;
    // 0xFFFF with no menu means the menu closed.
    if menu.is_null() || flags == 0xFFFF {
        return;
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let text = if flags & MF_POPUP != 0 {
            // SAFETY: `menu` is the live menu Windows reports for this selection
            // and `item` is a position in it; GetSubMenu only reads it.
            let sub = unsafe { GetSubMenu(menu, item as i32) };
            state
                .hints
                .popups
                .iter()
                .find(|(popup, _)| !sub.is_null() && *popup == sub)
                .map(|(_, text)| text.clone())
        } else {
            state
                .hints
                .items
                .iter()
                .find(|(id, _)| *id == item)
                .map(|(_, text)| text.clone())
        };
        if text.is_some() && !state.owner.is_null() {
            state.pending = text;
            // SAFETY: owner is the live app window on this thread.
            unsafe {
                SetTimer(state.owner, TIMER, DELAY_MS, None);
            }
        }
    });
}

/// Handles the rest timer: shows the pending hint beside the cursor.
pub fn on_timer() {
    let (owner, text) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        (state.owner, state.pending.take())
    });
    if owner.is_null() {
        return;
    }
    // SAFETY: owner is the live app window that armed this timer.
    unsafe {
        KillTimer(owner, TIMER);
    }
    if let Some(text) = text {
        show(owner, &text);
    }
}

/// Kills the timer and hides a visible hint.
fn reset() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.pending = None;
        if !state.owner.is_null() {
            // SAFETY: owner is the live app window; killing an unset timer is harmless.
            unsafe {
                KillTimer(state.owner, TIMER);
            }
        }
        if state.shown && !state.tips.is_null() {
            let mut info = tool_info(state.owner);
            // SAFETY: tips is the live tooltip owned by `owner`; info outlives the call.
            unsafe {
                SendMessageW(state.tips, TTM_TRACKACTIVATE, 0, (&raw mut info) as LPARAM);
            }
            state.shown = false;
        }
    });
}

fn tool_info(owner: HWND) -> TTTOOLINFOW {
    // SAFETY: TTTOOLINFOW is plain data; zero is a valid starting value.
    let mut info = unsafe { zeroed::<TTTOOLINFOW>() };
    info.cbSize = size_of::<TTTOOLINFOW>() as u32;
    info.uFlags = TTF_TRACK | TTF_ABSOLUTE;
    info.hwnd = owner;
    info.uId = TOOL_ID;
    info
}

/// Creates the tooltip on first use and registers its single tracking tool.
fn tooltip(owner: HWND) -> HWND {
    let existing = STATE.with(|state| state.borrow().tips);
    if !existing.is_null() {
        return existing;
    }
    let class = wide("tooltips_class32");
    // SAFETY: creates a tooltip owned by the live app window; Windows destroys
    // it with the owner.
    let tips = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class.as_ptr(),
            null(),
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            owner,
            null_mut(),
            null_mut(),
            null(),
        )
    };
    if tips.is_null() {
        return tips;
    }
    let mut empty = wide("");
    let mut info = tool_info(owner);
    info.lpszText = empty.as_mut_ptr();
    // SAFETY: tips is the new tooltip; the tool copies the text it is given.
    unsafe {
        SendMessageW(tips, TTM_SETMAXTIPWIDTH, 0, 400);
        SendMessageW(tips, TTM_ADDTOOLW, 0, (&raw mut info) as LPARAM);
    }
    STATE.with(|state| state.borrow_mut().tips = tips);
    tips
}

fn show(owner: HWND, text: &str) {
    let tips = tooltip(owner);
    if tips.is_null() {
        return;
    }
    let mut buffer = wide(text);
    let mut info = tool_info(owner);
    info.lpszText = buffer.as_mut_ptr();
    let mut point = POINT::default();
    // SAFETY: tips is the live tooltip owned by `owner`; `info` and `buffer`
    // outlive each call and the tooltip copies the text.
    unsafe {
        GetCursorPos(&mut point);
        SendMessageW(tips, TTM_UPDATETIPTEXTW, 0, (&raw mut info) as LPARAM);
        let x = (point.x + OFFSET.0) as u16 as u32;
        let y = (point.y + OFFSET.1) as u16 as u32;
        SendMessageW(tips, TTM_TRACKPOSITION, 0, ((y << 16) | x) as LPARAM);
        SendMessageW(tips, TTM_TRACKACTIVATE, 1, (&raw mut info) as LPARAM);
        // The menu is topmost too; raise the hint above it.
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
    STATE.with(|state| state.borrow_mut().shown = true);
}
