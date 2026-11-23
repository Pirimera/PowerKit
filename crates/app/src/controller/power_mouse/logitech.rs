//! Logitech HID++ 2.0 queries over a [`HidppChannel`]: what the device is,
//! its battery and its controls. Everything here only reads from the device.
use super::{HidppChannel, RequestError};
use powerkit_core::power_mouse::hidpp::{
    self, Battery, Control, DEVICE_TYPE_MOUSE, PAYLOAD_LEN, Request, feature,
};

/// Where the features PowerMouse uses live in this device's feature table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    pub name: Option<u8>,
    pub battery: Option<u8>,
    pub controls: Option<u8>,
}

/// What a device said about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub is_mouse: bool,
    pub features: Features,
    pub battery: Option<Battery>,
    pub controls: Vec<Control>,
}

/// Asks the device at `device` what it is. The first request is a ping, so a
/// device that is asleep, gone or not HID++ 2.0 fails fast with one timeout.
pub fn identify(channel: &mut impl HidppChannel, device: u8) -> Result<Identity, RequestError> {
    channel.request(&hidpp::ping(device, 0x5A))?;
    let features = Features {
        name: lookup(channel, device, feature::DEVICE_NAME)?,
        battery: lookup(channel, device, feature::UNIFIED_BATTERY)?,
        controls: lookup(channel, device, feature::REPROG_CONTROLS)?,
    };
    let (name, is_mouse) = match features.name {
        Some(index) => (
            name(channel, device, index)?,
            call(channel, device, index, 2, &[])?[0] == DEVICE_TYPE_MOUSE,
        ),
        None => (String::new(), false),
    };
    let battery = match features.battery {
        Some(index) => battery(channel, device, index)?,
        None => None,
    };
    let controls = match features.controls {
        Some(index) => controls(channel, device, index)?,
        None => Vec::new(),
    };
    Ok(Identity {
        name,
        is_mouse,
        features,
        battery,
        controls,
    })
}

/// One call of a feature's function, by the feature's index on this device.
pub fn call(
    channel: &mut impl HidppChannel,
    device: u8,
    feature_index: u8,
    function: u8,
    params: &[u8],
) -> Result<[u8; PAYLOAD_LEN], RequestError> {
    channel.request(&Request::new(device, feature_index, function, params))
}

/// Where feature `id` lives in the device's table; `None` when it has none.
pub fn lookup(
    channel: &mut impl HidppChannel,
    device: u8,
    id: u16,
) -> Result<Option<u8>, RequestError> {
    let reply = channel.request(&hidpp::feature_lookup(device, id))?;
    Ok(hidpp::feature_index(&reply))
}

/// The marketing name: function 0 gives its length, function 1 a chunk of it
/// starting at an offset.
fn name(channel: &mut impl HidppChannel, device: u8, index: u8) -> Result<String, RequestError> {
    let len = usize::from(call(channel, device, index, 0, &[])?[0]);
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        let offset = u8::try_from(bytes.len()).unwrap_or(u8::MAX);
        let chunk = call(channel, device, index, 1, &[offset])?;
        bytes.extend_from_slice(&chunk[..(len - bytes.len()).min(PAYLOAD_LEN)]);
    }
    Ok(String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string())
}

/// Unified battery, function 1: the current status.
pub fn battery(
    channel: &mut impl HidppChannel,
    device: u8,
    index: u8,
) -> Result<Option<Battery>, RequestError> {
    Ok(Battery::from_status(&call(channel, device, index, 1, &[])?))
}

/// Reprogrammable controls: function 0 counts the table, function 1 reads
/// one row.
fn controls(
    channel: &mut impl HidppChannel,
    device: u8,
    index: u8,
) -> Result<Vec<Control>, RequestError> {
    let count = call(channel, device, index, 0, &[])?[0];
    (0..count)
        .map(|row| {
            Ok(Control::from_info(&call(
                channel,
                device,
                index,
                1,
                &[row],
            )?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeMouse;
    use powerkit_core::power_mouse::hidpp::{DIRECT_DEVICE, control};

    #[test]
    fn identify_reads_name_type_battery_and_the_control_table() {
        let mut mouse = FakeMouse::default();
        let identity = identify(&mut mouse, DIRECT_DEVICE).unwrap();
        assert_eq!(identity.name, "MX Master 4");
        assert!(identity.is_mouse);
        assert_eq!(
            identity.battery,
            Some(Battery {
                percent: 80,
                charging: false
            })
        );
        assert_eq!(identity.features.name, mouse.index_of(feature::DEVICE_NAME));
        assert_eq!(
            identity.features.controls,
            mouse.index_of(feature::REPROG_CONTROLS)
        );
        assert_eq!(identity.controls.len(), 4);
        let gesture = identity
            .controls
            .iter()
            .find(|c| c.id == control::GESTURE_BUTTON)
            .unwrap();
        assert!(gesture.divertable() && gesture.raw_xy());
        let panel = identity
            .controls
            .iter()
            .find(|c| c.id == control::HAPTIC_PANEL)
            .unwrap();
        assert!(panel.divertable() && !panel.raw_xy());
        assert!(
            !identity
                .controls
                .iter()
                .any(|c| c.id == 0x0050 && c.divertable())
        );
        // Identifying only reads: nothing was diverted or set.
        assert!(mouse.diverted.is_empty());
        assert_eq!(mouse.dpi, 1000);
    }

    #[test]
    fn a_name_longer_than_one_reply_is_read_in_chunks() {
        let mut mouse = FakeMouse {
            name: "A Logitech Mouse With A Long Name".into(),
            ..FakeMouse::default()
        };
        let identity = identify(&mut mouse, DIRECT_DEVICE).unwrap();
        assert_eq!(identity.name, "A Logitech Mouse With A Long Name");
        assert_eq!(mouse.calls(feature::DEVICE_NAME, 1), 3);
    }

    #[test]
    fn a_device_without_a_feature_simply_lacks_what_it_provides() {
        let mut mouse = FakeMouse::default()
            .without(feature::DEVICE_NAME)
            .without(feature::UNIFIED_BATTERY)
            .without(feature::REPROG_CONTROLS);
        let identity = identify(&mut mouse, DIRECT_DEVICE).unwrap();
        assert_eq!(identity.name, "");
        assert!(!identity.is_mouse);
        assert_eq!(identity.battery, None);
        assert!(identity.controls.is_empty());
        assert_eq!(identity.features, Features::default());
    }

    #[test]
    fn charging_and_faulty_batteries_are_told_apart() {
        let mut charging = FakeMouse {
            battery: Some((35, 1)),
            ..FakeMouse::default()
        };
        let index = charging.index_of(feature::UNIFIED_BATTERY).unwrap();
        assert_eq!(
            battery(&mut charging, DIRECT_DEVICE, index).unwrap(),
            Some(Battery {
                percent: 35,
                charging: true
            })
        );
        let mut faulty = FakeMouse {
            battery: None,
            ..FakeMouse::default()
        };
        assert_eq!(battery(&mut faulty, DIRECT_DEVICE, index).unwrap(), None);
    }

    #[test]
    fn a_sleeping_device_fails_on_the_first_request_and_a_departed_one_reports_io() {
        let mut asleep = FakeMouse {
            silent: true,
            ..FakeMouse::default()
        };
        assert!(matches!(
            identify(&mut asleep, DIRECT_DEVICE),
            Err(RequestError::Timeout)
        ));
        // The ping failed, so nothing else was asked.
        assert!(asleep.log.is_empty());
        let mut gone = FakeMouse {
            gone: true,
            ..FakeMouse::default()
        };
        assert!(matches!(
            identify(&mut gone, DIRECT_DEVICE),
            Err(RequestError::Io(_))
        ));
    }
}
