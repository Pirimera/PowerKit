//! AlwaysTop controller: which window a pin command acts on, whether it pins
//! or unpins, and how opacity steps.
//!
//! The platform owns the pins themselves (keeping a window on top, its frame,
//! its transparency) behind [`PinnableWindows`]. This controller only decides.
//! It keeps no pin across launches and persists nothing: it remembers the last
//! window the user could pin, so a tray command still has a target after the
//! tray took the focus, and whether sound cues are wanted.
//!
//! Window events can arrive while a command is being carried out, so the
//! controller's state is in `Cell`s and every method takes `&self`.
use crate::ports::PinnableWindows;
use powerkit_core::always_top::{Pin, step_opacity};
use std::{cell::Cell, fmt, io};

/// An opaque top-level window handle. Only the platform gives it meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

/// A short sound that confirms a pin or opacity change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    PinOn,
    PinOff,
    OpacityUp,
    OpacityDown,
}

/// Why a pin command did nothing.
#[derive(Debug)]
pub enum Error {
    /// There is no window the command could pin.
    NoWindow,
    /// The platform could not pin or unpin the window.
    Platform(io::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoWindow => f.write_str("there is no window to pin"),
            Self::Platform(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Default)]
pub struct Controller {
    /// The last window the user focused that can be pinned.
    remembered: Cell<Option<WindowId>>,
    sound: Cell<bool>,
}
impl Controller {
    pub const fn new() -> Self {
        Self {
            remembered: Cell::new(None),
            sound: Cell::new(false),
        }
    }

    /// Whether pin and opacity changes play a cue.
    pub fn set_sound(&self, enabled: bool) {
        self.sound.set(enabled);
    }

    /// Nothing, one window by title, or a count.
    pub fn status(&self, host: &impl PinnableWindows) -> Pin {
        let pins = host.pinned();
        let last = pins.last().map(|pin| host.title(*pin)).unwrap_or_default();
        Pin::summary(pins.len(), last)
    }

    /// Remembers `window` as the pin target when it can be pinned.
    pub fn offer(&self, host: &impl PinnableWindows, window: WindowId) -> bool {
        if !host.eligible(window) {
            return false;
        }
        self.remembered.set(Some(window));
        true
    }

    pub fn forget(&self) {
        self.remembered.set(None);
    }

    /// The window a tray command acts on: the remembered one while it can
    /// still be pinned, else `candidate` (the window in front) when that can.
    pub fn target(
        &self,
        host: &impl PinnableWindows,
        candidate: Option<WindowId>,
    ) -> Option<WindowId> {
        if let Some(remembered) = self.remembered.get().filter(|w| host.eligible(*w)) {
            return Some(remembered);
        }
        candidate.filter(|window| self.offer(host, *window))
    }

    /// Drops pins whose window is gone. `true` when any was dropped.
    pub fn reconcile(&self, host: &mut impl PinnableWindows) -> bool {
        let dead: Vec<WindowId> = host
            .pinned()
            .into_iter()
            .filter(|pin| !host.is_live(*pin))
            .collect();
        for pin in &dead {
            let _ = host.unpin(*pin);
        }
        !dead.is_empty()
    }

    /// The pin shortcut: toggles the window in front. Over a PowerKit window
    /// the shortcut does nothing and is not an error; a PowerKit window is
    /// never pinned and the user did nothing wrong.
    pub fn toggle_shortcut(&self, host: &mut impl PinnableWindows) -> Result<Pin, Error> {
        match host.foreground() {
            Some(front) if host.is_own(front) => Ok(self.status(host)),
            front => self.toggle_window(host, front),
        }
    }

    /// The tray command: toggles the remembered window, or `candidate` when
    /// none is remembered. Dead pins are dropped first.
    pub fn toggle_from(
        &self,
        host: &mut impl PinnableWindows,
        candidate: Option<WindowId>,
    ) -> Result<Pin, Error> {
        self.reconcile(host);
        let target = self.target(host, candidate);
        self.toggle_window(host, target)
    }

    fn toggle_window(
        &self,
        host: &mut impl PinnableWindows,
        target: Option<WindowId>,
    ) -> Result<Pin, Error> {
        let target = target
            .filter(|window| host.eligible(*window))
            .ok_or(Error::NoWindow)?;
        if self.is_pinned(host, target) {
            host.unpin(target).map_err(Error::Platform)?;
            self.play(host, Cue::PinOff);
        } else {
            host.pin(target).map_err(Error::Platform)?;
            self.play(host, Cue::PinOn);
        }
        Ok(self.status(host))
    }

    /// The opacity shortcuts: steps the window in front when it is pinned.
    pub fn step_foreground_opacity(&self, host: &mut impl PinnableWindows, delta: i32) -> bool {
        match host.foreground() {
            Some(front) => self.step_opacity(host, front, delta),
            None => false,
        }
    }

    /// The tray opacity items: the window the tray pin item targets when it is
    /// pinned, otherwise the most recently pinned window, so the tray agrees
    /// with the shortcuts.
    pub fn step_tray_opacity(&self, host: &mut impl PinnableWindows, delta: i32) -> bool {
        match tray_opacity_target(self.remembered.get(), &host.pinned()) {
            Some(target) => self.step_opacity(host, target, delta),
            None => false,
        }
    }

    /// Steps a pinned window's opacity. `false` when it is not pinned, already
    /// at the limit, or could not be changed.
    pub fn step_opacity(
        &self,
        host: &mut impl PinnableWindows,
        window: WindowId,
        delta: i32,
    ) -> bool {
        if !self.is_pinned(host, window) {
            return false;
        }
        let current = host.opacity(window);
        let next = step_opacity(current, delta);
        if next == current || host.set_opacity(window, next).is_err() {
            return false;
        }
        self.play(
            host,
            if delta > 0 {
                Cue::OpacityUp
            } else {
                Cue::OpacityDown
            },
        );
        true
    }

    /// Unpins every window, even after one fails, and reports the first failure.
    pub fn release(&self, host: &mut impl PinnableWindows) -> io::Result<()> {
        let mut first = Ok(());
        for pin in host.pinned() {
            if let Err(error) = host.unpin(pin)
                && first.is_ok()
            {
                first = Err(error);
            }
        }
        first
    }

    fn is_pinned(&self, host: &impl PinnableWindows, window: WindowId) -> bool {
        host.is_live(window) && host.pinned().contains(&window)
    }

    fn play(&self, host: &mut impl PinnableWindows, cue: Cue) {
        if self.sound.get() {
            host.play(cue);
        }
    }
}

/// `remembered` when it is one of `pins`, else the last pin; `None` without pins.
fn tray_opacity_target(remembered: Option<WindowId>, pins: &[WindowId]) -> Option<WindowId> {
    remembered
        .filter(|window| pins.contains(window))
        .or_else(|| pins.last().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeWindows;
    use powerkit_core::always_top::{MAX_OPACITY, MIN_OPACITY, OPACITY_STEP};

    const NOTEPAD: WindowId = WindowId(8);
    const EDITOR: WindowId = WindowId(16);
    const SETTINGS: WindowId = WindowId(24);
    const DESKTOP: WindowId = WindowId(32);

    /// Notepad and an editor can be pinned; the settings window is PowerKit's
    /// own and the desktop is a shell surface.
    fn desktop() -> FakeWindows {
        let mut host = FakeWindows::default();
        host.add(NOTEPAD, "Notepad");
        host.add(EDITOR, "Editor");
        host.add_own(SETTINGS);
        host.add_ineligible(DESKTOP);
        host
    }

    #[test]
    fn the_shortcut_toggles_the_window_in_front() {
        let mut host = desktop();
        let top = Controller::new();
        host.front = Some(NOTEPAD);
        assert_eq!(top.toggle_shortcut(&mut host).unwrap(), Pin::on("Notepad"));
        assert_eq!(host.pins, [NOTEPAD]);
        // Pinning another window keeps the first.
        host.front = Some(EDITOR);
        assert_eq!(
            top.toggle_shortcut(&mut host).unwrap(),
            Pin::Several { count: 2 }
        );
        host.front = Some(NOTEPAD);
        assert_eq!(top.toggle_shortcut(&mut host).unwrap(), Pin::on("Editor"));
        assert_eq!(host.pins, [EDITOR]);
    }

    #[test]
    fn the_shortcut_over_a_powerkit_window_does_nothing_and_is_not_an_error() {
        let mut host = desktop();
        let top = Controller::new();
        host.front = Some(NOTEPAD);
        top.toggle_shortcut(&mut host).unwrap();
        host.front = Some(SETTINGS);
        assert_eq!(top.toggle_shortcut(&mut host).unwrap(), Pin::on("Notepad"));
        assert_eq!(host.pins, [NOTEPAD]);
        // Other windows that cannot be pinned, and no window at all, explain why.
        host.front = Some(DESKTOP);
        assert!(matches!(
            top.toggle_shortcut(&mut host),
            Err(Error::NoWindow)
        ));
        host.front = None;
        assert!(matches!(
            top.toggle_shortcut(&mut host),
            Err(Error::NoWindow)
        ));
        assert_eq!(host.pins, [NOTEPAD]);
    }

    #[test]
    fn the_tray_acts_on_the_remembered_window_not_on_what_is_in_front() {
        let mut host = desktop();
        let top = Controller::new();
        assert!(top.offer(&host, NOTEPAD));
        // The tray menu is in front now.
        assert_eq!(
            top.toggle_from(&mut host, Some(DESKTOP)).unwrap(),
            Pin::on("Notepad")
        );
        // An ineligible window never replaces the remembered one.
        assert!(!top.offer(&host, SETTINGS));
        assert!(!top.offer(&host, DESKTOP));
        assert_eq!(top.target(&host, Some(EDITOR)), Some(NOTEPAD));
        assert_eq!(top.toggle_from(&mut host, Some(EDITOR)).unwrap(), Pin::Off);
    }

    #[test]
    fn without_a_remembered_window_the_tray_uses_an_eligible_candidate_or_fails() {
        let mut host = desktop();
        let top = Controller::new();
        assert_eq!(top.target(&host, None), None);
        assert_eq!(top.target(&host, Some(DESKTOP)), None);
        assert!(matches!(
            top.toggle_from(&mut host, None),
            Err(Error::NoWindow)
        ));
        assert!(host.pins.is_empty());
        assert_eq!(top.status(&host), Pin::Off);
        // The candidate becomes the remembered window.
        assert_eq!(top.target(&host, Some(EDITOR)), Some(EDITOR));
        assert_eq!(top.target(&host, None), Some(EDITOR));
        // A remembered window that closed is not a target any more.
        host.close(EDITOR);
        assert_eq!(top.target(&host, None), None);
        top.offer(&host, NOTEPAD);
        top.forget();
        assert_eq!(top.target(&host, None), None);
    }

    #[test]
    fn closed_windows_are_dropped_before_a_tray_command_and_by_reconcile() {
        let mut host = desktop();
        let top = Controller::new();
        host.front = Some(NOTEPAD);
        top.toggle_shortcut(&mut host).unwrap();
        host.front = Some(EDITOR);
        top.toggle_shortcut(&mut host).unwrap();
        assert!(!top.reconcile(&mut host));
        host.close(NOTEPAD);
        assert!(top.reconcile(&mut host));
        assert_eq!(host.pins, [EDITOR]);
        assert_eq!(top.status(&host), Pin::on("Editor"));
        host.close(EDITOR);
        top.offer(&host, NOTEPAD);
        assert!(matches!(
            top.toggle_from(&mut host, None),
            Err(Error::NoWindow)
        ));
        assert!(host.pins.is_empty());
    }

    #[test]
    fn a_failed_pin_or_unpin_is_reported_and_changes_nothing() {
        let mut host = desktop();
        let top = Controller::new();
        top.set_sound(true);
        host.front = Some(NOTEPAD);
        host.fail = true;
        assert!(matches!(
            top.toggle_shortcut(&mut host),
            Err(Error::Platform(_))
        ));
        assert!(host.pins.is_empty() && host.cues.is_empty());
        host.fail = false;
        top.toggle_shortcut(&mut host).unwrap();
        host.fail = true;
        assert!(matches!(
            top.toggle_shortcut(&mut host),
            Err(Error::Platform(_))
        ));
        assert_eq!(host.pins, [NOTEPAD]);
        assert_eq!(host.cues, [Cue::PinOn]);
    }

    #[test]
    fn cues_follow_the_change_and_only_while_sound_is_on() {
        let mut host = desktop();
        let top = Controller::new();
        host.front = Some(NOTEPAD);
        top.toggle_shortcut(&mut host).unwrap();
        assert!(top.step_foreground_opacity(&mut host, -OPACITY_STEP));
        assert!(host.cues.is_empty());
        top.set_sound(true);
        assert!(top.step_foreground_opacity(&mut host, -OPACITY_STEP));
        assert!(top.step_foreground_opacity(&mut host, OPACITY_STEP));
        top.toggle_shortcut(&mut host).unwrap();
        top.toggle_shortcut(&mut host).unwrap();
        assert_eq!(
            host.cues,
            [Cue::OpacityDown, Cue::OpacityUp, Cue::PinOff, Cue::PinOn]
        );
    }

    #[test]
    fn opacity_steps_only_pinned_windows_and_stops_at_the_limits() {
        let mut host = desktop();
        let top = Controller::new();
        top.set_sound(true);
        host.front = Some(NOTEPAD);
        // Not pinned yet.
        assert!(!top.step_foreground_opacity(&mut host, -OPACITY_STEP));
        top.toggle_shortcut(&mut host).unwrap();
        host.cues.clear();
        // Already fully opaque: no change, no cue.
        assert!(!top.step_foreground_opacity(&mut host, OPACITY_STEP));
        assert!(host.cues.is_empty());
        let mut steps = 0;
        while top.step_foreground_opacity(&mut host, -OPACITY_STEP) {
            steps += 1;
        }
        assert_eq!(host.opacity_of(NOTEPAD), MIN_OPACITY);
        assert_eq!(steps, (MAX_OPACITY - MIN_OPACITY) / OPACITY_STEP);
        // A failed change reports false and plays nothing.
        host.cues.clear();
        host.fail = true;
        assert!(!top.step_foreground_opacity(&mut host, OPACITY_STEP));
        assert!(host.cues.is_empty());
        host.fail = false;
        host.front = None;
        assert!(!top.step_foreground_opacity(&mut host, OPACITY_STEP));
    }

    #[test]
    fn tray_opacity_follows_the_remembered_pin_then_the_last_pin() {
        let (a, b, other) = (WindowId(1), WindowId(2), WindowId(3));
        assert_eq!(tray_opacity_target(Some(a), &[a, b]), Some(a));
        assert_eq!(tray_opacity_target(Some(other), &[a, b]), Some(b));
        assert_eq!(tray_opacity_target(None, &[a, b]), Some(b));
        assert_eq!(tray_opacity_target(Some(a), &[]), None);

        let mut host = desktop();
        let top = Controller::new();
        assert!(!top.step_tray_opacity(&mut host, -OPACITY_STEP));
        host.front = Some(NOTEPAD);
        top.toggle_shortcut(&mut host).unwrap();
        host.front = Some(EDITOR);
        top.toggle_shortcut(&mut host).unwrap();
        // Notepad was focused last before the tray opened.
        top.offer(&host, NOTEPAD);
        assert!(top.step_tray_opacity(&mut host, -OPACITY_STEP));
        assert_eq!(host.opacity_of(NOTEPAD), MAX_OPACITY - OPACITY_STEP);
        assert_eq!(host.opacity_of(EDITOR), MAX_OPACITY);
        // The remembered window is not pinned: the most recent pin is stepped.
        top.forget();
        assert!(top.step_tray_opacity(&mut host, -OPACITY_STEP));
        assert_eq!(host.opacity_of(EDITOR), MAX_OPACITY - OPACITY_STEP);
    }

    #[test]
    fn release_unpins_everything_and_reports_the_first_failure() {
        let mut host = desktop();
        let top = Controller::new();
        for window in [NOTEPAD, EDITOR] {
            host.front = Some(window);
            top.toggle_shortcut(&mut host).unwrap();
        }
        host.fail_unpin = Some(NOTEPAD);
        assert!(top.release(&mut host).is_err());
        // The window that failed is still pinned; the other was released.
        assert_eq!(host.pins, [NOTEPAD]);
        host.fail_unpin = None;
        top.release(&mut host).unwrap();
        assert_eq!(top.status(&host), Pin::Off);
    }
}
