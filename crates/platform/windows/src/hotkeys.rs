//! Global shortcuts on Windows: the `RegisterHotKey` port for the
//! `powerkit_app` registry, and the registry itself on the app window's thread.
//! `WM_HOTKEY` carries the id this module gave a slot.
use super::keys;
use powerkit_app::controller::hotkeys::{Problem, Registry, Rejected, Slot};
use powerkit_app::ports::Hotkeys;
use powerkit_core::hotkey::Hotkey;
use powerkit_core::settings::Preferences;
use std::{cell::RefCell, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey},
};

struct Keys {
    /// The window that receives `WM_HOTKEY`; null until [`listen`] and in the
    /// self-test, which never registers a shortcut.
    app: HWND,
    registry: Registry,
}
thread_local! {
    static KEYS: RefCell<Keys> = const {
        RefCell::new(Keys {
            app: null_mut(),
            registry: Registry::new(),
        })
    };
}

/// `RegisterHotKey` ids, unique within the app window.
fn id(slot: Slot) -> i32 {
    match slot {
        Slot::Pin => 1,
        Slot::OpacityUp => 2,
        Slot::OpacityDown => 3,
        Slot::Ruler => 4,
        Slot::AlwaysOn => 5,
    }
}
/// The slot a `WM_HOTKEY` id belongs to.
pub fn slot(hotkey_id: i32) -> Option<Slot> {
    Slot::ALL.into_iter().find(|slot| id(*slot) == hotkey_id)
}

struct Port(HWND);
impl Hotkeys for Port {
    fn register(&mut self, slot: Slot, chord: Hotkey) -> bool {
        // SAFETY: self.0 is the live app window that receives WM_HOTKEY on this thread.
        unsafe {
            RegisterHotKey(
                self.0,
                id(slot),
                keys::register_mods(chord),
                u32::from(keys::vk(chord.key)),
            ) != 0
        }
    }
    fn unregister(&mut self, slot: Slot) {
        // SAFETY: unregistering an id this window does not hold is harmless.
        unsafe { UnregisterHotKey(self.0, id(slot)) };
    }
}

fn with<R>(f: impl FnOnce(&mut Keys) -> R) -> R {
    KEYS.with(|keys| f(&mut keys.borrow_mut()))
}

/// Names the window that owns the shortcuts. Nothing is registered before this.
pub fn listen(app: HWND) {
    with(|k| k.app = app);
}
/// Registers what `prefs` ask for; a tool that is off holds no chord.
pub fn sync(prefs: &Preferences) {
    with(|k| {
        if !k.app.is_null() {
            k.registry.sync(&mut Port(k.app), prefs);
        }
    });
}
/// Releases every shortcut while a new chord is recorded.
pub fn pause() {
    with(|k| {
        if !k.app.is_null() {
            k.registry.pause(&mut Port(k.app));
        }
    });
}
/// Releases every shortcut and forgets the window.
pub fn shutdown() {
    pause();
    with(|k| k.app = null_mut());
}
/// Label of the chord that is registered for `slot`, if any.
pub fn active_label(slot: Slot) -> Option<String> {
    with(|k| {
        k.registry
            .active(slot)
            .map(|chord| chord.label(super::keys::META_KEY))
    })
}
/// The registered chord's label, or the stored one while nothing is registered.
pub fn label(slot: Slot, prefs: &Preferences) -> String {
    with(|k| k.registry.label(slot, prefs, super::keys::META_KEY))
}
pub fn problems(
    prefs: &Preferences,
    rejected: Option<Rejected>,
    quiet: bool,
) -> Vec<(Slot, Problem)> {
    with(|k| powerkit_app::controller::hotkeys::problems(prefs, &k.registry, rejected, quiet))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_ids_are_distinct_and_map_back_to_their_slot() {
        for slot in Slot::ALL {
            assert_eq!(super::slot(id(slot)), Some(slot));
        }
        assert_eq!(super::slot(0), None);
        assert_eq!(super::slot(6), None);
    }
}
