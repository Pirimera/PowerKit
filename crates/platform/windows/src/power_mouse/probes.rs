//! Hardware probes: tests that talk to the Logitech mouse connected to this
//! machine. They are ignored by default and run by hand, one at a time, to
//! check the HID++ layer against a real device. The read-only ones are safe
//! while PowerKit runs; the two that write say so in their `ignore` reason.
use super::discovery;
use super::hidpp_channel::Channel;
use powerkit_app::controller::power_mouse::device_settings::Settings;
use powerkit_app::controller::power_mouse::logitech::{call, identify, lookup};
use powerkit_core::power_mouse::device;
use powerkit_core::power_mouse::hidpp::{self, DIRECT_DEVICE, Request, feature};

/// Read-only: whether each device button is diverted right now, which
/// shows whether a running PowerKit has taken it over.
#[test]
#[ignore = "queries the Logitech devices connected to this machine"]
fn probe_prints_control_reporting() {
    use powerkit_core::power_mouse::Trigger;
    let found = device::devices(&discovery::enumerate());
    let Some(device) = found.into_iter().find(device::Device::adapter_ready) else {
        println!("no adapter-ready device connected");
        return;
    };
    let mut channel = Channel::open(&device.channel_path).expect("open");
    let identity = identify(&mut channel, DIRECT_DEVICE).expect("identify");
    let index = identity.features.controls.expect("controls feature");
    for trigger in Trigger::DEVICE {
        let control = trigger.control_id().expect("device trigger");
        let [high, low] = control.to_be_bytes();
        // Function 2 reads one control's reporting flags; bit 0 is
        // "diverted", bit 4 "raw XY".
        match call(&mut channel, DIRECT_DEVICE, index, 2, &[high, low]) {
            Ok(reply) => println!(
                "{trigger:?} ({control:04X}): diverted={} raw_xy={}",
                reply[2] & 1 != 0,
                reply[2] & 1 << 4 != 0
            ),
            Err(error) => println!("{trigger:?} ({control:04X}): {error:?}"),
        }
    }
}

/// Read-only: which device-setting features the mouse has and their
/// current values, as raw reply bytes.
#[test]
#[ignore = "queries the Logitech devices connected to this machine"]
fn probe_prints_setting_features() {
    let found = device::devices(&discovery::enumerate());
    let Some(device) = found.into_iter().find(device::Device::adapter_ready) else {
        println!("no adapter-ready device connected");
        return;
    };
    let mut channel = Channel::open(&device.channel_path).expect("open");
    channel
        .request(&hidpp::ping(DIRECT_DEVICE, 1))
        .expect("ping");
    /// Feature id, label, then the read-only functions to call with
    /// their params.
    type Query = (u16, &'static str, &'static [(u8, &'static [u8])]);
    let features: [Query; 9] = [
        (0x2201, "adjustable dpi", &[(0, &[]), (1, &[0]), (2, &[0])]),
        (0x2202, "extended dpi", &[(0, &[]), (1, &[0]), (5, &[0])]),
        (0x2110, "smartshift", &[(0, &[])]),
        (0x2111, "smartshift enhanced", &[(0, &[]), (1, &[])]),
        (0x2121, "hires wheel", &[(0, &[]), (1, &[]), (3, &[])]),
        (0x2150, "thumbwheel", &[(0, &[]), (1, &[])]),
        (0x1814, "change host", &[(0, &[])]),
        (0x1815, "hosts info", &[(0, &[])]),
        (0x19B0, "haptic feedback", &[(0, &[]), (1, &[]), (3, &[])]),
    ];
    for (id, label, calls) in features {
        let Ok(Some(index)) = lookup(&mut channel, DIRECT_DEVICE, id) else {
            println!("{id:04X} {label}: absent");
            continue;
        };
        println!("{id:04X} {label}: index {index}");
        for (function, params) in calls {
            match call(&mut channel, DIRECT_DEVICE, index, *function, params) {
                Ok(reply) => println!("  fn{function}: {reply:02X?}"),
                Err(error) => println!("  fn{function}: {error:?}"),
            }
        }
    }
}

/// Diverts the gesture button with raw XY, reads the device's reporting
/// state back, then undiverts it and reads again. Diversion is volatile,
/// and the last step restores it whatever the earlier steps returned.
#[test]
#[ignore = "changes and restores a volatile setting on the connected Logitech mouse"]
fn probe_diverts_and_restores_the_gesture_button() {
    use powerkit_core::power_mouse::hidpp::control::GESTURE_BUTTON;
    let found = device::devices(&discovery::enumerate());
    let Some(device) = found.into_iter().find(device::Device::adapter_ready) else {
        println!("no adapter-ready device connected");
        return;
    };
    let mut channel = Channel::open(&device.channel_path).expect("open");
    let identity = identify(&mut channel, DIRECT_DEVICE).expect("identify");
    let index = identity.features.controls.expect("controls feature");
    let [high, low] = GESTURE_BUTTON.to_be_bytes();
    let reporting = |channel: &mut Channel| {
        call(channel, DIRECT_DEVICE, index, 2, &[high, low]).map(|reply| reply[2])
    };
    println!("before: flags={:?}", reporting(&mut channel));
    let diverted = channel.request(&hidpp::set_diversion(
        DIRECT_DEVICE,
        index,
        GESTURE_BUTTON,
        true,
        true,
    ));
    println!("divert: ok={}", diverted.is_ok());
    println!("diverted: flags={:?}", reporting(&mut channel));
    let restored = channel.request(&hidpp::set_diversion(
        DIRECT_DEVICE,
        index,
        GESTURE_BUTTON,
        false,
        true,
    ));
    println!("undivert: ok={}", restored.is_ok());
    println!("after: flags={:?}", reporting(&mut channel));
}

/// Read-only: asks each directly connected Logitech mouse what it is and
/// prints the answers, for checking the adapter against real hardware.
#[test]
#[ignore = "queries the Logitech devices connected to this machine"]
fn probe_prints_device_identity() {
    let found = device::devices(&discovery::enumerate());
    println!("{} adapter-capable devices", found.len());
    for device in found {
        if device.transport.is_receiver() || device.channel_path.is_empty() {
            println!("skipped {:?} ({:?})", device.name, device.transport);
            continue;
        }
        let mut channel = match Channel::open(&device.channel_path) {
            Ok(channel) => channel,
            Err(error) => {
                println!("open failed for {:?}: {error}", device.name);
                continue;
            }
        };
        match identify(&mut channel, DIRECT_DEVICE) {
            Ok(identity) => {
                println!(
                    "name={:?} mouse={} features={:?} battery={:?}",
                    identity.name, identity.is_mouse, identity.features, identity.battery
                );
                for control in &identity.controls {
                    println!(
                        "  control id={:04X} task={:04X} divertable={} raw_xy={}",
                        control.id,
                        control.task,
                        control.divertable(),
                        control.raw_xy()
                    );
                }
            }
            Err(error) => println!("identify failed for {:?}: {error:?}", device.name),
        }
    }
}

/// Writes each setting back with the value the device already has, so
/// nothing changes, and prints whether the device accepted the request.
/// This checks the write functions and layouts against real hardware.
#[test]
#[ignore = "writes the connected Logitech mouse's current settings back to it"]
fn probe_writes_current_settings_back() {
    let found = device::devices(&discovery::enumerate());
    let Some(device) = found.into_iter().find(device::Device::adapter_ready) else {
        println!("no adapter-ready device connected");
        return;
    };
    let mut channel = Channel::open(&device.channel_path).expect("open");
    let settings = Settings::discover(&mut channel).expect("discover");
    println!("{settings:?}");
    println!("presets={:?}", settings.dpi_presets());
    let request = |index: u8, function: u8, params: &[u8]| {
        Request::new(DIRECT_DEVICE, index, function, params)
    };
    let index_of =
        |channel: &mut Channel, id: u16| lookup(channel, DIRECT_DEVICE, id).ok().flatten();
    if let (Some(index), Some(dpi)) = (
        index_of(&mut channel, feature::ADJUSTABLE_DPI),
        settings.dpi(),
    ) {
        let set = channel.request(&hidpp::set_dpi(DIRECT_DEVICE, index, dpi));
        let after = channel
            .request(&request(index, 2, &[0]))
            .map(|reply| hidpp::dpi_from_reply(&reply));
        println!("dpi write: ok={} after={after:?}", set.is_ok());
    }
    if let Some(index) = index_of(&mut channel, feature::SMARTSHIFT_ENHANCED) {
        // Function 1 reads mode, release speed and torque; write them back.
        let before = channel
            .request(&request(index, 1, &[]))
            .expect("wheel mode");
        let set = channel.request(&request(index, 2, &before[..3]));
        let after = channel
            .request(&request(index, 1, &[]))
            .map(|reply| reply[..3].to_vec());
        println!(
            "wheel mode write: ok={} before={:02X?} after={after:02X?}",
            set.is_ok(),
            &before[..3]
        );
    }
    if let Some(index) = index_of(&mut channel, feature::HIRES_WHEEL) {
        let mode = channel
            .request(&request(index, 1, &[]))
            .expect("wheel resolution");
        let high = mode[0] & 0b10 != 0;
        let set = channel.request(&hidpp::set_wheel_resolution(
            DIRECT_DEVICE,
            index,
            &mode,
            high,
        ));
        let after = channel
            .request(&request(index, 1, &[]))
            .map(|reply| reply[0]);
        println!(
            "wheel resolution write: ok={} before={:02X} after={after:02X?}",
            set.is_ok(),
            mode[0]
        );
    }
    if let Some(index) = index_of(&mut channel, feature::THUMB_WHEEL) {
        let before = channel
            .request(&request(index, 1, &[]))
            .expect("thumb wheel");
        let set = channel.request(&hidpp::set_thumb_wheel_diverted(
            DIRECT_DEVICE,
            index,
            before[0] != 0,
        ));
        let after = channel
            .request(&request(index, 1, &[]))
            .map(|reply| reply[..2].to_vec());
        println!(
            "thumb wheel write: ok={} before={:02X?} after={after:02X?}",
            set.is_ok(),
            &before[..2]
        );
    }
    println!(
        "host feature={:?} haptic feature={:?} (not exercised)",
        index_of(&mut channel, feature::CHANGE_HOST),
        index_of(&mut channel, feature::HAPTIC_FEEDBACK)
    );
}

/// Read-only: what the settings layer finds on the connected mouse. Discovery
/// only looks features up and reads their current values.
#[test]
#[ignore = "queries the Logitech devices connected to this machine"]
fn probe_prints_discovered_settings() {
    let found = device::devices(&discovery::enumerate());
    let Some(device) = found.into_iter().find(device::Device::adapter_ready) else {
        println!("no adapter-ready device connected");
        return;
    };
    let mut channel = Channel::open(&device.channel_path).expect("open");
    let settings = Settings::discover(&mut channel).expect("discover");
    println!("{settings:?}");
    println!(
        "dpi={:?} presets={:?} wheel_mode={} hires={} thumb={}",
        settings.dpi(),
        settings.dpi_presets(),
        settings.has_wheel_mode(),
        settings.has_hires_wheel(),
        settings.has_thumb_wheel()
    );
}
