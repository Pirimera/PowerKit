//! Logitech device adapter: one thread per connected device. The thread owns
//! the HID++ channel and the waiting: for a report, a command, the retry and
//! the battery tick. What to divert and what the device's events mean is
//! decided by `powerkit_app::controller::power_mouse::link`; its trigger input goes to
//! the hook thread and its status to the page. All device I/O happens on
//! this thread, never on the UI thread.
#![expect(
    unsafe_code,
    reason = "wake event handle and PostMessageW to the PowerKit window"
)]
use super::conflict;
use super::hidpp_channel::{Channel, Gone, Read};
use super::hook::DeviceSink;
use powerkit_app::controller::power_mouse::link::{self, Wanted};
use powerkit_core::power_mouse::Diversion;
use powerkit_core::power_mouse::device::{
    AdapterLink, AdapterStatus, DeviceAction, DeviceSettings,
};
use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND},
    System::Threading::{CreateEventW, SetEvent, WaitForSingleObject},
};

/// How long to wait before trying an unreachable or contested device again.
const RETRY: Duration = Duration::from_secs(10);
/// Without events, look at the battery and at rival applications this often.
const TICK: Duration = Duration::from_secs(120);

static STATUS: Mutex<BTreeMap<String, AdapterStatus>> = Mutex::new(BTreeMap::new());

/// Status of every running adapter by device key.
pub fn statuses() -> BTreeMap<String, AdapterStatus> {
    STATUS
        .lock()
        .map(|statuses| statuses.clone())
        .unwrap_or_default()
}

/// Command senders of the running adapters, so an action fired on the
/// injector thread can reach the devices without a detour through the UI.
static ROUTES: Mutex<BTreeMap<String, Route>> = Mutex::new(BTreeMap::new());

/// How to reach one adapter thread: its command queue and its wake event.
type Route = (Sender<Command>, Arc<Wake>);

/// Asks every connected device to carry out a device action. A device that
/// lacks the feature ignores it; with no adapter running nothing happens.
pub fn act(action: DeviceAction) {
    if let Ok(routes) = ROUTES.lock() {
        for (commands, wake) in routes.values() {
            if commands.send(Command::Act(action)).is_ok() {
                wake.signal();
            }
        }
    }
}

pub enum Command {
    /// The device controls to divert from now on.
    Divert(Vec<Diversion>),
    /// The settings to keep on the device from now on.
    Settings(DeviceSettings),
    /// A device action fired by a binding.
    Act(DeviceAction),
    /// An elevated window came to the foreground or left it.
    Elevated(bool),
    Stop,
}

/// Auto-reset event that interrupts the adapter thread's wait for a report.
/// The handle is shared by the UI thread (signal) and the adapter thread
/// (wait) and closed when the last owner drops it; event handles are safe
/// to use from several threads.
struct Wake(isize);

impl Wake {
    fn new() -> Option<Self> {
        // SAFETY: unnamed auto-reset event, initially not signaled.
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        (!event.is_null()).then_some(Self(event as isize))
    }

    fn handle(&self) -> HANDLE {
        self.0 as HANDLE
    }

    fn signal(&self) {
        // SAFETY: the handle is live while self exists.
        unsafe {
            SetEvent(self.handle());
        }
    }

    /// Waits for a signal; `false` on timeout.
    fn wait(&self, timeout: Duration) -> bool {
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        // SAFETY: the handle is live while self exists.
        unsafe { WaitForSingleObject(self.handle(), millis) == 0 }
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        // SAFETY: closed once, by the last owner.
        unsafe {
            CloseHandle(self.handle());
        }
    }
}

pub struct Adapter {
    key: String,
    commands: Sender<Command>,
    wake: Arc<Wake>,
    join: Option<JoinHandle<()>>,
}

/// What an adapter thread needs from the session that starts it.
pub struct Start {
    pub app: HWND,
    pub key: String,
    pub path: String,
    pub sink: DeviceSink,
    pub diversions: Vec<Diversion>,
    pub settings: DeviceSettings,
    pub elevated: bool,
}

impl Adapter {
    pub fn start(start: Start) -> Option<Self> {
        let wake = Arc::new(Wake::new()?);
        let (commands, inbox) = channel();
        let key = start.key.clone();
        let worker = Worker {
            app: start.app as isize,
            key: start.key,
            path: start.path,
            sink: start.sink,
            wake: Arc::clone(&wake),
            inbox,
            wanted: Wanted {
                diversions: start.diversions,
                settings: start.settings,
                elevated: start.elevated,
            },
            actions: Vec::new(),
        };
        let join = thread::Builder::new()
            .name("powerkit-mouse-adapter".into())
            .spawn(move || worker.run())
            .ok()?;
        if let Ok(mut routes) = ROUTES.lock() {
            routes.insert(key.clone(), (commands.clone(), Arc::clone(&wake)));
        }
        Some(Self {
            key,
            commands,
            wake,
            join: Some(join),
        })
    }

    pub fn send(&self, command: Command) {
        if self.commands.send(command).is_ok() {
            self.wake.signal();
        }
    }
}

impl Drop for Adapter {
    /// Stops the thread, which undiverts what it diverted and releases any
    /// control still held, then forgets the device's status. This joins on
    /// the calling (UI) thread; the thread bounds its own cleanup to one
    /// request timeout when the device no longer answers.
    fn drop(&mut self) {
        if let Ok(mut routes) = ROUTES.lock() {
            routes.remove(&self.key);
        }
        self.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        if let Ok(mut statuses) = STATUS.lock() {
            statuses.remove(&self.key);
        }
    }
}

struct Worker {
    app: isize,
    key: String,
    path: String,
    sink: DeviceSink,
    wake: Arc<Wake>,
    inbox: Receiver<Command>,
    wanted: Wanted,
    /// Device actions received but not carried out yet.
    actions: Vec<DeviceAction>,
}

/// An open, identified device on this thread's channel.
type Link = link::Link<Channel>;

impl Worker {
    fn run(mut self) {
        let sink = self.sink.clone();
        let mut emit = move |input| sink.send(input);
        loop {
            if !self.drain() {
                return;
            }
            // An action fired while no device was connected is dropped; it
            // must not surprise the user when the device comes back.
            self.actions.clear();
            let outcome = match conflict::running() {
                Some(rival) => {
                    self.publish(AdapterStatus::new(AdapterLink::Conflict(rival)));
                    None
                }
                None => {
                    // Only the first attempt shows "connecting"; retries keep
                    // the last status instead of flickering every few seconds.
                    if !self.has_status() {
                        self.publish(AdapterStatus::new(AdapterLink::Connecting));
                    }
                    Channel::open(&self.path).ok().and_then(Link::connect)
                }
            };
            match outcome {
                Some(mut link) => {
                    let stop = self.serve(&mut link, &mut emit).unwrap_or(false);
                    link.release_held(&mut emit);
                    if stop {
                        // Hand the controls back to the device before leaving.
                        link.give_back();
                        return;
                    }
                }
                None if conflict::running().is_none() => {
                    self.publish(AdapterStatus::new(AdapterLink::Unreachable));
                }
                None => {}
            }
            // Wait before the next attempt; a command ends the wait early.
            self.wake.wait(RETRY);
        }
    }

    /// Applies queued commands; `false` means stop.
    fn drain(&mut self) -> bool {
        loop {
            match self.inbox.try_recv() {
                Ok(Command::Divert(wanted)) => self.wanted.diversions = wanted,
                Ok(Command::Settings(settings)) => self.wanted.settings = settings,
                Ok(Command::Act(action)) => self.actions.push(action),
                Ok(Command::Elevated(elevated)) => self.wanted.elevated = elevated,
                Ok(Command::Stop) | Err(TryRecvError::Disconnected) => return false,
                Err(TryRecvError::Empty) => return true,
            }
        }
    }

    /// Runs the connected device until it is lost (`Err`, or `Ok(false)`
    /// when a rival application took over) or the adapter is told to stop
    /// (`Ok(true)`).
    fn serve(&mut self, link: &mut Link, emit: link::Emit) -> Result<bool, Gone> {
        link.apply(&self.wanted, std::mem::take(&mut self.actions), emit)?;
        self.publish(link.status());
        loop {
            let read = link
                .channel()
                .read(TICK, Some(self.wake.handle()))
                .map_err(|_| Gone)?;
            let changed = match read {
                Read::Message(message) => link.on_message(&self.wanted, &message, emit)?,
                Read::Other => false,
                Read::Woken => {
                    if !self.drain() {
                        return Ok(true);
                    }
                    link.apply(&self.wanted, std::mem::take(&mut self.actions), emit)?
                }
                Read::Timeout => {
                    if conflict::running().is_some() {
                        link.give_back();
                        return Ok(false);
                    }
                    link.refresh_battery()?
                }
            };
            if changed {
                self.publish(link.status());
            }
            // Events that arrived while a request waited for its reply.
            for message in link.channel().take_events() {
                if link.on_message(&self.wanted, &message, emit)? {
                    self.publish(link.status());
                }
            }
        }
    }

    fn has_status(&self) -> bool {
        STATUS
            .lock()
            .is_ok_and(|statuses| statuses.contains_key(&self.key))
    }

    /// Stores the status and tells the UI thread when it changed.
    fn publish(&self, status: AdapterStatus) {
        let Ok(mut statuses) = STATUS.lock() else {
            return;
        };
        if statuses.get(&self.key) == Some(&status) {
            return;
        }
        statuses.insert(self.key.clone(), status);
        drop(statuses);
        crate::events::post(
            self.app as HWND,
            powerkit_app::event::Event::MouseDevicesChanged,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_interrupts_a_wait_and_resets_itself() {
        let wake = Wake::new().unwrap();
        assert!(!wake.wait(Duration::from_millis(1)));
        wake.signal();
        assert!(wake.wait(Duration::from_millis(1)));
        assert!(!wake.wait(Duration::from_millis(1)), "auto-reset");
    }
}
