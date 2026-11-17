//! SendInput synthesis for PowerMouse actions. Runs on a dedicated injector thread.
#![expect(
    unsafe_code,
    reason = "SendInput batches, desktop queries and LockWorkStation"
)]
use crate::power_mouse::{Action, Fire, Output};
use std::mem::size_of;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;
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
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
            KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
            MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN,
            MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VK_BROWSER_BACK, VK_BROWSER_FORWARD,
            VK_CONTROL, VK_LWIN, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
            VK_MENU, VK_RWIN, VK_SHIFT, VK_TAB, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
        },
        WindowsAndMessaging::{PostMessageW, WM_COMMAND, XBUTTON1, XBUTTON2},
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
    tx: SyncSender<Command>,
    join: Option<JoinHandle<()>>,
}

impl Injector {
    pub fn start(app: HWND) -> Self {
        let app = app as isize;
        let (tx, rx) = sync_channel(64);
        let join = thread::Builder::new()
            .name("powerkit-mouse-inject".into())
            .spawn(move || run(app as HWND, rx))
            .expect("spawn PowerMouse injector");
        Self {
            tx,
            join: Some(join),
        }
    }

    pub fn sender(&self) -> SyncSender<Command> {
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
    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => {
                release_all(&mut mirrored);
                break;
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Ok(Command::Outputs(outputs)) => {
                for output in outputs {
                    match output {
                        Output::Fire(Fire::Instant(action)) => fire_instant(app, &action),
                        Output::Fire(Fire::MirrorDown(action)) => {
                            if mirror_down(&action) {
                                mirrored.push(action);
                            }
                        }
                        Output::Fire(Fire::MirrorUp(action)) => {
                            if mirror_up(&action) {
                                mirrored.retain(|held| held != &action);
                            }
                        }
                        Output::ArmLongPress { .. } | Output::DisarmLongPress { .. } => {}
                    }
                }
            }
        }
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
    if let Some(keys) = chord(action) {
        send_chord(&keys);
    }
}

fn powerkit_command(action: &Action) -> Option<u16> {
    match action {
        Action::PinLastWindow => Some(super::super::command::PIN as u16),
        Action::StartScreenRuler => Some(super::super::command::MEASURE as u16),
        Action::OpenPowerKit => Some(super::super::command::OPEN as u16),
        _ => None,
    }
}

fn mirror_down(action: &Action) -> bool {
    mouse_flags(action).is_some_and(|(down, _, data)| {
        if !desk_ok() || key_conflict(&[]) {
            return false;
        }
        send_inputs(&[mouse_input(down, data)])
    })
}

fn mirror_up(action: &Action) -> bool {
    mouse_flags(action).is_some_and(|(_, up, data)| send_inputs(&[mouse_input(up, data)]))
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
            if hotkey.win {
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
            keys.push(hotkey.vk);
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
    fn catalog_actions_map_to_chords_or_specials() {
        assert_eq!(chord(&Action::Copy), Some(vec![VK_CONTROL, VK_C]));
        assert_eq!(chord(&Action::VolumeUp), Some(vec![VK_VOLUME_UP]));
        assert!(chord(&Action::LockScreen).is_none());
        assert!(powerkit_command(&Action::OpenPowerKit).is_some());
        assert!(mouse_flags(&Action::MiddleClick).is_some());
    }
}
