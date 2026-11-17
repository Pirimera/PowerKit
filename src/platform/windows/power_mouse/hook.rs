//! Low-level mouse (and optional keyboard) hook on a dedicated thread.
#![expect(
    unsafe_code,
    reason = "WH_MOUSE_LL / WH_KEYBOARD_LL callbacks and message pump"
)]
use super::inject::{self, Command as InjectCommand};
use crate::power_mouse::{Disposition, Engine, Event, Output, Preferences, Trigger};
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use windows_sys::Win32::{
    Foundation::{GetLastError, LPARAM, LRESULT, POINT, WPARAM},
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
            KillTimer, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
            PeekMessageW, PostThreadMessageW, SetTimer, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_QUIT,
            WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_USER,
            WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
        },
    },
};

const MSG_APPLY: u32 = WM_USER + 1;
const MSG_PASS_ALL: u32 = WM_USER + 2;
const LONG_PRESS_TIMER_BASE: usize = 0x504B_0000;

thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::new(Preferences::default()));
    static INJECT: RefCell<Option<SyncSender<InjectCommand>>> = const { RefCell::new(None) };
    static LAST_POINT: RefCell<Option<(i32, i32)>> = const { RefCell::new(None) };
    static MOUSE_HOOK: RefCell<HHOOK> = const { RefCell::new(std::ptr::null_mut()) };
    static KEYBOARD_HOOK: RefCell<HHOOK> = const { RefCell::new(std::ptr::null_mut()) };
}

pub struct Hook {
    tid: u32,
    apply_tx: SyncSender<Preferences>,
    join: Option<JoinHandle<()>>,
}

impl Hook {
    pub fn start(prefs: Preferences, inject_tx: SyncSender<InjectCommand>) -> Option<Self> {
        let (ready_tx, ready_rx) = sync_channel(1);
        let (apply_tx, apply_rx) = sync_channel(1);
        let join = thread::Builder::new()
            .name("powerkit-mouse-hook".into())
            .spawn(move || hook_thread(prefs, inject_tx, apply_rx, ready_tx))
            .ok()?;
        match ready_rx.recv() {
            Ok(Ok(tid)) => Some(Self {
                tid,
                apply_tx,
                join: Some(join),
            }),
            _ => {
                let _ = join.join();
                None
            }
        }
    }

    pub fn apply(&self, prefs: Preferences) {
        let _ = self.apply_tx.try_send(prefs);
        // SAFETY: tid belongs to the hook thread with a message queue.
        unsafe {
            PostThreadMessageW(self.tid, MSG_APPLY, 0, 0);
        }
    }

    pub fn set_pass_all(&self, pass_all: bool) {
        // SAFETY: tid belongs to the hook thread with a message queue.
        unsafe {
            PostThreadMessageW(self.tid, MSG_PASS_ALL, usize::from(pass_all), 0);
        }
    }

    pub fn stop(mut self) {
        // SAFETY: ask the hook thread to quit its message loop.
        unsafe {
            PostThreadMessageW(self.tid, WM_QUIT, 0, 0);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        if self.join.is_some() {
            unsafe {
                PostThreadMessageW(self.tid, WM_QUIT, 0, 0);
            }
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }
}

fn hook_thread(
    prefs: Preferences,
    inject_tx: SyncSender<InjectCommand>,
    apply_rx: Receiver<Preferences>,
    ready: SyncSender<Result<u32, u32>>,
) {
    INJECT.with(|slot| *slot.borrow_mut() = Some(inject_tx));
    let needs_keys = prefs.needs_keyboard_hook();
    ENGINE.with(|engine| {
        *engine.borrow_mut() = Engine::new(prefs);
    });

    let mut bootstrap = MSG::default();
    // SAFETY: forces creation of this thread's message queue before ready is signaled.
    unsafe {
        PeekMessageW(
            &raw mut bootstrap,
            std::ptr::null_mut(),
            WM_USER,
            WM_USER,
            PM_NOREMOVE,
        );
    }
    // SAFETY: GetCurrentThreadId has no preconditions.
    let tid = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };

    // SAFETY: mouse_proc is a valid HOOKPROC; null module + thread 0 installs a global LL hook.
    let mouse =
        unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), std::ptr::null_mut(), 0) };
    if mouse.is_null() {
        // SAFETY: GetLastError after the failed SetWindowsHookExW.
        let err = unsafe { GetLastError() };
        let _ = ready.send(Err(err));
        return;
    }
    MOUSE_HOOK.with(|slot| *slot.borrow_mut() = mouse);
    sync_keyboard_hook(needs_keys);
    let _ = ready.send(Ok(tid));

    let mut msg = MSG::default();
    loop {
        // SAFETY: msg is owned; null hwnd retrieves this thread's messages.
        let result = unsafe { GetMessageW(&raw mut msg, std::ptr::null_mut(), 0, 0) };
        if result == 0 {
            break;
        }
        if result < 0 {
            break;
        }
        match msg.message {
            MSG_APPLY => {
                if let Ok(prefs) = apply_rx.try_recv() {
                    sync_keyboard_hook(prefs.needs_keyboard_hook());
                    let outputs = ENGINE
                        .with(|engine| engine.borrow_mut().replace_preferences(prefs).outputs);
                    dispatch(outputs);
                }
            }
            MSG_PASS_ALL => {
                let outputs =
                    ENGINE.with(|engine| engine.borrow_mut().set_pass_all(msg.wParam != 0).outputs);
                dispatch(outputs);
            }
            WM_TIMER => {
                if let Some(trigger) = trigger_from_timer(msg.wParam) {
                    // SAFETY: timer was armed for this thread.
                    unsafe {
                        KillTimer(std::ptr::null_mut(), msg.wParam);
                    }
                    let decision = ENGINE.with(|engine| {
                        engine
                            .borrow_mut()
                            .handle(Event::LongPressElapsed(trigger), Instant::now())
                    });
                    dispatch(decision.outputs);
                }
            }
            _ => {
                // SAFETY: msg was filled by GetMessageW.
                unsafe {
                    TranslateMessage(&raw const msg);
                    DispatchMessageW(&raw const msg);
                }
            }
        }
    }

    let outputs = ENGINE.with(|engine| {
        engine
            .borrow_mut()
            .handle(Event::Cancel, Instant::now())
            .outputs
    });
    dispatch(outputs);
    sync_keyboard_hook(false);
    MOUSE_HOOK.with(|slot| {
        let hook = slot.replace(std::ptr::null_mut());
        if !hook.is_null() {
            // SAFETY: hook was installed above and is unhooked once.
            unsafe {
                UnhookWindowsHookEx(hook);
            }
        }
    });
    INJECT.with(|slot| *slot.borrow_mut() = None);
}

fn sync_keyboard_hook(needs: bool) {
    KEYBOARD_HOOK.with(|slot| {
        let current = *slot.borrow();
        if needs && current.is_null() {
            // SAFETY: keyboard_proc is a valid HOOKPROC for WH_KEYBOARD_LL.
            let hook = unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), std::ptr::null_mut(), 0)
            };
            if !hook.is_null() {
                *slot.borrow_mut() = hook;
            }
        } else if !needs && !current.is_null() {
            // SAFETY: hook was installed on this thread and is unhooked once.
            unsafe {
                UnhookWindowsHookEx(current);
            }
            *slot.borrow_mut() = std::ptr::null_mut();
        }
    });
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return call_next(code, wparam, lparam);
    }
    // SAFETY: WH_KEYBOARD_LL HC_ACTION guarantees a live KBDLLHOOKSTRUCT.
    let Some(data) = (unsafe { keyboard_data(lparam) }) else {
        return call_next(code, wparam, lparam);
    };
    if data.flags & LLKHF_INJECTED != 0 || data.dwExtraInfo == inject::EXTRA_INFO {
        return call_next(code, wparam, lparam);
    }
    if super::elevation::is_elevated_foreground() {
        let outputs = ENGINE.with(|engine| engine.borrow_mut().set_pass_all(true).outputs);
        dispatch(outputs);
        return call_next(code, wparam, lparam);
    } else {
        let _ = ENGINE.with(|engine| engine.borrow_mut().set_pass_all(false));
    }
    let vk = data.vkCode as u16;
    let Some(trigger) = Trigger::from_vk(vk) else {
        return call_next(code, wparam, lparam);
    };
    debug_assert_eq!(trigger.vk(), Some(vk));
    let now = Instant::now();
    let event = match wparam as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => Event::Press(trigger),
        WM_KEYUP | WM_SYSKEYUP => Event::Release(trigger),
        _ => return call_next(code, wparam, lparam),
    };
    let decision = ENGINE.with(|engine| engine.borrow_mut().handle(event, now));
    dispatch(decision.outputs);
    match decision.disposition {
        Disposition::PassThrough => call_next(code, wparam, lparam),
        Disposition::Suppress => 1,
    }
}

unsafe fn keyboard_data(lparam: LPARAM) -> Option<KBDLLHOOKSTRUCT> {
    if lparam == 0 {
        return None;
    }
    // SAFETY: caller guarantees WH_KEYBOARD_LL HC_ACTION lParam.
    Some(unsafe { *(lparam as *const KBDLLHOOKSTRUCT) })
}

fn dispatch(outputs: Vec<Output>) {
    if outputs.is_empty() {
        return;
    }
    for output in &outputs {
        match output {
            Output::ArmLongPress { trigger } => arm_timer(*trigger),
            Output::DisarmLongPress { trigger } => disarm_timer(*trigger),
            Output::Fire(_) => {}
        }
    }
    INJECT.with(|slot| {
        if let Some(tx) = slot.borrow().as_ref() {
            match tx.try_send(InjectCommand::Outputs(outputs)) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {}
            }
        }
    });
}

fn arm_timer(trigger: Trigger) {
    let id = timer_id(trigger);
    // SAFETY: thread timers on this message loop; id is unique per trigger.
    unsafe {
        SetTimer(
            std::ptr::null_mut(),
            id,
            u32::try_from(crate::power_mouse::LONG_PRESS.as_millis()).unwrap_or(500),
            None,
        );
    }
}

fn disarm_timer(trigger: Trigger) {
    // SAFETY: matching SetTimer id on this thread.
    unsafe {
        KillTimer(std::ptr::null_mut(), timer_id(trigger));
    }
}

fn timer_id(trigger: Trigger) -> usize {
    LONG_PRESS_TIMER_BASE + trigger as usize
}

fn trigger_from_timer(id: usize) -> Option<Trigger> {
    let index = id.checked_sub(LONG_PRESS_TIMER_BASE)?;
    [
        Trigger::Middle,
        Trigger::Back,
        Trigger::Forward,
        Trigger::TiltLeft,
        Trigger::TiltRight,
        Trigger::F13,
        Trigger::F14,
        Trigger::F15,
        Trigger::F16,
        Trigger::F17,
        Trigger::F18,
        Trigger::F19,
        Trigger::F20,
        Trigger::F21,
        Trigger::F22,
        Trigger::F23,
        Trigger::F24,
    ]
    .get(index)
    .copied()
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return call_next(code, wparam, lparam);
    }
    // SAFETY: WH_MOUSE_LL HC_ACTION guarantees a live MSLLHOOKSTRUCT.
    let Some(data) = (unsafe { hook_data(lparam) }) else {
        return call_next(code, wparam, lparam);
    };
    if data.flags & LLMHF_INJECTED != 0 || data.dwExtraInfo == inject::EXTRA_INFO {
        return call_next(code, wparam, lparam);
    }
    if super::elevation::is_elevated_foreground() {
        let outputs = ENGINE.with(|engine| engine.borrow_mut().set_pass_all(true).outputs);
        dispatch(outputs);
        return call_next(code, wparam, lparam);
    } else {
        let _ = ENGINE.with(|engine| engine.borrow_mut().set_pass_all(false));
    }

    let now = Instant::now();
    let decision = match translate(wparam, data) {
        Some(event) => {
            if matches!(event, Event::Press(_) | Event::Move { .. }) {
                update_dpi(data.pt.x, data.pt.y);
            }
            ENGINE.with(|engine| engine.borrow_mut().handle(event, now))
        }
        None => {
            return call_next(code, wparam, lparam);
        }
    };
    dispatch(decision.outputs);
    match decision.disposition {
        Disposition::PassThrough => call_next(code, wparam, lparam),
        Disposition::Suppress => 1,
    }
}

fn update_dpi(x: i32, y: i32) {
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
    let point = POINT { x, y };
    // SAFETY: MONITOR_DEFAULTTONEAREST always returns a monitor.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut dpi_x = 0u32;
    let mut dpi_y = 0u32;
    // SAFETY: monitor is valid; dpi out-params are writable.
    let hr =
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) };
    if hr >= 0 && dpi_x > 0 {
        let scale = dpi_x as f32 / 96.0;
        ENGINE.with(|engine| engine.borrow_mut().set_dpi_scale(scale));
    }
}

fn translate(wparam: WPARAM, data: MSLLHOOKSTRUCT) -> Option<Event> {
    let previous = LAST_POINT.with(|slot| slot.replace(Some((data.pt.x, data.pt.y))));
    match wparam as u32 {
        WM_MBUTTONDOWN => Some(Event::Press(Trigger::Middle)),
        WM_MBUTTONUP => Some(Event::Release(Trigger::Middle)),
        WM_XBUTTONDOWN => match high_word(data.mouseData) {
            XBUTTON1 => Some(Event::Press(Trigger::Back)),
            XBUTTON2 => Some(Event::Press(Trigger::Forward)),
            _ => None,
        },
        WM_XBUTTONUP => match high_word(data.mouseData) {
            XBUTTON1 => Some(Event::Release(Trigger::Back)),
            XBUTTON2 => Some(Event::Release(Trigger::Forward)),
            _ => None,
        },
        WM_MOUSEHWHEEL => {
            let delta = signed_high_word(data.mouseData);
            if delta < 0 {
                Some(Event::Press(Trigger::TiltLeft))
            } else if delta > 0 {
                Some(Event::Press(Trigger::TiltRight))
            } else {
                None
            }
        }
        WM_MOUSEMOVE => {
            let (prev_x, prev_y) = previous?;
            let dx = data.pt.x - prev_x;
            let dy = data.pt.y - prev_y;
            (dx != 0 || dy != 0).then_some(Event::Move { dx, dy })
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP => None,
        _ => None,
    }
}

fn high_word(value: u32) -> u16 {
    (value >> 16) as u16
}

fn signed_high_word(value: u32) -> i16 {
    high_word(value) as i16
}

unsafe fn hook_data(lparam: LPARAM) -> Option<MSLLHOOKSTRUCT> {
    if lparam == 0 {
        return None;
    }
    // SAFETY: caller guarantees WH_MOUSE_LL HC_ACTION lParam.
    Some(unsafe { *(lparam as *const MSLLHOOKSTRUCT) })
}

fn call_next(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: null hhk is the documented way to call the next hook.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_ids_round_trip_for_page_triggers() {
        for trigger in [Trigger::Middle, Trigger::Back, Trigger::Forward] {
            assert_eq!(trigger_from_timer(timer_id(trigger)), Some(trigger));
        }
    }

    #[test]
    fn pass_through_bindings_do_not_need_a_hook_thread_message() {
        let prefs = Preferences::default();
        assert!(!prefs.enabled);
        assert!(prefs.binding(Trigger::Middle).is_pass_through());
    }
}
