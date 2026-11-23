//! SendInput synthesis for PowerMouse actions. Runs on a dedicated injector thread.
#![expect(
    unsafe_code,
    reason = "SendInput batches, desktop queries and LockWorkStation"
)]
use powerkit_core::power_mouse::{Action, Fire, Output};
use std::collections::VecDeque;
use std::mem::size_of;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::HWND,
    System::{
        Shutdown::LockWorkStation,
        StationsAndDesktops::{
            CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop,
            UOI_NAME,
        },
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyState, GetKeyboardLayout, HKL, INPUT, INPUT_0, INPUT_KEYBOARD,
            INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_CHAR,
            MAPVK_VK_TO_VSC, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
            MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN,
            MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyExW, SendInput, VK_BROWSER_BACK,
            VK_BROWSER_FORWARD, VK_CAPITAL, VK_CONTROL, VK_LWIN, VK_MEDIA_NEXT_TRACK,
            VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT,
            VK_TAB, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP, VkKeyScanExW,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, WM_COMMAND, XBUTTON1,
            XBUTTON2,
        },
    },
};

/// Tagged in dwExtraInfo so low-level hooks can ignore our own synthesis.
pub const EXTRA_INFO: usize = 0x504B_4D53; // 'PKMS'

const VK_C: u16 = 0x43;
const VK_V: u16 = 0x56;
const VK_X: u16 = 0x58;
const VK_Z: u16 = 0x5A;
const VK_Y: u16 = 0x59;
const VK_A: u16 = 0x41;
const VK_F: u16 = 0x46;
const VK_S: u16 = 0x53;
const VK_T: u16 = 0x54;
const VK_W: u16 = 0x57;
const VK_R: u16 = 0x52;
const VK_D: u16 = 0x44;
const VK_LEFT: u16 = 0x25;
const VK_RIGHT: u16 = 0x27;

pub enum Command {
    Outputs(Vec<Output>),
    Stop,
}

pub struct Injector {
    tx: Sender<Command>,
    join: Option<JoinHandle<()>>,
}

impl Injector {
    pub fn start(app: HWND) -> Self {
        let app = app as isize;
        // Unbounded so the hook callback can always hand over a release.
        let (tx, rx) = channel();
        let join = thread::Builder::new()
            .name("powerkit-mouse-inject".into())
            .spawn(move || run(app as HWND, rx))
            .expect("spawn PowerMouse injector");
        Self {
            tx,
            join: Some(join),
        }
    }

    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }

    pub fn stop(mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Injector {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run(app: HWND, rx: Receiver<Command>) {
    let mut mirrored = Vec::<Action>::new();
    let mut flow = Flow::default();
    loop {
        // A workflow pause is a deadline, not a sleep, so button releases and
        // other outputs are still handled while it waits.
        let wait = flow
            .wait(Instant::now())
            .unwrap_or(Duration::from_millis(250));
        match rx.recv_timeout(wait) {
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => {
                release_all(&mut mirrored);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Ok(Command::Outputs(outputs)) => {
                for output in outputs {
                    match output {
                        Output::Fire(Fire::Instant(Action::Workflow(steps))) => {
                            let _ = flow.start(steps);
                        }
                        Output::Fire(Fire::Instant(action)) => fire_instant(app, &action),
                        Output::Fire(Fire::MirrorDown(action)) => {
                            if mirror_down(&action) {
                                mirrored.push(action);
                            }
                        }
                        Output::Fire(Fire::MirrorUp(action)) => {
                            release_held(&mut mirrored, &action, mirror_up);
                        }
                        Output::ArmLongPress { .. } | Output::DisarmLongPress { .. } => {}
                    }
                }
            }
        }
        for step in flow.due(Instant::now()) {
            fire_instant(app, &step);
        }
    }
}

/// The workflow in progress: its remaining steps and, during a pause, when to
/// carry on. Stopping the injector drops whatever has not run yet.
#[derive(Default)]
struct Flow {
    steps: VecDeque<Action>,
    resume_at: Option<Instant>,
}

impl Flow {
    /// Starts a workflow unless one is still running or pausing; a second
    /// press must not interleave its steps with the first.
    fn start(&mut self, steps: Vec<Action>) -> bool {
        if !self.steps.is_empty() || self.resume_at.is_some() {
            return false;
        }
        self.steps = steps.into();
        true
    }

    /// How long the injector may sleep before a step is due; `None` when idle.
    fn wait(&self, now: Instant) -> Option<Duration> {
        match self.resume_at {
            Some(at) => Some(at.saturating_duration_since(now)),
            None => (!self.steps.is_empty()).then_some(Duration::ZERO),
        }
    }

    /// Takes the steps to run now, up to the next pause.
    fn due(&mut self, now: Instant) -> Vec<Action> {
        if let Some(at) = self.resume_at {
            if now < at {
                return Vec::new();
            }
            self.resume_at = None;
        }
        let mut ready = Vec::new();
        while let Some(step) = self.steps.pop_front() {
            if let Action::Delay(ms) = step {
                self.resume_at = Some(now + Duration::from_millis(u64::from(ms)));
                break;
            }
            ready.push(step);
        }
        ready
    }
}

/// Sends the button-up only for a button this thread actually put down. A
/// down that was refused (modifier held, secure desktop) has no entry, so its
/// release must not inject a stray up. One entry is released per call: two
/// triggers mirrored to the same button each own their own down.
fn release_held(
    mirrored: &mut Vec<Action>,
    action: &Action,
    send_up: impl FnOnce(&Action) -> bool,
) {
    let Some(index) = mirrored.iter().position(|held| held == action) else {
        return;
    };
    if send_up(action) {
        mirrored.swap_remove(index);
    }
}

fn release_all(mirrored: &mut Vec<Action>) {
    for action in mirrored.drain(..) {
        let _ = mirror_up(&action);
    }
}

fn fire_instant(app: HWND, action: &Action) {
    if action.is_noop() {
        return;
    }
    if let Some(id) = powerkit_command(action) {
        // SAFETY: app is the live PowerKit window for this process.
        unsafe {
            PostMessageW(app, WM_COMMAND, usize::from(id), 0);
        }
        return;
    }
    if matches!(action, Action::LockScreen) {
        if desk_ok() {
            // SAFETY: LockWorkStation takes no arguments.
            unsafe {
                LockWorkStation();
            }
        }
        return;
    }
    if action.is_mouse_button() {
        click(action);
        return;
    }
    if let Action::Text(text) = action {
        type_text(text);
        return;
    }
    if let Some(device_action) = action.device_action() {
        // Carried out by the adapter thread of each connected device.
        super::adapter::act(device_action);
        return;
    }
    // Workflows are scheduled by the run loop and pauses are its deadlines;
    // neither reaches here as a step to send.
    if let Some(keys) = chord(action) {
        send_chord(&keys);
    }
}

/// Pause after each Unicode packet, so the target has read one before the
/// next replaces it.
const PACKET_GAP: Duration = Duration::from_millis(10);
const SCAN_SHIFT: u16 = 0x2A;

/// How one UTF-16 unit of typed text is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stroke {
    /// A key of the target's keyboard layout, with Shift when the character
    /// needs it. Windows keeps the key with its message, so the character is
    /// right however late the target reads it.
    Key { vk: u16, shift: bool },
    /// A Unicode packet (`VK_PACKET`). Windows keeps only the last packet's
    /// character for the target's thread, so a target that falls behind
    /// reads the latest one for every packet still queued; packets are
    /// therefore used only for characters the layout cannot type, one at a
    /// time with a pause between them.
    Packet(u16),
}

/// Plans typed text. `key_for` says which key of the target's layout types
/// a character (and whether with Shift); with Caps Lock on, letters would
/// change case, so they go as packets.
fn plan_text(
    text: &str,
    caps_lock: bool,
    key_for: impl Fn(u16) -> Option<(u16, bool)>,
) -> Vec<Stroke> {
    text.encode_utf16()
        .filter(|unit| *unit != u16::from(b'\r'))
        .map(|unit| match unit {
            0x0A => Stroke::Key {
                vk: VK_RETURN,
                shift: false,
            },
            0x09 => Stroke::Key {
                vk: VK_TAB,
                shift: false,
            },
            0xD800..=0xDFFF => Stroke::Packet(unit),
            _ if caps_lock && char::from_u32(u32::from(unit)).is_some_and(char::is_alphabetic) => {
                Stroke::Packet(unit)
            }
            _ => key_for(unit).map_or(Stroke::Packet(unit), |(vk, shift)| Stroke::Key {
                vk,
                shift,
            }),
        })
        .collect()
}

/// The input events of one stroke; `scan_for` gives a key's scan code.
fn stroke_inputs(stroke: Stroke, scan_for: impl Fn(u16) -> u16) -> Vec<INPUT> {
    match stroke {
        Stroke::Key { vk, shift } => {
            let key = |up| typed_key(vk, scan_for(vk), up);
            if shift {
                vec![
                    typed_key(VK_SHIFT, SCAN_SHIFT, false),
                    key(false),
                    key(true),
                    typed_key(VK_SHIFT, SCAN_SHIFT, true),
                ]
            } else {
                vec![key(false), key(true)]
            }
        }
        Stroke::Packet(unit) => vec![unicode_input(unit, false), unicode_input(unit, true)],
    }
}

/// Types text into the foreground window. Refused while a modifier is held,
/// because the characters would become shortcuts. Stops at the first stroke
/// Windows accepts only in part, after releasing that stroke's keys.
fn type_text(text: &str) {
    if text.is_empty() || !desk_ok() || key_conflict(&[]) {
        return;
    }
    let layout = foreground_layout();
    // SAFETY: reads the toggle state of a valid virtual key.
    let caps_lock = unsafe { GetKeyState(i32::from(VK_CAPITAL)) } & 1 != 0;
    let strokes = plan_text(text, caps_lock, |unit| layout_key(unit, layout));
    let scan_for = |vk: u16| {
        // SAFETY: plain lookup in the layout; returns 0 for no scan code.
        let scan = unsafe { MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_VSC, layout) };
        u16::try_from(scan).unwrap_or(0)
    };
    for stroke in strokes {
        let inputs = stroke_inputs(stroke, scan_for);
        if send_inputs_count(&inputs) < inputs.len() {
            let releases: Vec<_> = inputs
                .iter()
                .filter_map(|input| {
                    // SAFETY: every stroke input is a keyboard input.
                    let key = unsafe { input.Anonymous.ki };
                    (key.dwFlags & KEYEVENTF_KEYUP != 0).then_some(*input)
                })
                .collect();
            let _ = send_inputs_count(&releases);
            return;
        }
        if matches!(stroke, Stroke::Packet(_)) {
            thread::sleep(PACKET_GAP);
        }
    }
}

/// The keyboard layout of the window that will receive the text.
fn foreground_layout() -> HKL {
    // SAFETY: plain queries; a null window gives thread 0, whose layout is
    // this thread's, a safe fallback.
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        GetKeyboardLayout(thread)
    }
}

/// The key that types `unit` on `layout`, unmodified or with Shift. `None`
/// when the character needs Ctrl, Alt or AltGr, is a dead key, or is not on
/// the layout at all.
fn layout_key(unit: u16, layout: HKL) -> Option<(u16, bool)> {
    // SAFETY: plain lookups in the layout.
    let found = unsafe { VkKeyScanExW(unit, layout) };
    if found == -1 {
        return None;
    }
    let [vk, modifiers] = found.to_le_bytes();
    if vk == 0 || modifiers > 1 {
        return None;
    }
    // SAFETY: plain lookup; the top bit marks a dead key.
    let mapped = unsafe { MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_CHAR, layout) };
    (mapped & 0x8000_0000 == 0).then_some((u16::from(vk), modifiers == 1))
}

fn typed_key(vk: u16, scan: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: EXTRA_INFO,
            },
        },
    }
}

fn unicode_input(unit: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: EXTRA_INFO,
            },
        },
    }
}

fn powerkit_command(action: &Action) -> Option<u16> {
    match action {
        Action::PinLastWindow => Some(super::super::command::PIN as u16),
        Action::StartScreenRuler => Some(super::super::command::MEASURE as u16),
        Action::OpenPowerKit => Some(super::super::command::OPEN as u16),
        // Shown by the UI thread, which owns the ring window.
        Action::ActionsRing => Some(super::super::command::POWER_MOUSE_RING as u16),
        _ => None,
    }
}

/// Puts a button or a key chord down and leaves it down. Returns whether
/// anything is now held, so the caller records only what it must release.
fn mirror_down(action: &Action) -> bool {
    if let Some((down, _, data)) = mouse_flags(action) {
        return desk_ok() && !key_conflict(&[]) && send_inputs(&[mouse_input(down, data)]);
    }
    let Some(keys) = chord(action) else {
        return false;
    };
    if !desk_ok() || key_conflict(&keys) {
        return false;
    }
    let downs: Vec<_> = keys.iter().map(|key| key_input(*key, false)).collect();
    let inserted = send_inputs_count(&downs);
    if inserted == downs.len() {
        return true;
    }
    // Partial batch: release exactly the keys that went down, newest first.
    let releases: Vec<_> = keys[..inserted.min(keys.len())]
        .iter()
        .rev()
        .map(|key| key_input(*key, true))
        .collect();
    let _ = send_inputs_count(&releases);
    false
}

fn mirror_up(action: &Action) -> bool {
    if let Some((_, up, data)) = mouse_flags(action) {
        return send_inputs(&[mouse_input(up, data)]);
    }
    chord(action).is_some_and(|keys| {
        let ups: Vec<_> = keys.iter().rev().map(|key| key_input(*key, true)).collect();
        send_inputs(&ups)
    })
}

fn click(action: &Action) {
    let Some((down, up, data)) = mouse_flags(action) else {
        return;
    };
    if !desk_ok() || key_conflict(&[]) {
        return;
    }
    let _ = send_inputs(&[mouse_input(down, data), mouse_input(up, data)]);
}

fn mouse_flags(action: &Action) -> Option<(u32, u32, i32)> {
    match action {
        Action::LeftClick => Some((MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0)),
        Action::RightClick => Some((MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0)),
        Action::MiddleClick => Some((MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0)),
        Action::MouseBack => Some((MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, i32::from(XBUTTON1))),
        Action::MouseForward => Some((MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, i32::from(XBUTTON2))),
        _ => None,
    }
}

fn chord(action: &Action) -> Option<Vec<u16>> {
    Some(match action {
        Action::BrowserBack => vec![VK_BROWSER_BACK],
        Action::BrowserForward => vec![VK_BROWSER_FORWARD],
        Action::TaskView => vec![VK_LWIN, VK_TAB],
        Action::PreviousDesktop => vec![VK_LWIN, VK_CONTROL, VK_LEFT],
        Action::NextDesktop => vec![VK_LWIN, VK_CONTROL, VK_RIGHT],
        Action::ShowDesktop => vec![VK_LWIN, VK_D],
        Action::Copy => vec![VK_CONTROL, VK_C],
        Action::Paste => vec![VK_CONTROL, VK_V],
        Action::Cut => vec![VK_CONTROL, VK_X],
        Action::Undo => vec![VK_CONTROL, VK_Z],
        Action::Redo => vec![VK_CONTROL, VK_Y],
        Action::SelectAll => vec![VK_CONTROL, VK_A],
        Action::Find => vec![VK_CONTROL, VK_F],
        Action::Save => vec![VK_CONTROL, VK_S],
        Action::NewTab => vec![VK_CONTROL, VK_T],
        Action::CloseTab => vec![VK_CONTROL, VK_W],
        Action::ReopenTab => vec![VK_CONTROL, VK_SHIFT, VK_T],
        Action::NextTab => vec![VK_CONTROL, VK_TAB],
        Action::PrevTab => vec![VK_CONTROL, VK_SHIFT, VK_TAB],
        Action::ReloadPage => vec![VK_CONTROL, VK_R],
        Action::PlayPause => vec![VK_MEDIA_PLAY_PAUSE],
        Action::NextTrack => vec![VK_MEDIA_NEXT_TRACK],
        Action::PrevTrack => vec![VK_MEDIA_PREV_TRACK],
        Action::VolumeUp => vec![VK_VOLUME_UP],
        Action::VolumeDown => vec![VK_VOLUME_DOWN],
        Action::MuteVolume => vec![VK_VOLUME_MUTE],
        Action::CaptureRegion => vec![VK_LWIN, VK_SHIFT, VK_S],
        Action::Custom(hotkey) => {
            let mut keys = Vec::new();
            if hotkey.meta {
                keys.push(VK_LWIN);
            }
            if hotkey.ctrl {
                keys.push(VK_CONTROL);
            }
            if hotkey.alt {
                keys.push(VK_MENU);
            }
            if hotkey.shift {
                keys.push(VK_SHIFT);
            }
            keys.push(super::super::keys::vk(hotkey.key));
            keys
        }
        _ => return None,
    })
}

fn send_chord(keys: &[u16]) {
    if !desk_ok() || key_conflict(keys) {
        return;
    }
    let mut inputs = Vec::with_capacity(keys.len() * 2);
    for key in keys {
        inputs.push(key_input(*key, false));
    }
    for key in keys.iter().rev() {
        inputs.push(key_input(*key, true));
    }
    let inserted = send_inputs_count(&inputs);
    if inserted != inputs.len() {
        let pending = pending_keys(keys, inserted);
        let releases: Vec<_> = pending.iter().rev().map(|k| key_input(*k, true)).collect();
        let _ = send_inputs_count(&releases);
    }
}

fn pending_keys(keys: &[u16], inserted: usize) -> Vec<u16> {
    let mut down = Vec::new();
    for key in keys.iter().take(inserted.min(keys.len())) {
        down.push(*key);
    }
    for key in keys.iter().rev().take(inserted.saturating_sub(keys.len())) {
        down.retain(|held| held != key);
    }
    down
}

fn key_conflict(keys: &[u16]) -> bool {
    for vk in [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
        .iter()
        .copied()
        .chain(keys.iter().copied())
    {
        // SAFETY: valid virtual-key code; reads current key state only.
        if unsafe { GetAsyncKeyState(i32::from(vk)) } < 0 {
            return true;
        }
    }
    false
}

fn desk_ok() -> bool {
    // SAFETY: read-only access to the input desktop.
    let desk = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if desk.is_null() {
        return false;
    }
    let mut name = [0u16; 64];
    let mut bytes = 0;
    // SAFETY: correctly sized buffer; desk closed once below.
    let ok = unsafe {
        GetUserObjectInformationW(
            desk,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            128,
            &raw mut bytes,
        )
    } != 0;
    // SAFETY: desk was returned by OpenInputDesktop.
    unsafe {
        CloseDesktop(desk);
    }
    if !ok {
        return false;
    }
    let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    String::from_utf16_lossy(&name[..end]) == "Default"
}

fn send_inputs(inputs: &[INPUT]) -> bool {
    send_inputs_count(inputs) == inputs.len()
}

fn send_inputs_count(inputs: &[INPUT]) -> usize {
    if inputs.is_empty() {
        return 0;
    }
    let Ok(count) = u32::try_from(inputs.len()) else {
        return 0;
    };
    let Ok(size) = i32::try_from(size_of::<INPUT>()) else {
        return 0;
    };
    // SAFETY: initialized INPUT slice; SendInput copies it.
    unsafe { SendInput(count, inputs.as_ptr(), size) as usize }
}

fn key_input(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: EXTRA_INFO,
            },
        },
    }
}

fn mouse_input(flags: u32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: u32::from_ne_bytes(data.to_ne_bytes()),
                dwFlags: flags,
                time: 0,
                dwExtraInfo: EXTRA_INFO,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_chord_cleanup_matches_modifier_state() {
        let keys = [VK_CONTROL, VK_SHIFT, VK_TAB];
        assert_eq!(pending_keys(&keys, 0), vec![]);
        assert_eq!(pending_keys(&keys, 1), vec![VK_CONTROL]);
        assert_eq!(pending_keys(&keys, 3), vec![VK_CONTROL, VK_SHIFT, VK_TAB]);
        assert_eq!(pending_keys(&keys, 4), vec![VK_CONTROL, VK_SHIFT]);
        assert_eq!(pending_keys(&keys, 6), vec![]);
    }

    #[test]
    fn release_sends_up_only_for_a_button_that_went_down() {
        let mut sent = 0;
        let mut mirrored = Vec::new();
        // The down was refused, so nothing is held and no up is injected.
        release_held(&mut mirrored, &Action::LeftClick, |_| {
            sent += 1;
            true
        });
        assert_eq!(sent, 0);

        // Two triggers hold the same button: each release sends one up.
        mirrored.extend([Action::LeftClick, Action::MiddleClick, Action::LeftClick]);
        release_held(&mut mirrored, &Action::LeftClick, |_| {
            sent += 1;
            true
        });
        assert_eq!(sent, 1);
        assert_eq!(mirrored.len(), 2);
        assert!(mirrored.contains(&Action::LeftClick));

        // A failed up stays recorded so stopping still releases it.
        release_held(&mut mirrored, &Action::MiddleClick, |_| false);
        assert!(mirrored.contains(&Action::MiddleClick));
    }

    #[test]
    fn workflow_runs_up_to_each_pause_and_refuses_to_interleave() {
        let start = Instant::now();
        let mut flow = Flow::default();
        assert_eq!(flow.wait(start), None);
        assert!(flow.start(vec![
            Action::Copy,
            Action::Delay(200),
            Action::Paste,
            Action::Undo,
        ]));
        assert_eq!(flow.wait(start), Some(Duration::ZERO));
        assert_eq!(flow.due(start), vec![Action::Copy]);
        // Pausing: nothing is due early, and a second workflow is refused.
        assert_eq!(flow.wait(start), Some(Duration::from_millis(200)));
        assert!(flow.due(start + Duration::from_millis(199)).is_empty());
        assert!(!flow.start(vec![Action::Cut]));
        assert_eq!(
            flow.due(start + Duration::from_millis(200)),
            vec![Action::Paste, Action::Undo]
        );
        assert_eq!(flow.wait(start + Duration::from_millis(200)), None);
        assert!(flow.start(vec![Action::Cut]));
    }

    /// A Turkish Q layout in miniature: letters, Shift for capitals, `ş` on
    /// its own key, `@` only with AltGr.
    fn turkish_key(unit: u16) -> Option<(u16, bool)> {
        match char::from_u32(u32::from(unit))? {
            c @ 'a'..='z' => Some((c.to_ascii_uppercase() as u16, false)),
            c @ 'A'..='Z' => Some((c as u16, true)),
            'ş' => Some((0xBA, false)),
            'Ş' => Some((0xBA, true)),
            ' ' => Some((0x20, false)),
            _ => None,
        }
    }

    #[test]
    fn text_uses_layout_keys_and_packets_only_for_the_rest() {
        let strokes = plan_text("aŞ @\r\n😀", false, turkish_key);
        assert_eq!(
            strokes,
            vec![
                Stroke::Key {
                    vk: 0x41,
                    shift: false
                },
                Stroke::Key {
                    vk: 0xBA,
                    shift: true
                },
                Stroke::Key {
                    vk: 0x20,
                    shift: false
                },
                Stroke::Packet(u16::from(b'@')),
                Stroke::Key {
                    vk: VK_RETURN,
                    shift: false
                },
                // An emoji is a surrogate pair, sent unit by unit.
                Stroke::Packet(0xD83D),
                Stroke::Packet(0xDE00),
            ]
        );
        // With Caps Lock on, Shift would type the wrong case: letters go as
        // packets, other keys stay keys.
        assert_eq!(
            plan_text("a ", true, turkish_key),
            vec![
                Stroke::Packet(u16::from(b'a')),
                Stroke::Key {
                    vk: 0x20,
                    shift: false
                },
            ]
        );
    }

    #[test]
    fn a_shifted_key_is_wrapped_in_shift_and_a_packet_is_a_unicode_pair() {
        let key = |input: &INPUT| {
            // SAFETY: every stroke input is a keyboard input.
            let ki = unsafe { input.Anonymous.ki };
            (ki.wVk, ki.wScan, ki.dwFlags, ki.dwExtraInfo)
        };
        let shifted: Vec<_> = stroke_inputs(
            Stroke::Key {
                vk: 0xBA,
                shift: true,
            },
            |_| 0x27,
        )
        .iter()
        .map(key)
        .collect();
        assert_eq!(
            shifted,
            vec![
                (VK_SHIFT, SCAN_SHIFT, 0, EXTRA_INFO),
                (0xBA, 0x27, 0, EXTRA_INFO),
                (0xBA, 0x27, KEYEVENTF_KEYUP, EXTRA_INFO),
                (VK_SHIFT, SCAN_SHIFT, KEYEVENTF_KEYUP, EXTRA_INFO),
            ]
        );
        let packet: Vec<_> = stroke_inputs(Stroke::Packet(0x011F), |_| 0)
            .iter()
            .map(key)
            .collect();
        assert_eq!(
            packet,
            vec![
                (0, 0x011F, KEYEVENTF_UNICODE, EXTRA_INFO),
                (0, 0x011F, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, EXTRA_INFO),
            ]
        );
    }

    #[test]
    fn the_current_layout_types_latin_letters_with_plain_keys() {
        let layout = foreground_layout();
        // Every Latin-script layout puts `a` on VK_A; skip on any other.
        if let Some((vk, shift)) = layout_key(u16::from(b'a'), layout) {
            assert_eq!((vk, shift), (0x41, false));
            assert_eq!(layout_key(u16::from(b'A'), layout), Some((0x41, true)));
        }
    }

    #[test]
    fn catalog_actions_map_to_chords_or_specials() {
        assert_eq!(chord(&Action::Copy), Some(vec![VK_CONTROL, VK_C]));
        assert_eq!(chord(&Action::VolumeUp), Some(vec![VK_VOLUME_UP]));
        assert!(chord(&Action::LockScreen).is_none());
        assert!(powerkit_command(&Action::OpenPowerKit).is_some());
        assert!(mouse_flags(&Action::MiddleClick).is_some());
    }
}
