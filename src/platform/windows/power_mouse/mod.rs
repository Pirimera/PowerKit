//! PowerMouse Windows adapter: hook thread, injector and elevation watch.
mod elevation;
mod hook;
mod inject;

use crate::power_mouse::Preferences;
use hook::Hook;
use inject::Injector;
use std::cell::RefCell;
use windows_sys::Win32::Foundation::HWND;

thread_local! {
    static SESSION: RefCell<Session> = const { RefCell::new(Session::new()) };
}

struct Running {
    hook: Hook,
    injector: Injector,
    _elevation: elevation::Watch,
}

struct Session {
    app: isize,
    prefs: Preferences,
    running: Option<Running>,
    suspended: bool,
}

impl Session {
    const fn new() -> Self {
        Self {
            app: 0,
            prefs: Preferences {
                enabled: false,
                bindings: std::collections::BTreeMap::new(),
            },
            running: None,
            suspended: false,
        }
    }
}

/// Bind the hidden app window and apply the current PowerMouse preferences.
pub(super) fn listen(app: HWND) {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.app = app as isize;
    });
}

pub(super) fn apply_prefs(prefs: &crate::settings::Preferences) {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.prefs = prefs.mouse.clone();
        reconcile(&mut session);
    });
}

pub(super) fn elevated_foreground() -> bool {
    elevation::is_elevated_foreground()
}

pub(super) fn on_suspend() {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.suspended = true;
        stop_running(&mut session);
    });
}

pub(super) fn on_resume() {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.suspended = false;
        reconcile(&mut session);
    });
}

pub(super) fn shutdown() {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        stop_running(&mut session);
        session.app = 0;
    });
}

fn reconcile(session: &mut Session) {
    if session.suspended || session.app == 0 || !session.prefs.enabled {
        stop_running(session);
        return;
    }
    if let Some(running) = session.running.as_ref() {
        running.hook.apply(session.prefs.clone());
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
    });
}

fn stop_running(session: &mut Session) {
    if let Some(running) = session.running.take() {
        running.hook.stop();
        running.injector.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_mouse::{Action, Binding, Trigger};

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
}
