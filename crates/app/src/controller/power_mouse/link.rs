//! One connected Logitech mouse: which of its controls are diverted, which
//! are held, and what its events mean for the button engine.
//!
//! A [`Link`] decides; the platform's adapter thread owns the waiting. The
//! thread opens the channel, reads reports from it, hands them to the link
//! and forwards the [`DeviceInput`] the link emits. Nothing here blocks
//! longer than one request on the channel.
use super::device_settings::Settings;
use super::{Gone, HidppChannel, logitech, settled};
use powerkit_core::power_mouse::device::{
    AdapterLink, AdapterStatus, DeviceAction, DeviceSettings,
};
use powerkit_core::power_mouse::hidpp::{
    self, Battery, Control, ControlEvent, DIRECT_DEVICE, Message, feature,
};
use powerkit_core::power_mouse::{Diversion, Trigger};
use std::collections::{BTreeMap, BTreeSet};

/// A device control event from a vendor adapter. It enters the same press
/// state machine as a hooked button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceInput {
    Press(Trigger),
    Release(Trigger),
    /// Movement the device reported while a raw-XY control is held.
    Move {
        dx: i32,
        dy: i32,
    },
}

/// What the session wants from a device right now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wanted {
    /// The device controls the bindings use.
    pub diversions: Vec<Diversion>,
    /// The settings to keep on the device.
    pub settings: DeviceSettings,
    /// An elevated window is in the foreground.
    pub elevated: bool,
}

/// Where a link's trigger input goes.
pub type Emit<'a> = &'a mut dyn FnMut(DeviceInput);

/// An open, identified device.
pub struct Link<C> {
    channel: C,
    extras: Settings,
    name: String,
    controls: Vec<Control>,
    controls_index: u8,
    battery_index: Option<u8>,
    reconnect_index: Option<u8>,
    battery: Option<Battery>,
    /// Control id → raw XY, for what this link has diverted.
    diverted: BTreeMap<u16, bool>,
    held: BTreeSet<Trigger>,
}

impl<C: HidppChannel> Link<C> {
    /// Identifies the device behind `channel`. `None` when it does not
    /// answer, is not a mouse or has no reprogrammable controls. Only reads:
    /// nothing is diverted or set until [`Self::apply`].
    pub fn connect(mut channel: C) -> Option<Self> {
        let identity = logitech::identify(&mut channel, DIRECT_DEVICE).ok()?;
        let controls_index = identity.features.controls?;
        if !identity.is_mouse {
            return None;
        }
        let reconnect_index =
            logitech::lookup(&mut channel, DIRECT_DEVICE, feature::WIRELESS_STATUS)
                .ok()
                .flatten();
        let extras = Settings::discover(&mut channel).ok()?;
        Some(Self {
            channel,
            extras,
            name: identity.name,
            controls: identity.controls,
            controls_index,
            battery_index: identity.features.battery,
            reconnect_index,
            battery: identity.battery,
            diverted: BTreeMap::new(),
            held: BTreeSet::new(),
        })
    }

    /// The channel, for the thread that waits on it.
    pub fn channel(&mut self) -> &mut C {
        &mut self.channel
    }

    /// Makes the device match `wanted` and carries out `actions`. `Ok(true)`
    /// means the page's view of the device (its DPI) changed.
    pub fn apply(
        &mut self,
        wanted: &Wanted,
        actions: Vec<DeviceAction>,
        emit: Emit,
    ) -> Result<bool, Gone> {
        self.sync(wanted, emit)?;
        self.configure(wanted, actions)
    }

    /// Makes the device's diversions match the bindings. While an elevated
    /// window is in front nothing stays diverted, so the controls keep their
    /// own function there instead of doing nothing.
    fn sync(&mut self, wanted: &Wanted, emit: Emit) -> Result<(), Gone> {
        let target = targets(&wanted.diversions, &self.controls, wanted.elevated);
        let stale: Vec<u16> = self
            .diverted
            .iter()
            .filter(|(control, raw)| target.get(control) != Some(raw))
            .map(|(control, _)| *control)
            .collect();
        for control in stale {
            self.set(control, false, true)?;
            self.diverted.remove(&control);
            if let Some(trigger) = Trigger::from_control_id(control)
                && self.held.remove(&trigger)
            {
                emit(DeviceInput::Release(trigger));
            }
        }
        for (control, raw) in target {
            if !self.diverted.contains_key(&control) && self.set(control, true, raw)? {
                self.diverted.insert(control, raw);
            }
        }
        Ok(())
    }

    /// Writes the device settings that changed, then carries out the device
    /// actions.
    fn configure(&mut self, wanted: &Wanted, actions: Vec<DeviceAction>) -> Result<bool, Gone> {
        let mut changed =
            self.extras
                .apply(&mut self.channel, &wanted.settings, wanted.elevated)?;
        for action in actions {
            changed |= self.extras.act(&mut self.channel, action)?;
        }
        Ok(changed)
    }

    /// Takes one report from the device. `Ok(true)` means the status changed.
    pub fn on_message(
        &mut self,
        wanted: &Wanted,
        message: &Message,
        emit: Emit,
    ) -> Result<bool, Gone> {
        if let Some(trigger) = self.extras.thumb_wheel_turn(message) {
            // A turn has no release; the engine takes a run of these as one
            // press per burst, like horizontal scroll.
            emit(DeviceInput::Press(trigger));
            return Ok(false);
        }
        if let Some(event) = ControlEvent::decode(message, self.controls_index) {
            match event {
                ControlEvent::Pressed(pressed) => {
                    let (released, newly, held) = press_changes(&self.held, pressed);
                    for trigger in released {
                        emit(DeviceInput::Release(trigger));
                    }
                    for trigger in newly {
                        emit(DeviceInput::Press(trigger));
                    }
                    self.held = held;
                }
                ControlEvent::Moved { dx, dy } => emit(DeviceInput::Move {
                    dx: i32::from(dx),
                    dy: i32::from(dy),
                }),
            }
            return Ok(false);
        }
        if !message.is_event() {
            return Ok(false);
        }
        if Some(message.feature_index) == self.battery_index && message.function == 0 {
            if let Some(battery) = Battery::from_status(&message.payload) {
                self.battery = Some(battery);
                return Ok(true);
            }
        } else if Some(message.feature_index) == self.reconnect_index {
            // The device came back from sleep or a power cycle and forgot its
            // diversions and settings: nothing is held any more, and both are
            // applied anew.
            self.release_held(emit);
            self.diverted.clear();
            self.extras.forget();
            let changed = self.apply(wanted, Vec::new(), emit)?;
            return Ok(self.refresh_battery()? || changed);
        }
        Ok(false)
    }

    /// Asks for the battery level. `Ok(true)` means it changed.
    pub fn refresh_battery(&mut self) -> Result<bool, Gone> {
        let Some(index) = self.battery_index else {
            return Ok(false);
        };
        // No answer means asleep: the next event or tick will find it again.
        let battery =
            settled(logitech::battery(&mut self.channel, DIRECT_DEVICE, index))?.flatten();
        if battery.is_some() && battery != self.battery {
            self.battery = battery;
            return Ok(true);
        }
        Ok(false)
    }

    /// Releases every control the engine still believes is held.
    pub fn release_held(&mut self, emit: Emit) {
        for trigger in std::mem::take(&mut self.held) {
            emit(DeviceInput::Release(trigger));
        }
    }

    /// Gives every diverted control, and the thumb wheel, back to the
    /// device. Stops at the first control that gets no answer: a device that
    /// is gone has forgotten them anyway, and the caller may be waiting on
    /// this thread. DPI and wheel settings are the user's choice and stay.
    pub fn give_back(&mut self) {
        for control in std::mem::take(&mut self.diverted).into_keys() {
            if !matches!(self.set(control, false, true), Ok(true)) {
                return;
            }
        }
        self.extras.restore(&mut self.channel);
    }

    /// What the page shows for this device.
    pub fn status(&self) -> AdapterStatus {
        AdapterStatus {
            link: AdapterLink::Active,
            name: self.name.clone(),
            battery: self.battery,
            controls: self
                .controls
                .iter()
                .filter(|control| control.divertable())
                .filter_map(|control| Trigger::from_control_id(control.id))
                .collect(),
            dpi: self.extras.dpi(),
            dpi_presets: self.extras.dpi_presets(),
            has_wheel_mode: self.extras.has_wheel_mode(),
            has_hires_wheel: self.extras.has_hires_wheel(),
            has_thumb_wheel: self.extras.has_thumb_wheel(),
        }
    }

    /// Sets one control's diversion. `Ok(false)` means the device refused or
    /// did not answer for this control; a failed channel is `Gone`.
    fn set(&mut self, control: u16, diverted: bool, raw_xy: bool) -> Result<bool, Gone> {
        let request = hidpp::set_diversion(
            DIRECT_DEVICE,
            self.controls_index,
            control,
            diverted,
            raw_xy,
        );
        Ok(settled(self.channel.request(&request))?.is_some())
    }
}

/// The diversions the device should have: every wanted control the device
/// has and can divert, with raw XY only where the control supports it, and
/// nothing at all while an elevated window is in the foreground.
fn targets(wanted: &[Diversion], controls: &[Control], elevated: bool) -> BTreeMap<u16, bool> {
    if elevated {
        return BTreeMap::new();
    }
    wanted
        .iter()
        .filter_map(|diversion| {
            let id = diversion.trigger.control_id()?;
            let control = controls
                .iter()
                .find(|control| control.id == id && control.divertable())?;
            Some((id, diversion.raw_xy && control.raw_xy()))
        })
        .collect()
}

/// Compares the controls a device reports as held with the previous report:
/// what was released, what was newly pressed, and the new held set. Controls
/// PowerMouse has no trigger for are ignored.
fn press_changes(
    held: &BTreeSet<Trigger>,
    pressed: [u16; 4],
) -> (Vec<Trigger>, Vec<Trigger>, BTreeSet<Trigger>) {
    let now: BTreeSet<Trigger> = pressed
        .into_iter()
        .filter_map(Trigger::from_control_id)
        .collect();
    let released = held.difference(&now).copied().collect();
    let newly = now.difference(held).copied().collect();
    (released, newly, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeMouse;
    use powerkit_core::power_mouse::hidpp::{PAYLOAD_LEN, control};

    fn control_row(id: u16, divertable: bool, raw_xy: bool) -> Control {
        let mut info = [0u8; PAYLOAD_LEN];
        info[..2].copy_from_slice(&id.to_be_bytes());
        info[4] = u8::from(divertable) << 5;
        info[8] = u8::from(raw_xy);
        Control::from_info(&info)
    }

    fn diversion(trigger: Trigger, raw_xy: bool) -> Diversion {
        Diversion { trigger, raw_xy }
    }

    fn wanted(diversions: &[Diversion]) -> Wanted {
        Wanted {
            diversions: diversions.to_vec(),
            ..Wanted::default()
        }
    }

    /// A report the device sends on its own for the feature `feature_id`.
    fn event(link: &mut Link<FakeMouse>, feature_id: u16, function: u8, bytes: &[u8]) -> Message {
        let mut payload = [0u8; PAYLOAD_LEN];
        payload[..bytes.len()].copy_from_slice(bytes);
        Message {
            device: DIRECT_DEVICE,
            feature_index: link.channel().index_of(feature_id).unwrap(),
            function,
            software_id: 0,
            payload,
        }
    }

    fn held(controls: &[u16]) -> Vec<u8> {
        controls.iter().flat_map(|id| id.to_be_bytes()).collect()
    }

    /// Feeds one report and returns the emitted input and the status flag.
    fn feed(
        link: &mut Link<FakeMouse>,
        wanted: &Wanted,
        message: &Message,
    ) -> (Vec<DeviceInput>, bool) {
        let mut inputs = Vec::new();
        let changed = link
            .on_message(wanted, message, &mut |input| inputs.push(input))
            .unwrap();
        (inputs, changed)
    }

    fn apply(link: &mut Link<FakeMouse>, wanted: &Wanted) -> Vec<DeviceInput> {
        let mut inputs = Vec::new();
        link.apply(wanted, Vec::new(), &mut |input| inputs.push(input))
            .unwrap();
        inputs
    }

    #[test]
    fn only_bound_controls_the_device_can_divert_are_targets() {
        let controls = [
            control_row(control::GESTURE_BUTTON, true, true),
            control_row(control::MODE_SHIFT, true, false),
            control_row(control::HAPTIC_PANEL, false, true),
        ];
        let wanted = [
            diversion(Trigger::GestureButton, true),
            diversion(Trigger::ModeShift, true),
            diversion(Trigger::HapticPanel, false),
        ];
        assert_eq!(
            targets(&wanted, &controls, false),
            BTreeMap::from([
                (control::GESTURE_BUTTON, true),
                // The binding wants raw XY but this control cannot report it.
                (control::MODE_SHIFT, false),
            ])
        );
        assert!(targets(&wanted, &controls, true).is_empty());
        assert!(targets(&[], &controls, false).is_empty());
        // A device without the control: nothing to divert.
        assert!(targets(&wanted[..1], &controls[1..], false).is_empty());
    }

    #[test]
    fn held_reports_become_press_and_release_edges() {
        let none = BTreeSet::new();
        let (released, newly, held) = press_changes(&none, [control::GESTURE_BUTTON, 0x0052, 0, 0]);
        assert!(released.is_empty());
        assert_eq!(newly, vec![Trigger::GestureButton]);
        // A second control joins while the first stays held.
        let (released, newly, held) = press_changes(
            &held,
            [control::GESTURE_BUTTON, control::HAPTIC_PANEL, 0, 0],
        );
        assert!(released.is_empty());
        assert_eq!(newly, vec![Trigger::HapticPanel]);
        // The first is released; then everything.
        let (released, newly, held) = press_changes(&held, [control::HAPTIC_PANEL, 0, 0, 0]);
        assert_eq!(released, vec![Trigger::GestureButton]);
        assert!(newly.is_empty());
        let (released, newly, held) = press_changes(&held, [0; 4]);
        assert_eq!(released, vec![Trigger::HapticPanel]);
        assert!(newly.is_empty() && held.is_empty());
    }

    #[test]
    fn connecting_only_reads_and_refuses_what_is_not_a_usable_mouse() {
        let mut link = Link::connect(FakeMouse::default()).unwrap();
        assert!(link.channel().diverted.is_empty());
        assert!(!link.channel().thumb_diverted);
        let status = link.status();
        assert_eq!(status.link, AdapterLink::Active);
        assert_eq!(status.name, "MX Master 4");
        assert_eq!(status.dpi, Some(1000));
        assert_eq!(
            status.controls,
            [
                Trigger::GestureButton,
                Trigger::ModeShift,
                Trigger::HapticPanel
            ]
        );

        let keyboard = FakeMouse {
            is_mouse: false,
            ..FakeMouse::default()
        };
        assert!(Link::connect(keyboard).is_none());
        assert!(Link::connect(FakeMouse::default().without(feature::REPROG_CONTROLS)).is_none());
        let asleep = FakeMouse {
            silent: true,
            ..FakeMouse::default()
        };
        assert!(Link::connect(asleep).is_none());
    }

    #[test]
    fn diversions_follow_the_bindings_and_are_written_once() {
        let mut link = Link::connect(FakeMouse::default()).unwrap();
        let both = wanted(&[
            diversion(Trigger::GestureButton, true),
            diversion(Trigger::HapticPanel, false),
        ]);
        apply(&mut link, &both);
        assert_eq!(
            link.channel().diverted,
            BTreeMap::from([
                (control::GESTURE_BUTTON, (true, true)),
                (control::HAPTIC_PANEL, (true, false)),
            ])
        );
        let writes = link.channel().calls(feature::REPROG_CONTROLS, 3);
        apply(&mut link, &both);
        assert_eq!(link.channel().calls(feature::REPROG_CONTROLS, 3), writes);

        // The gesture binding no longer needs movement: the control is given
        // back and diverted again without raw XY.
        let plain = wanted(&[diversion(Trigger::GestureButton, false)]);
        apply(&mut link, &plain);
        assert_eq!(
            link.channel().diverted,
            BTreeMap::from([
                (control::GESTURE_BUTTON, (true, false)),
                (control::HAPTIC_PANEL, (false, false)),
            ])
        );
    }

    #[test]
    fn a_control_the_device_refuses_is_tried_again_later() {
        let mut mouse = FakeMouse::default();
        mouse.refuse.push((feature::REPROG_CONTROLS, 3));
        let mut link = Link::connect(mouse).unwrap();
        let gesture = wanted(&[diversion(Trigger::GestureButton, true)]);
        apply(&mut link, &gesture);
        assert!(link.channel().diverted.is_empty());
        link.channel().refuse.clear();
        apply(&mut link, &gesture);
        assert_eq!(
            link.channel().diverted.get(&control::GESTURE_BUTTON),
            Some(&(true, true))
        );
    }

    #[test]
    fn an_elevated_window_gets_the_controls_back_and_releases_what_was_held() {
        let mut link = Link::connect(FakeMouse::default()).unwrap();
        let mut gesture = wanted(&[diversion(Trigger::GestureButton, true)]);
        apply(&mut link, &gesture);
        let press = event(
            &mut link,
            feature::REPROG_CONTROLS,
            0,
            &held(&[control::GESTURE_BUTTON]),
        );
        assert_eq!(
            feed(&mut link, &gesture, &press).0,
            [DeviceInput::Press(Trigger::GestureButton)]
        );

        gesture.elevated = true;
        assert_eq!(
            apply(&mut link, &gesture),
            [DeviceInput::Release(Trigger::GestureButton)]
        );
        assert_eq!(
            link.channel().diverted.get(&control::GESTURE_BUTTON),
            Some(&(false, false))
        );
        // Nothing is held any more, so nothing is released twice.
        let mut inputs = Vec::new();
        link.release_held(&mut |input| inputs.push(input));
        assert!(inputs.is_empty());

        gesture.elevated = false;
        apply(&mut link, &gesture);
        assert_eq!(
            link.channel().diverted.get(&control::GESTURE_BUTTON),
            Some(&(true, true))
        );
    }

    #[test]
    fn control_events_become_trigger_input() {
        let mut link = Link::connect(FakeMouse::default()).unwrap();
        let both = wanted(&[
            diversion(Trigger::GestureButton, true),
            diversion(Trigger::HapticPanel, false),
        ]);
        apply(&mut link, &both);
        let press = event(
            &mut link,
            feature::REPROG_CONTROLS,
            0,
            &held(&[control::GESTURE_BUTTON]),
        );
        assert_eq!(
            feed(&mut link, &both, &press),
            (vec![DeviceInput::Press(Trigger::GestureButton)], false)
        );
        let moved = event(
            &mut link,
            feature::REPROG_CONTROLS,
            1,
            &[0xFF, 0xFD, 0x00, 0x07],
        );
        assert_eq!(
            feed(&mut link, &both, &moved).0,
            [DeviceInput::Move { dx: -3, dy: 7 }]
        );
        let swap = event(
            &mut link,
            feature::REPROG_CONTROLS,
            0,
            &held(&[control::HAPTIC_PANEL]),
        );
        assert_eq!(
            feed(&mut link, &both, &swap).0,
            [
                DeviceInput::Release(Trigger::GestureButton),
                DeviceInput::Press(Trigger::HapticPanel)
            ]
        );
        // A reply to some other client's request is not an event.
        let mut reply = press;
        reply.software_id = 3;
        assert_eq!(feed(&mut link, &both, &reply), (Vec::new(), false));

        let mut inputs = Vec::new();
        link.release_held(&mut |input| inputs.push(input));
        assert_eq!(inputs, [DeviceInput::Release(Trigger::HapticPanel)]);
    }

    #[test]
    fn a_battery_event_changes_the_status() {
        let mut link = Link::connect(FakeMouse::default()).unwrap();
        let nothing = Wanted::default();
        let low = event(&mut link, feature::UNIFIED_BATTERY, 0, &[15, 0, 0]);
        assert_eq!(feed(&mut link, &nothing, &low), (Vec::new(), true));
        assert_eq!(
            link.status().battery,
            Some(Battery {
                percent: 15,
                charging: false
            })
        );
        // Asking again: the device still says 80, which is a change.
        assert_eq!(link.refresh_battery(), Ok(true));
        assert_eq!(link.refresh_battery(), Ok(false));
        // An asleep device keeps the last reading; a departed one is gone.
        link.channel().silent = true;
        assert_eq!(link.refresh_battery(), Ok(false));
        link.channel().gone = true;
        assert_eq!(link.refresh_battery(), Err(Gone));
    }

    #[test]
    fn a_device_that_comes_back_gets_its_diversions_and_settings_again() {
        let mut mouse = FakeMouse::default();
        mouse.features.push(feature::WIRELESS_STATUS);
        let mut link = Link::connect(mouse).unwrap();
        let mut gesture = wanted(&[diversion(Trigger::GestureButton, true)]);
        gesture.settings.dpi = Some(1600);
        apply(&mut link, &gesture);
        let press = event(
            &mut link,
            feature::REPROG_CONTROLS,
            0,
            &held(&[control::GESTURE_BUTTON]),
        );
        feed(&mut link, &gesture, &press);

        // The power cycle: the device forgot everything.
        link.channel().diverted.clear();
        link.channel().dpi = 1000;
        let back = event(&mut link, feature::WIRELESS_STATUS, 0, &[1, 1, 1]);
        let (inputs, _) = feed(&mut link, &gesture, &back);
        assert_eq!(inputs, [DeviceInput::Release(Trigger::GestureButton)]);
        assert_eq!(
            link.channel().diverted.get(&control::GESTURE_BUTTON),
            Some(&(true, true))
        );
        assert_eq!(link.channel().dpi, 1600);
    }

    #[test]
    fn giving_back_undiverts_everything_and_keeps_the_users_settings() {
        let mut link = Link::connect(FakeMouse::default()).unwrap();
        let mut both = wanted(&[
            diversion(Trigger::GestureButton, true),
            diversion(Trigger::HapticPanel, false),
        ]);
        both.settings.dpi = Some(1600);
        apply(&mut link, &both);
        link.give_back();
        assert!(
            link.channel()
                .diverted
                .values()
                .all(|(diverted, _)| !diverted)
        );
        assert!(!link.channel().thumb_diverted);
        assert_eq!(link.channel().dpi, 1600);

        // A device that left is not asked control by control.
        apply(&mut link, &both);
        link.channel().refuse.push((feature::REPROG_CONTROLS, 3));
        let before = link.channel().log.len();
        link.give_back();
        assert_eq!(link.channel().log.len() - before, 1);
    }

    #[test]
    fn a_lost_channel_ends_the_link() {
        let mut link = Link::connect(FakeMouse::default()).unwrap();
        link.channel().gone = true;
        let gesture = wanted(&[diversion(Trigger::GestureButton, true)]);
        assert_eq!(link.apply(&gesture, Vec::new(), &mut |_| {}), Err(Gone));
    }
}
