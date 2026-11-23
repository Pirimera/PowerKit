//! PowerMouse Windows adapter: hook thread, injector, elevation watch,
//! discovery of devices a vendor adapter can drive and the Logitech adapter.
mod adapter;
pub mod assets;
mod conflict;
mod devices;
mod discovery;
mod elevation;
mod hidpp_channel;
mod hook;
mod inject;
#[cfg(test)]
mod probes;
mod ring_window;

use adapter::Adapter;
use hook::Hook;
use inject::Injector;
use powerkit_core::power_mouse::Preferences;
use powerkit_core::power_mouse::device::AdapterStatus;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use windows_sys::Win32::Foundation::HWND;

thread_local! {
    static SESSION: RefCell<Session> = const { RefCell::new(Session::new()) };
}

struct Running {
    hook: Hook,
    injector: Injector,
    _elevation: elevation::Watch,
    /// Absent when its thread could not start; remapping works without it.
    devices: Option<devices::Watch>,
    /// One adapter per connected device it may drive, by device key.
    adapters: BTreeMap<String, Adapter>,
}

struct Session {
    app: isize,
    prefs: Preferences,
    running: Option<Running>,
    suspended: bool,
    /// Whether this Windows session is locked or not attached to a display.
    presence: SessionPresence,
    /// Devices whose low battery the user was already told about.
    low_notified: BTreeSet<String>,
}

/// What a session-change notification (`WM_WTSSESSION_CHANGE`) reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChange {
    Locked,
    Unlocked,
    Disconnected,
    Connected,
}

/// Whether the user is at this session. A lock and a disconnect are tracked
/// apart because fast user switching sends both, in either order, and the
/// session is back only when it is unlocked *and* connected again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SessionPresence {
    locked: bool,
    disconnected: bool,
}

impl SessionPresence {
    fn apply(&mut self, change: SessionChange) {
        match change {
            SessionChange::Locked => self.locked = true,
            SessionChange::Unlocked => self.locked = false,
            SessionChange::Disconnected => self.disconnected = true,
            SessionChange::Connected => self.disconnected = false,
        }
    }

    fn away(self) -> bool {
        self.locked || self.disconnected
    }
}

impl Session {
    const fn new() -> Self {
        Self {
            app: 0,
            prefs: Preferences {
                enabled: false,
                bindings: std::collections::BTreeMap::new(),
                dpi: None,
                wheel_mode: None,
                hires_wheel: None,
            },
            running: None,
            suspended: false,
            presence: SessionPresence {
                locked: false,
                disconnected: false,
            },
            low_notified: BTreeSet::new(),
        }
    }
}

/// Bind the hidden app window and apply the current PowerMouse preferences.
/// Startup applies the preferences before the window is bound, when nothing
/// can start yet, so this is where PowerMouse starts after PowerKit launches.
pub fn listen(app: HWND) {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.app = app as isize;
        reconcile(&mut session);
    });
}

/// The session was locked, unlocked, disconnected or reconnected. While the
/// user is away PowerMouse stops entirely: its hooks have nothing to remap on
/// the lock screen, and its device adapters give the Logitech buttons back,
/// so another user signed in on the same computer gets a working mouse.
pub fn on_session_change(change: SessionChange) {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.presence.apply(change);
        reconcile(&mut session);
    });
}

pub fn apply_prefs(prefs: &powerkit_core::settings::Preferences) {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.prefs = prefs.mouse.clone();
        reconcile(&mut session);
    });
}

pub fn elevated_foreground() -> bool {
    elevation::is_elevated_foreground()
}

/// Connected devices a vendor adapter exists for; empty while PowerMouse is
/// off. Read on the UI thread from the list the discovery thread publishes.
pub fn connected_devices() -> Vec<powerkit_core::power_mouse::device::Device> {
    devices::connected()
}

/// What each running adapter knows about its device, by device key.
pub fn adapter_statuses() -> BTreeMap<String, AdapterStatus> {
    adapter::statuses()
}

/// The discovery thread or an adapter published a change: start adapters for
/// devices that arrived and stop them for devices that left. Returns the
/// devices whose battery just became low, as name and percent, each once
/// until its battery recovers.
pub fn on_devices_published() -> Vec<(String, u8)> {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        let connected = devices::connected();
        if !connected.is_empty() && session.app != 0 {
            assets::prefetch(&connected, session.app as HWND);
        }
        sync_adapters(&mut session);
        let statuses = adapter::statuses();
        session
            .low_notified
            .retain(|key| statuses.get(key).is_some_and(AdapterStatus::battery_low));
        let mut notices = Vec::new();
        for (key, status) in &statuses {
            if status.battery_low()
                && session.low_notified.insert(key.clone())
                && let Some(battery) = status.battery
            {
                notices.push((status.name.clone(), battery.percent));
            }
        }
        notices
    })
}

/// The "Show Actions Ring" action fired: open the ring around the cursor,
/// or close it when it is already open. `labels` names each slot's action;
/// `None` is an empty slot. Nothing opens while PowerMouse is not running or
/// an elevated window is in front, where the chosen action could not act.
pub fn toggle_ring(labels: [Option<String>; powerkit_core::power_mouse::ring::SLOTS]) {
    if ring_window::is_open() {
        ring_window::close();
        return;
    }
    let running = SESSION.with(|session| session.borrow().running.is_some());
    if running && !elevation::is_elevated_foreground() {
        ring_window::open(labels);
    }
}

/// A ring slot was chosen: press and release its trigger, so the slot's
/// binding runs through the same engine as every other trigger.
fn ring_chosen(slot: usize) {
    let Some(trigger) = powerkit_core::power_mouse::Trigger::RING.get(slot).copied() else {
        return;
    };
    SESSION.with(|session| {
        if let Ok(session) = session.try_borrow()
            && let Some(running) = session.running.as_ref()
        {
            let sink = running.hook.device_sink();
            sink.send(hook::DeviceInput::Press(trigger));
            sink.send(hook::DeviceInput::Release(trigger));
        }
    });
}

/// An elevated window came to the foreground or left it. Runs on the UI
/// thread, from the foreground WinEvent hook; the session is skipped when it
/// is busy, and the adapters then learn the state on their next start.
fn elevation_changed(elevated: bool) {
    SESSION.with(|session| {
        if let Ok(session) = session.try_borrow()
            && let Some(running) = session.running.as_ref()
        {
            for adapter in running.adapters.values() {
                adapter.send(adapter::Command::Elevated(elevated));
            }
        }
    });
}

/// Keeps one adapter per connected device that may be driven and tells each
/// what to divert.
fn sync_adapters(session: &mut Session) {
    let app = session.app as HWND;
    let diversions = session.prefs.diversions();
    let settings = session.prefs.device_settings();
    let Some(running) = session.running.as_mut() else {
        return;
    };
    let ready: Vec<_> = devices::connected()
        .into_iter()
        .filter(powerkit_core::power_mouse::device::Device::adapter_ready)
        .collect();
    running
        .adapters
        .retain(|key, _| ready.iter().any(|device| &device.key == key));
    for device in ready {
        if running.adapters.contains_key(&device.key) {
            continue;
        }
        let started = Adapter::start(adapter::Start {
            app,
            key: device.key.clone(),
            path: device.channel_path,
            sink: running.hook.device_sink(),
            diversions: diversions.clone(),
            settings,
            elevated: elevation::is_elevated_foreground(),
        });
        if let Some(adapter) = started {
            running.adapters.insert(device.key, adapter);
        }
    }
}

/// A device arrived or left (WM_DEVICECHANGE): scan again off the UI thread.
pub fn on_devices_changed() {
    SESSION.with(|session| {
        if let Some(watch) = session
            .borrow()
            .running
            .as_ref()
            .and_then(|running| running.devices.as_ref())
        {
            watch.rescan();
        }
    });
}

pub fn on_suspend() {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.suspended = true;
        stop_running(&mut session);
    });
}

pub fn on_resume() {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.suspended = false;
        reconcile(&mut session);
    });
}

pub fn shutdown() {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        stop_running(&mut session);
        session.app = 0;
    });
}

fn reconcile(session: &mut Session) {
    if session.suspended || session.presence.away() || session.app == 0 || !session.prefs.enabled {
        stop_running(session);
        return;
    }
    if let Some(running) = session.running.as_ref() {
        running.hook.apply(session.prefs.clone());
        let diversions = session.prefs.diversions();
        let settings = session.prefs.device_settings();
        for adapter in running.adapters.values() {
            adapter.send(adapter::Command::Divert(diversions.clone()));
            adapter.send(adapter::Command::Settings(settings));
        }
        return;
    }
    let app = session.app as HWND;
    let injector = Injector::start(app);
    let inject_tx = injector.sender();
    let Some(hook) = Hook::start(session.prefs.clone(), inject_tx) else {
        injector.stop();
        return;
    };
    let elevation = elevation::Watch::start();
    hook.set_pass_all(elevation::is_elevated_foreground());
    session.running = Some(Running {
        hook,
        injector,
        _elevation: elevation,
        devices: devices::Watch::start(app),
        adapters: BTreeMap::new(),
    });
    // Devices found by an earlier run are still listed only if the watch
    // kept running; normally the first scan result starts the adapters.
    sync_adapters(session);
}

fn stop_running(session: &mut Session) {
    ring_window::close();
    if let Some(running) = session.running.take() {
        // Adapters first: each undiverts its controls and releases any that
        // is held, and those releases still need the hook and the injector.
        drop(running.adapters);
        running.hook.stop();
        running.injector.stop();
    }
    session.low_notified.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use powerkit_core::power_mouse::{Action, Binding, Trigger};

    #[test]
    fn disabled_session_stays_idle_without_a_window() {
        SESSION.with(|session| {
            let mut session = session.borrow_mut();
            session.prefs.enabled = true;
            session
                .prefs
                .set_binding(Trigger::Middle, Binding::Single(Action::Copy));
            reconcile(&mut session);
            assert!(session.running.is_none());
            session.prefs.enabled = false;
            session.prefs.bindings.clear();
        });
    }

    #[test]
    fn fast_user_switching_is_back_only_when_unlocked_and_connected() {
        let mut presence = SessionPresence::default();
        assert!(!presence.away());
        // Switching away: Windows locks, then disconnects the console.
        presence.apply(SessionChange::Locked);
        presence.apply(SessionChange::Disconnected);
        assert!(presence.away());
        // Coming back connects first; the lock screen is still up.
        presence.apply(SessionChange::Connected);
        assert!(presence.away());
        presence.apply(SessionChange::Unlocked);
        assert!(!presence.away());
        // The other order, as a remote session may send it.
        presence.apply(SessionChange::Disconnected);
        presence.apply(SessionChange::Locked);
        presence.apply(SessionChange::Unlocked);
        assert!(presence.away(), "still disconnected");
        presence.apply(SessionChange::Connected);
        assert!(!presence.away());
    }

    #[test]
    fn a_locked_session_keeps_power_mouse_stopped() {
        SESSION.with(|session| {
            let mut session = session.borrow_mut();
            session.presence.apply(SessionChange::Locked);
            session.app = 1;
            session.prefs.enabled = true;
            reconcile(&mut session);
            assert!(session.running.is_none(), "nothing starts while locked");
            session.presence = SessionPresence::default();
            session.app = 0;
            session.prefs.enabled = false;
        });
    }
}
