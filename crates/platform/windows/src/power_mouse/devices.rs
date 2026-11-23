//! Keeps the list of connected adapter-capable devices current. Discovery
//! runs on its own thread, never on the UI thread; the UI thread only reads
//! the published list and is told when it changed.
use super::discovery;
use powerkit_core::power_mouse::device::{self, Device};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;

/// Windows reports one device arrival as a burst of changes; scanning waits
/// until the burst has been quiet this long.
const SETTLE: Duration = Duration::from_millis(400);

static CONNECTED: Mutex<Vec<Device>> = Mutex::new(Vec::new());

/// The devices found by the last scan; empty while the watch is stopped.
pub fn connected() -> Vec<Device> {
    CONNECTED
        .lock()
        .map(|devices| devices.clone())
        .unwrap_or_default()
}

enum Request {
    Rescan,
    Stop,
}

pub struct Watch {
    requests: Sender<Request>,
    join: Option<JoinHandle<()>>,
}

impl Watch {
    /// Starts scanning in the background. `None` when the thread cannot be
    /// created; PowerMouse then runs without a device list.
    pub fn start(app: HWND) -> Option<Self> {
        let app = app as isize;
        let (requests, inbox) = channel();
        let join = thread::Builder::new()
            .name("powerkit-mouse-devices".into())
            .spawn(move || run(app, &inbox))
            .ok()?;
        Some(Self {
            requests,
            join: Some(join),
        })
    }

    /// Asks for a new scan after a device arrived or left.
    pub fn rescan(&self) {
        let _ = self.requests.send(Request::Rescan);
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        publish(Vec::new());
    }
}

fn run(app: isize, inbox: &Receiver<Request>) {
    loop {
        if publish(device::devices(&discovery::enumerate())) {
            crate::events::post(app as HWND, powerkit_app::event::Event::MouseDevicesChanged);
        }
        match inbox.recv() {
            Ok(Request::Rescan) => {}
            Ok(Request::Stop) | Err(_) => return,
        }
        if !settle(inbox) {
            return;
        }
    }
}

/// Waits for a burst of rescan requests to end. `false` means stop.
fn settle(inbox: &Receiver<Request>) -> bool {
    loop {
        match inbox.recv_timeout(SETTLE) {
            Ok(Request::Rescan) => {}
            Ok(Request::Stop) | Err(RecvTimeoutError::Disconnected) => return false,
            Err(RecvTimeoutError::Timeout) => return true,
        }
    }
}

/// Stores the list; `true` when it differs from the previous one.
fn publish(devices: Vec<Device>) -> bool {
    let Ok(mut connected) = CONNECTED.lock() else {
        return false;
    };
    if *connected == devices {
        return false;
    }
    *connected = devices;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_of_requests_settles_into_one_scan_and_stop_wins() {
        let (requests, inbox) = channel();
        for _ in 0..5 {
            requests.send(Request::Rescan).unwrap();
        }
        assert!(settle(&inbox), "quiet after the burst: scan once");
        requests.send(Request::Rescan).unwrap();
        requests.send(Request::Stop).unwrap();
        assert!(!settle(&inbox));
        drop(requests);
        assert!(!settle(&inbox), "a dropped sender also stops the thread");
    }
}
