//! Low-level mouse (and optional keyboard) hook on a dedicated thread.
#![expect(
    unsafe_code,
    reason = "WH_MOUSE_LL / WH_KEYBOARD_LL callbacks and message pump"
)]
use super::inject::{self, Command as InjectCommand};
use powerkit_core::power_mouse::{
    CursorAnchor, Disposition, Engine, Event, Output, Preferences, Trigger,
};
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
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
const MSG_DEVICE: u32 = WM_USER + 3;

pub use powerkit_app::controller::power_mouse::link::DeviceInput;

/// Hands adapter events to the hook thread, which owns the engine.
#[derive(Clone)]
pub struct DeviceSink {
    inputs: Sender<DeviceInput>,
    tid: u32,
}

impl DeviceSink {
    pub fn send(&self, input: DeviceInput) {
        if self.inputs.send(input).is_ok() {
            // SAFETY: tid is the hook thread; a thread that has exited only
            // makes the post fail.
            unsafe {
                PostThreadMessageW(self.tid, MSG_DEVICE, 0, 0);
            }
        }
    }
}
/// The long-press timers running on the hook thread, by the id Windows gave
/// each one. A thread timer (no window) ignores the id passed to `SetTimer`
/// and gets a new one from Windows, so the trigger is found through this
/// table rather than by encoding it in the id.
#[derive(Debug, Default)]
struct TimerTable {
    running: Vec<(usize, Trigger)>,
}

impl TimerTable {
    /// Records a timer; returns the trigger's previous timer, to be killed.
    fn arm(&mut self, id: usize, trigger: Trigger) -> Option<usize> {
        let previous = self.disarm(trigger);
        self.running.push((id, trigger));
        previous
    }

    /// Forgets the trigger's timer and returns its id, to be killed.
    fn disarm(&mut self, trigger: Trigger) -> Option<usize> {
        let index = self.running.iter().position(|(_, t)| *t == trigger)?;
        Some(self.running.swap_remove(index).0)
    }

    /// The trigger a WM_TIMER is for; the timer is forgotten. `None` for a
    /// timer that is not a long-press timer or was disarmed in the meantime.
    fn fire(&mut self, id: usize) -> Option<Trigger> {
        let index = self.running.iter().position(|(each, _)| *each == id)?;
        Some(self.running.swap_remove(index).1)
    }
}

thread_local! {
    static TIMERS: RefCell<TimerTable> = RefCell::new(TimerTable::default());
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::new(Preferences::default()));
    static INJECT: RefCell<Option<Sender<InjectCommand>>> = const { RefCell::new(None) };
    static ANCHOR: RefCell<CursorAnchor> = const { RefCell::new(CursorAnchor::new()) };
    static MOUSE_HOOK: RefCell<HHOOK> = const { RefCell::new(std::ptr::null_mut()) };
    static KEYBOARD_HOOK: RefCell<HHOOK> = const { RefCell::new(std::ptr::null_mut()) };
}

pub struct Hook {
    tid: u32,
    apply_tx: Sender<Preferences>,
    device_tx: Sender<DeviceInput>,
    join: Option<JoinHandle<()>>,
}

impl Hook {
    pub fn start(prefs: Preferences, inject_tx: Sender<InjectCommand>) -> Option<Self> {
        let (ready_tx, ready_rx) = sync_channel(1);
        // Unbounded: a snapshot published before the previous one was read
        // must not be dropped; the hook thread keeps only the newest.
        let (apply_tx, apply_rx) = channel();
        let (device_tx, device_rx) = channel();
        let join = thread::Builder::new()
            .name("powerkit-mouse-hook".into())
            .spawn(move || hook_thread(prefs, inject_tx, apply_rx, device_rx, ready_tx))
            .ok()?;
        match ready_rx.recv() {
            Ok(Ok(tid)) => Some(Self {
                tid,
                apply_tx,
                device_tx,
                join: Some(join),
            }),
            _ => {
                let _ = join.join();
                None
            }
        }
    }

    pub fn apply(&self, prefs: Preferences) {
        let _ = self.apply_tx.send(prefs);
        // SAFETY: tid belongs to the hook thread with a message queue.
        unsafe {
            PostThreadMessageW(self.tid, MSG_APPLY, 0, 0);
        }
    }

    /// Where vendor adapters send their device control events.
    pub fn device_sink(&self) -> DeviceSink {
        DeviceSink {
            inputs: self.device_tx.clone(),
            tid: self.tid,
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
    inject_tx: Sender<InjectCommand>,
    apply_rx: Receiver<Preferences>,
    device_rx: Receiver<DeviceInput>,
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
                if let Some(prefs) = newest(&apply_rx) {
                    sync_keyboard_hook(prefs.needs_keyboard_hook());
                    let outputs = ENGINE
                        .with(|engine| engine.borrow_mut().replace_preferences(prefs).outputs);
                    dispatch(outputs);
                }
            }
            MSG_DEVICE => {
                for input in device_rx.try_iter() {
                    device_input(input);
                }
            }
            MSG_PASS_ALL => {
                let outputs =
                    ENGINE.with(|engine| engine.borrow_mut().set_pass_all(msg.wParam != 0).outputs);
                dispatch(outputs);
            }
            WM_TIMER => {
                if let Some(trigger) = TIMERS.with(|timers| timers.borrow_mut().fire(msg.wParam)) {
                    // SAFETY: the timer was set on this thread; a long press
                    // fires once, so it is killed rather than left repeating.
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

/// Feeds one adapter event to the engine. A device control has no Windows
/// event to pass through or suppress, so only the outputs matter. While an
/// elevated window is in front the engine passes everything; the adapter
/// undiverts its controls then, so events normally stop arriving at all.
fn device_input(input: DeviceInput) {
    let pass_all = super::elevation::is_elevated_foreground();
    let outputs = ENGINE.with(|engine| engine.borrow_mut().set_pass_all(pass_all).outputs);
    dispatch(outputs);
    let event = match input {
        DeviceInput::Press(trigger) => Event::Press(trigger),
        DeviceInput::Release(trigger) => Event::Release(trigger),
        DeviceInput::Move { dx, dy } => Event::Move { dx, dy },
    };
    let decision = ENGINE.with(|engine| engine.borrow_mut().handle(event, Instant::now()));
    dispatch(decision.outputs);
}

/// Drains the queue and keeps the last snapshot, so a burst of settings
/// changes applies the newest one whichever MSG_APPLY is handled first.
fn newest<T>(queue: &Receiver<T>) -> Option<T> {
    queue.try_iter().last()
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
    let Some(trigger) = super::super::keys::from_vk(vk).and_then(Trigger::from_key) else {
        return call_next(code, wparam, lparam);
    };
    debug_assert_eq!(trigger.key().map(super::super::keys::vk), Some(vk));
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
            // Unbounded send: never blocks the hook and never drops a
            // MirrorUp, which would leave a button held down. It fails only
            // once the injector is gone, when nothing is left to release.
            let _ = tx.send(InjectCommand::Outputs(outputs));
        }
    });
}

fn arm_timer(trigger: Trigger) {
    // SAFETY: a thread timer on this thread's message loop, delivered as
    // WM_TIMER with the id Windows returns here.
    let id = unsafe {
        SetTimer(
            std::ptr::null_mut(),
            0,
            u32::try_from(powerkit_core::power_mouse::LONG_PRESS.as_millis()).unwrap_or(500),
            None,
        )
    };
    if id == 0 {
        return;
    }
    if let Some(previous) = TIMERS.with(|timers| timers.borrow_mut().arm(id, trigger)) {
        kill_timer(previous);
    }
}

fn disarm_timer(trigger: Trigger) {
    if let Some(id) = TIMERS.with(|timers| timers.borrow_mut().disarm(trigger)) {
        kill_timer(id);
    }
}

fn kill_timer(id: usize) {
    // SAFETY: the id was returned by SetTimer on this thread.
    unsafe {
        KillTimer(std::ptr::null_mut(), id);
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return call_next(code, wparam, lparam);
    }
    // SAFETY: WH_MOUSE_LL HC_ACTION guarantees a live MSLLHOOKSTRUCT.
    let Some(data) = (unsafe { hook_data(lparam) }) else {
        return call_next(code, wparam, lparam);
    };
    let (x, y) = (data.pt.x, data.pt.y);
    if data.flags & LLMHF_INJECTED != 0 || data.dwExtraInfo == inject::EXTRA_INFO {
        advance_anchor(x, y);
        return call_next(code, wparam, lparam);
    }
    if super::elevation::is_elevated_foreground() {
        let outputs = ENGINE.with(|engine| engine.borrow_mut().set_pass_all(true).outputs);
        dispatch(outputs);
        advance_anchor(x, y);
        return call_next(code, wparam, lparam);
    } else {
        let _ = ENGINE.with(|engine| engine.borrow_mut().set_pass_all(false));
    }

    let now = Instant::now();
    let travel = ANCHOR.with(|anchor| anchor.borrow().delta_to(x, y));
    let Some(event) = translate(wparam, data, travel) else {
        advance_anchor(x, y);
        return call_next(code, wparam, lparam);
    };
    let is_move = matches!(event, Event::Move { .. });
    if matches!(event, Event::Press(_)) || is_move {
        update_dpi(x, y);
    }
    let decision = ENGINE.with(|engine| engine.borrow_mut().handle(event, now));
    // A swallowed move leaves the cursor in place, so the anchor stays too.
    if !(is_move && decision.disposition == Disposition::Suppress) {
        advance_anchor(x, y);
    }
    dispatch(decision.outputs);
    match decision.disposition {
        Disposition::PassThrough => call_next(code, wparam, lparam),
        Disposition::Suppress => 1,
    }
}

fn advance_anchor(x: i32, y: i32) {
    ANCHOR.with(|anchor| anchor.borrow_mut().advance(x, y));
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

/// `travel` is the distance from the last applied cursor position to this
/// event's point; it is `None` before the first event.
fn translate(wparam: WPARAM, data: MSLLHOOKSTRUCT, travel: Option<(i32, i32)>) -> Option<Event> {
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
            let (dx, dy) = travel?;
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
    fn long_press_timers_are_found_by_the_id_windows_gave_them() {
        let mut timers = TimerTable::default();
        assert_eq!(timers.arm(7, Trigger::Middle), None);
        assert_eq!(timers.arm(9, Trigger::F13), None);
        // Re-arming a trigger hands back its old timer to kill.
        assert_eq!(timers.arm(12, Trigger::Middle), Some(7));
        assert_eq!(timers.fire(7), None, "a killed timer fires nothing");
        assert_eq!(timers.fire(12), Some(Trigger::Middle));
        assert_eq!(timers.fire(12), None, "a long press fires once");
        // Releasing before the timer elapses disarms it.
        assert_eq!(timers.disarm(Trigger::F13), Some(9));
        assert_eq!(timers.fire(9), None);
        assert_eq!(timers.disarm(Trigger::F13), None);
    }

    /// The real thread-timer behaviour the table exists for: Windows gives a
    /// windowless timer its own id, and WM_TIMER carries that id.
    #[test]
    fn a_thread_timer_reports_the_id_windows_returned() {
        use windows_sys::Win32::UI::WindowsAndMessaging::PM_REMOVE;
        // SAFETY: a thread timer on this test thread, read back with a
        // message loop on the same thread and killed before returning.
        unsafe {
            let id = SetTimer(std::ptr::null_mut(), 0x504B_0000, 1, None);
            assert_ne!(id, 0);
            let mut msg = MSG::default();
            let deadline = Instant::now() + std::time::Duration::from_secs(2);
            let mut fired = None;
            while fired.is_none() && Instant::now() < deadline {
                if PeekMessageW(
                    &raw mut msg,
                    std::ptr::null_mut(),
                    WM_TIMER,
                    WM_TIMER,
                    PM_REMOVE,
                ) != 0
                {
                    fired = Some(msg.wParam);
                }
            }
            KillTimer(std::ptr::null_mut(), id);
            assert_eq!(fired, Some(id));
            assert_ne!(id, 0x504B_0000, "the id passed in is ignored");
        }
    }

    #[test]
    fn a_burst_of_snapshots_applies_the_newest() {
        let (tx, rx) = channel();
        for value in [1, 2, 3] {
            tx.send(value).unwrap();
        }
        assert_eq!(newest(&rx), Some(3));
        assert_eq!(newest(&rx), None, "a later MSG_APPLY finds nothing stale");
    }
}
