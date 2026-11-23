//! Logitech device settings and device actions over a [`HidppChannel`]: DPI,
//! wheel mode, wheel resolution, thumb-wheel reporting, Easy-Switch and the
//! haptic tap. Used only on the adapter thread that owns the channel.
//!
//! A setting is written only when it differs from what this adapter last
//! set, and a write the device refuses or does not answer is tried again on
//! the next call. DPI and wheel settings are the user's choice and stay on
//! the device; only the thumb-wheel diversion is given back.
use super::{Gone, HidppChannel, RequestError, settled};
use powerkit_core::power_mouse::device::{DeviceAction, DeviceSettings};
use powerkit_core::power_mouse::hidpp::{
    self, DIRECT_DEVICE, DpiSupport, Request, WheelMode, feature,
};
use powerkit_core::power_mouse::{DPI_PRESETS, Trigger, next_dpi};

/// Where the setting features live on this device, and what this adapter
/// last set, so a setting is written only when it has to change.
#[derive(Debug, Default)]
pub struct Settings {
    dpi_index: Option<u8>,
    dpi_support: DpiSupport,
    dpi: Option<u16>,
    wheel_index: Option<u8>,
    /// The device's own default speed at which the ratchet releases.
    auto_disengage: u8,
    wheel_mode: Option<WheelMode>,
    hires_index: Option<u8>,
    hires: Option<bool>,
    thumb_index: Option<u8>,
    thumb_diverted: bool,
    /// The device counts rotation to the right as positive.
    thumb_positive_right: bool,
    host_index: Option<u8>,
    haptic_index: Option<u8>,
}

fn lookup(channel: &mut impl HidppChannel, id: u16) -> Result<Option<u8>, RequestError> {
    let reply = channel.request(&hidpp::feature_lookup(DIRECT_DEVICE, id))?;
    Ok(hidpp::feature_index(&reply))
}

fn request(index: u8, function: u8, params: &[u8]) -> Request {
    Request::new(DIRECT_DEVICE, index, function, params)
}

impl Settings {
    /// Finds the setting features and reads what is needed to use them. A
    /// device simply lacks the features it does not report.
    pub fn discover(channel: &mut impl HidppChannel) -> Result<Self, RequestError> {
        let mut settings = Self {
            dpi_index: lookup(channel, feature::ADJUSTABLE_DPI)?,
            wheel_index: lookup(channel, feature::SMARTSHIFT_ENHANCED)?,
            hires_index: lookup(channel, feature::HIRES_WHEEL)?,
            thumb_index: lookup(channel, feature::THUMB_WHEEL)?,
            host_index: lookup(channel, feature::CHANGE_HOST)?,
            haptic_index: lookup(channel, feature::HAPTIC_FEEDBACK)?,
            ..Self::default()
        };
        if let Some(index) = settings.dpi_index {
            // Function 1 lists the values of sensor 0, function 2 reads it.
            settings.dpi_support =
                DpiSupport::from_list(&channel.request(&request(index, 1, &[0]))?);
            settings.dpi = Some(hidpp::dpi_from_reply(&channel.request(&request(
                index,
                2,
                &[0],
            ))?));
        }
        if let Some(index) = settings.wheel_index {
            // Capabilities: flags, then the default release speed.
            settings.auto_disengage = channel.request(&request(index, 0, &[]))?[1];
        }
        if let Some(index) = settings.thumb_index {
            // Info: byte 4 bit 0 is the default direction.
            settings.thumb_positive_right = channel.request(&request(index, 0, &[]))?[4] & 1 != 0;
        }
        Ok(settings)
    }

    /// The device reconnected and forgot its volatile settings: everything
    /// this adapter set must be written again.
    pub fn forget(&mut self) {
        self.dpi = None;
        self.wheel_mode = None;
        self.hires = None;
        self.thumb_diverted = false;
    }

    /// Writes the settings that differ from what the device has. `Ok(true)`
    /// means something the page shows has changed. A setting the device
    /// refuses or does not answer is left for the next call.
    pub fn apply(
        &mut self,
        channel: &mut impl HidppChannel,
        wanted: &DeviceSettings,
        elevated: bool,
    ) -> Result<bool, Gone> {
        let mut changed = false;
        if let (Some(index), Some(dpi)) = (self.dpi_index, wanted.dpi)
            && self.dpi != Some(dpi)
            && self.dpi_support.supports(dpi)
            && settled(channel.request(&hidpp::set_dpi(DIRECT_DEVICE, index, dpi)))?.is_some()
        {
            self.dpi = Some(dpi);
            changed = true;
        }
        if let (Some(index), Some(mode)) = (self.wheel_index, wanted.wheel_mode)
            && self.wheel_mode != Some(mode)
            && settled(channel.request(&hidpp::set_wheel_mode(
                DIRECT_DEVICE,
                index,
                mode,
                self.auto_disengage,
            )))?
            .is_some()
        {
            self.wheel_mode = Some(mode);
        }
        if let (Some(index), Some(high)) = (self.hires_index, wanted.hires_wheel)
            && self.hires != Some(high)
            // Function 1 reads the mode whose other bits must be kept.
            && let Some(mode) = settled(channel.request(&request(index, 1, &[])))?
            && settled(channel.request(&hidpp::set_wheel_resolution(
                DIRECT_DEVICE,
                index,
                &mode,
                high,
            )))?
            .is_some()
        {
            self.hires = Some(high);
        }
        // Like diverted buttons, the thumb wheel goes back to the device
        // while an elevated window is in front.
        self.divert_thumb_wheel(channel, wanted.thumb_wheel && !elevated)?;
        Ok(changed)
    }

    fn divert_thumb_wheel(
        &mut self,
        channel: &mut impl HidppChannel,
        diverted: bool,
    ) -> Result<(), Gone> {
        if let Some(index) = self.thumb_index
            && self.thumb_diverted != diverted
            && settled(channel.request(&hidpp::set_thumb_wheel_diverted(
                DIRECT_DEVICE,
                index,
                diverted,
            )))?
            .is_some()
        {
            self.thumb_diverted = diverted;
        }
        Ok(())
    }

    /// Gives the thumb wheel back to the device. DPI and wheel settings are
    /// the user's choice and stay.
    pub fn restore(&mut self, channel: &mut impl HidppChannel) {
        let _ = self.divert_thumb_wheel(channel, false);
    }

    /// Carries out a device action. `Ok(true)` means the DPI changed.
    pub fn act(
        &mut self,
        channel: &mut impl HidppChannel,
        action: DeviceAction,
    ) -> Result<bool, Gone> {
        match action {
            DeviceAction::SwitchHost(host) => {
                if let Some(index) = self.host_index {
                    // The device leaves for the other computer without replying.
                    channel
                        .notify(&hidpp::change_host(DIRECT_DEVICE, index, host))
                        .map_err(|_| Gone)?;
                }
                Ok(false)
            }
            DeviceAction::HapticPulse => {
                if let Some(index) = self.haptic_index {
                    settled(channel.request(&hidpp::play_haptic(DIRECT_DEVICE, index)))?;
                }
                Ok(false)
            }
            DeviceAction::DpiCycle => {
                let (Some(index), Some(current)) = (self.dpi_index, self.dpi) else {
                    return Ok(false);
                };
                let Some(next) = next_dpi(current, &self.dpi_support) else {
                    return Ok(false);
                };
                if settled(channel.request(&hidpp::set_dpi(DIRECT_DEVICE, index, next)))?.is_none()
                {
                    return Ok(false);
                }
                self.dpi = Some(next);
                Ok(true)
            }
        }
    }

    /// The thumb-wheel direction a report stands for, if `message` is one.
    pub fn thumb_wheel_turn(&self, message: &hidpp::Message) -> Option<Trigger> {
        let rotation = hidpp::thumb_wheel_rotation(message, self.thumb_index?)?;
        if rotation == 0 {
            return None;
        }
        Some(if (rotation > 0) == self.thumb_positive_right {
            Trigger::ThumbRight
        } else {
            Trigger::ThumbLeft
        })
    }

    pub fn dpi(&self) -> Option<u16> {
        self.dpi
    }

    /// The page's DPI choices: the presets this device accepts.
    pub fn dpi_presets(&self) -> Vec<u16> {
        DPI_PRESETS
            .into_iter()
            .filter(|dpi| self.dpi_support.supports(*dpi))
            .collect()
    }

    pub fn has_wheel_mode(&self) -> bool {
        self.wheel_index.is_some()
    }

    pub fn has_hires_wheel(&self) -> bool {
        self.hires_index.is_some()
    }

    pub fn has_thumb_wheel(&self) -> bool {
        self.thumb_index.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeMouse;
    use powerkit_core::power_mouse::hidpp::Message;

    fn wanted(dpi: Option<u16>) -> DeviceSettings {
        DeviceSettings {
            dpi,
            ..DeviceSettings::default()
        }
    }
    fn discovered(mouse: &mut FakeMouse) -> Settings {
        let settings = Settings::discover(mouse).unwrap();
        mouse.log.clear();
        settings
    }

    #[test]
    fn discovery_reads_what_the_device_has_and_writes_nothing() {
        let mut mouse = FakeMouse::default();
        let settings = Settings::discover(&mut mouse).unwrap();
        assert_eq!(settings.dpi(), Some(1000));
        assert_eq!(settings.dpi_presets(), DPI_PRESETS);
        assert!(settings.has_wheel_mode() && settings.has_hires_wheel());
        assert!(settings.has_thumb_wheel());
        // Only lookups and reads: functions 0 to 2, never a write.
        assert_eq!(mouse.calls(feature::ADJUSTABLE_DPI, 3), 0);
        assert_eq!(mouse.calls(feature::SMARTSHIFT_ENHANCED, 2), 0);
        assert_eq!(mouse.calls(feature::HIRES_WHEEL, 2), 0);
        assert_eq!(mouse.calls(feature::THUMB_WHEEL, 2), 0);
        assert_eq!(
            (mouse.dpi, mouse.wheel, mouse.hires_mode),
            (1000, (2, 10), 0b001)
        );

        let mut plain = FakeMouse::default()
            .without(feature::ADJUSTABLE_DPI)
            .without(feature::SMARTSHIFT_ENHANCED)
            .without(feature::HIRES_WHEEL)
            .without(feature::THUMB_WHEEL);
        let settings = Settings::discover(&mut plain).unwrap();
        assert_eq!(settings.dpi(), None);
        assert!(settings.dpi_presets().is_empty());
        assert!(!settings.has_wheel_mode() && !settings.has_hires_wheel());
        assert!(!settings.has_thumb_wheel());
    }

    #[test]
    fn only_presets_the_sensor_accepts_are_offered() {
        let mut mouse = FakeMouse {
            dpi_list: vec![800, 1600, 3200],
            dpi: 800,
            ..FakeMouse::default()
        };
        let settings = discovered(&mut mouse);
        assert_eq!(settings.dpi_presets(), [800, 1600, 3200]);
    }

    #[test]
    fn a_setting_is_written_once_and_left_alone_while_it_holds() {
        let mut mouse = FakeMouse::default();
        let mut settings = discovered(&mut mouse);
        // Nothing wanted: the device keeps its own values.
        assert_eq!(settings.apply(&mut mouse, &wanted(None), false), Ok(false));
        assert!(mouse.log.is_empty());
        assert_eq!(
            settings.apply(&mut mouse, &wanted(Some(1600)), false),
            Ok(true)
        );
        assert_eq!(mouse.dpi, 1600);
        assert_eq!(settings.dpi(), Some(1600));
        assert_eq!(
            settings.apply(&mut mouse, &wanted(Some(1600)), false),
            Ok(false)
        );
        assert_eq!(mouse.calls(feature::ADJUSTABLE_DPI, 3), 1);
        // A value the sensor does not accept is never sent.
        assert_eq!(
            settings.apply(&mut mouse, &wanted(Some(425)), false),
            Ok(false)
        );
        assert_eq!(mouse.dpi, 1600);
        assert_eq!(mouse.calls(feature::ADJUSTABLE_DPI, 3), 1);
    }

    #[test]
    fn wheel_modes_use_the_devices_own_release_speed_for_smartshift() {
        let mut mouse = FakeMouse {
            auto_disengage: 23,
            ..FakeMouse::default()
        };
        let mut settings = discovered(&mut mouse);
        let mode = |mode| DeviceSettings {
            wheel_mode: Some(mode),
            ..DeviceSettings::default()
        };
        settings
            .apply(&mut mouse, &mode(WheelMode::FreeSpin), false)
            .unwrap();
        assert_eq!(mouse.wheel.0, 1);
        settings
            .apply(&mut mouse, &mode(WheelMode::Ratchet), false)
            .unwrap();
        assert_eq!(mouse.wheel, (2, 0xFF), "a ratchet that never releases");
        settings
            .apply(&mut mouse, &mode(WheelMode::SmartShift), false)
            .unwrap();
        assert_eq!(mouse.wheel, (2, 23));
        assert_eq!(mouse.calls(feature::SMARTSHIFT_ENHANCED, 2), 3);
        settings
            .apply(&mut mouse, &mode(WheelMode::SmartShift), false)
            .unwrap();
        assert_eq!(mouse.calls(feature::SMARTSHIFT_ENHANCED, 2), 3);
    }

    #[test]
    fn the_wheel_resolution_bit_changes_and_the_other_mode_bits_stay() {
        let mut mouse = FakeMouse {
            hires_mode: 0b101,
            ..FakeMouse::default()
        };
        let mut settings = discovered(&mut mouse);
        let hires = |on| DeviceSettings {
            hires_wheel: Some(on),
            ..DeviceSettings::default()
        };
        settings.apply(&mut mouse, &hires(true), false).unwrap();
        assert_eq!(mouse.hires_mode, 0b111);
        settings.apply(&mut mouse, &hires(false), false).unwrap();
        assert_eq!(mouse.hires_mode, 0b101);
        // The mode is read before each write, so unknown bits survive.
        assert_eq!(mouse.calls(feature::HIRES_WHEEL, 1), 2);
        assert_eq!(mouse.calls(feature::HIRES_WHEEL, 2), 2);
    }

    #[test]
    fn the_thumb_wheel_is_diverted_only_while_wanted_and_not_elevated() {
        let mut mouse = FakeMouse::default();
        let mut settings = discovered(&mut mouse);
        let thumb = DeviceSettings {
            thumb_wheel: true,
            ..DeviceSettings::default()
        };
        settings.apply(&mut mouse, &thumb, false).unwrap();
        assert!(mouse.thumb_diverted);
        // An elevated window in front gives the wheel back to the device.
        settings.apply(&mut mouse, &thumb, true).unwrap();
        assert!(!mouse.thumb_diverted);
        settings.apply(&mut mouse, &thumb, false).unwrap();
        assert!(mouse.thumb_diverted);
        settings.restore(&mut mouse);
        assert!(!mouse.thumb_diverted);
        // Restoring keeps the user's DPI.
        assert_eq!(mouse.dpi, 1000);
        assert_eq!(mouse.calls(feature::ADJUSTABLE_DPI, 3), 0);
    }

    #[test]
    fn a_reconnected_device_gets_every_setting_again() {
        let mut mouse = FakeMouse::default();
        let mut settings = discovered(&mut mouse);
        let all = DeviceSettings {
            dpi: Some(1600),
            wheel_mode: Some(WheelMode::FreeSpin),
            hires_wheel: Some(true),
            thumb_wheel: true,
        };
        settings.apply(&mut mouse, &all, false).unwrap();
        // The device reconnected and lost its volatile settings.
        mouse.dpi = 1000;
        mouse.wheel = (2, 10);
        mouse.hires_mode = 0;
        mouse.thumb_diverted = false;
        assert_eq!(settings.apply(&mut mouse, &all, false), Ok(false));
        assert_eq!(mouse.dpi, 1000, "without forget nothing is rewritten");
        settings.forget();
        assert_eq!(settings.apply(&mut mouse, &all, false), Ok(true));
        assert_eq!(mouse.dpi, 1600);
        assert_eq!(mouse.wheel.0, 1);
        assert_eq!(mouse.hires_mode & 0b10, 0b10);
        assert!(mouse.thumb_diverted);
    }

    #[test]
    fn a_refused_or_unanswered_write_is_tried_again_and_a_lost_device_is_gone() {
        let mut mouse = FakeMouse::default();
        let mut settings = discovered(&mut mouse);
        mouse.refuse.push((feature::ADJUSTABLE_DPI, 3));
        assert_eq!(
            settings.apply(&mut mouse, &wanted(Some(1600)), false),
            Ok(false)
        );
        assert_eq!(settings.dpi(), Some(1000));
        mouse.refuse.clear();
        mouse.silent = true;
        assert_eq!(
            settings.apply(&mut mouse, &wanted(Some(1600)), false),
            Ok(false)
        );
        mouse.silent = false;
        assert_eq!(
            settings.apply(&mut mouse, &wanted(Some(1600)), false),
            Ok(true)
        );
        assert_eq!(mouse.dpi, 1600);
        mouse.gone = true;
        assert_eq!(
            settings.apply(&mut mouse, &wanted(Some(800)), false),
            Err(Gone)
        );
        assert_eq!(
            settings.act(&mut mouse, DeviceAction::HapticPulse),
            Err(Gone)
        );
        assert_eq!(
            settings.act(&mut mouse, DeviceAction::SwitchHost(1)),
            Err(Gone)
        );
    }

    #[test]
    fn device_actions_cycle_dpi_switch_host_and_tap() {
        let mut mouse = FakeMouse {
            dpi: 3200,
            ..FakeMouse::default()
        };
        let mut settings = discovered(&mut mouse);
        assert_eq!(settings.act(&mut mouse, DeviceAction::DpiCycle), Ok(true));
        assert_eq!(mouse.dpi, 4000);
        // After the highest preset the cycle wraps to the lowest.
        assert_eq!(settings.act(&mut mouse, DeviceAction::DpiCycle), Ok(true));
        assert_eq!(mouse.dpi, 400);
        assert_eq!(
            settings.act(&mut mouse, DeviceAction::HapticPulse),
            Ok(false)
        );
        assert_eq!(mouse.haptic_taps, 1);
        assert_eq!(
            settings.act(&mut mouse, DeviceAction::SwitchHost(2)),
            Ok(false)
        );
        assert_eq!(mouse.host, Some(2));

        // A device without the features ignores the actions.
        let mut plain = FakeMouse::default()
            .without(feature::ADJUSTABLE_DPI)
            .without(feature::CHANGE_HOST)
            .without(feature::HAPTIC_FEEDBACK);
        let mut settings = discovered(&mut plain);
        for action in [
            DeviceAction::DpiCycle,
            DeviceAction::HapticPulse,
            DeviceAction::SwitchHost(0),
        ] {
            assert_eq!(settings.act(&mut plain, action), Ok(false));
        }
        assert!(plain.log.is_empty());
        assert_eq!((plain.host, plain.haptic_taps), (None, 0));
    }

    #[test]
    fn thumb_wheel_reports_become_left_and_right_by_the_devices_direction() {
        let turn = |settings: &Settings, index: u8, rotation: i16| {
            let mut payload = [0; 16];
            payload[..2].copy_from_slice(&rotation.to_be_bytes());
            settings.thumb_wheel_turn(&Message {
                device: DIRECT_DEVICE,
                feature_index: index,
                function: 0,
                software_id: 0,
                payload,
            })
        };
        let mut mouse = FakeMouse::default();
        let index = mouse.index_of(feature::THUMB_WHEEL).unwrap();
        let settings = discovered(&mut mouse);
        // Default direction: positive is left.
        assert_eq!(turn(&settings, index, 3), Some(Trigger::ThumbLeft));
        assert_eq!(turn(&settings, index, -3), Some(Trigger::ThumbRight));
        assert_eq!(turn(&settings, index, 0), None);
        assert_eq!(
            turn(&settings, index + 1, 3),
            None,
            "another feature's event"
        );

        let mut flipped = FakeMouse {
            thumb_positive_right: true,
            ..FakeMouse::default()
        };
        let settings = discovered(&mut flipped);
        assert_eq!(turn(&settings, index, 3), Some(Trigger::ThumbRight));
        assert_eq!(turn(&settings, index, -3), Some(Trigger::ThumbLeft));
    }
}
