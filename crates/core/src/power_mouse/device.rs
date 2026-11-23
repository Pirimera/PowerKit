//! Pure model of the mice a PowerMouse device adapter can drive: the HID
//! collections the platform discovers, grouped into physical devices, and the
//! adapter each one would use. Nothing here talks to a device.
use std::collections::BTreeMap;

pub const LOGITECH_VENDOR_ID: u16 = 0x046D;
const BOLT_RECEIVER_PRODUCT_ID: u16 = 0xC548;
const UNIFYING_RECEIVER_PRODUCT_IDS: [u16; 2] = [0xC52B, 0xC532];
/// HID-over-GATT service, present in the path of a Bluetooth LE HID device.
const BLE_HID_SERVICE: &str = "{00001812-0000-1000-8000-00805f9b34fb}";
/// Vendor usage page of HID++ collections on receivers and wired devices;
/// usage 1 is the short report and usage 2 the long one.
const HIDPP_USAGE_PAGE: u16 = 0xFF00;
/// Vendor usage page of the HID++ collection on Bluetooth LE devices. The low
/// byte of the usage is a mask of the reports it carries.
const HIDPP_BLE_USAGE_PAGE: u16 = 0xFF43;
const GENERIC_DESKTOP_PAGE: u16 = 0x01;
const MOUSE_USAGE: u16 = 0x02;

/// One HID top-level collection as the platform reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HidInterface {
    /// Device interface path; identifies the collection and its device.
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub product: String,
}

/// HID++ report sizes a device accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HidppChannels {
    pub short: bool,
    pub long: bool,
}

impl HidppChannels {
    pub fn any(self) -> bool {
        self.short || self.long
    }

    fn merge(self, other: Self) -> Self {
        Self {
            short: self.short || other.short,
            long: self.long || other.long,
        }
    }
}

impl HidInterface {
    /// The HID++ reports this collection carries; none for ordinary mouse,
    /// keyboard and consumer collections.
    pub fn hidpp(&self) -> HidppChannels {
        if self.vendor_id != LOGITECH_VENDOR_ID {
            return HidppChannels::default();
        }
        match (self.usage_page, self.usage) {
            (HIDPP_USAGE_PAGE, 1) => HidppChannels {
                short: true,
                long: false,
            },
            (HIDPP_USAGE_PAGE, 2) => HidppChannels {
                short: false,
                long: true,
            },
            (HIDPP_BLE_USAGE_PAGE, usage) => HidppChannels {
                short: usage & 0x01 != 0,
                long: usage & 0x02 != 0,
            },
            _ => HidppChannels::default(),
        }
    }

    fn is_mouse(&self) -> bool {
        self.usage_page == GENERIC_DESKTOP_PAGE && self.usage == MOUSE_USAGE
    }

    fn transport(&self) -> Transport {
        if self.path.to_ascii_lowercase().contains(BLE_HID_SERVICE) {
            Transport::Bluetooth
        } else if self.product_id == BOLT_RECEIVER_PRODUCT_ID {
            Transport::BoltReceiver
        } else if UNIFYING_RECEIVER_PRODUCT_IDS.contains(&self.product_id) {
            Transport::UnifyingReceiver
        } else {
            Transport::Usb
        }
    }

    /// Collections of one physical device share this key: the hardware id of
    /// the path without its per-collection and per-USB-interface parts. A
    /// Bluetooth id carries the device address, so two paired mice stay
    /// apart; two identical receivers share one key.
    fn device_key(&self) -> String {
        let lower = self.path.to_ascii_lowercase();
        let hardware_id = lower.split('#').nth(1).unwrap_or(&lower);
        hardware_id
            .split('&')
            .filter(|part| !part.starts_with("col") && !part.starts_with("mi_"))
            .collect::<Vec<_>>()
            .join("&")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    BoltReceiver,
    UnifyingReceiver,
    Bluetooth,
    Usb,
}

impl Transport {
    /// A receiver speaks for the devices paired to it; which mouse is behind
    /// it is known only after asking over HID++.
    pub fn is_receiver(self) -> bool {
        matches!(self, Self::BoltReceiver | Self::UnifyingReceiver)
    }
}

/// Models an adapter has been physically verified with, by transport and
/// product id. Adapter features stay off for anything not listed here. A
/// mouse behind a receiver is identified over HID++, not by the receiver's
/// id, so receivers never appear in this table.
const VERIFIED_MODELS: [(Transport, u16, &str); 1] =
    [(Transport::Bluetooth, 0xB042, "MX Master 4")];

fn verified_model(transport: Transport, product_id: u16) -> Option<&'static str> {
    VERIFIED_MODELS
        .iter()
        .find(|(at, id, _)| *at == transport && *id == product_id)
        .map(|(_, _, name)| *name)
}

/// The adapter that can add vendor features to a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    LogitechHidpp,
}

/// Battery level at or below which the user is told once, while discharging.
pub const LOW_BATTERY_PERCENT: u8 = 15;

/// Something an action asks a device to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceAction {
    /// Move the device to the paired computer in this slot, counted from 0.
    SwitchHost(u8),
    HapticPulse,
    DpiCycle,
}

/// Settings an adapter keeps on its device. `None` leaves the device as it
/// is; a set value is applied on connect and after every reconnect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceSettings {
    pub dpi: Option<u16>,
    pub wheel_mode: Option<super::hidpp::WheelMode>,
    /// High-resolution wheel reports, for finer scrolling.
    pub hires_wheel: Option<bool>,
    /// Report the thumb wheel to PowerMouse instead of scrolling sideways.
    pub thumb_wheel: bool,
}

/// Where an adapter stands with its device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterLink {
    /// Opening the device and asking what it is.
    Connecting,
    /// Talking to the device; bound device controls are diverted.
    Active,
    /// The device did not answer (asleep, out of range); it is retried.
    Unreachable,
    /// Another application that drives the same controls is running; the
    /// adapter leaves the device alone.
    Conflict(&'static str),
}

/// What an adapter knows about its device, for the page and the tray notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterStatus {
    pub link: AdapterLink,
    /// The name the device reports; empty until it has answered.
    pub name: String,
    pub battery: Option<super::hidpp::Battery>,
    /// Device triggers this device has and can divert.
    pub controls: Vec<super::Trigger>,
    /// The sensor's current DPI and the presets it supports; empty when the
    /// device has no adjustable DPI.
    pub dpi: Option<u16>,
    pub dpi_presets: Vec<u16>,
    /// The device can switch wheel mode, wheel resolution, and report its
    /// thumb wheel.
    pub has_wheel_mode: bool,
    pub has_hires_wheel: bool,
    pub has_thumb_wheel: bool,
}

impl AdapterStatus {
    pub fn new(link: AdapterLink) -> Self {
        Self {
            link,
            name: String::new(),
            battery: None,
            controls: Vec::new(),
            dpi: None,
            dpi_presets: Vec::new(),
            has_wheel_mode: false,
            has_hires_wheel: false,
            has_thumb_wheel: false,
        }
    }

    /// Whether the battery is low enough to tell the user.
    pub fn battery_low(&self) -> bool {
        self.battery
            .is_some_and(|battery| !battery.charging && battery.percent <= LOW_BATTERY_PERCENT)
    }
}

impl Device {
    /// Whether an adapter may drive this device: a verified model, connected
    /// directly, with a long-report HID++ collection. A mouse behind a
    /// receiver is not supported yet.
    pub fn adapter_ready(&self) -> bool {
        self.verified && !self.transport.is_receiver() && !self.channel_path.is_empty()
    }
}

/// A connected device an adapter exists for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// Stable while the device stays connected; not shown to the user.
    pub key: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub transport: Transport,
    pub adapter: Adapter,
    pub hidpp: HidppChannels,
    /// Path of the collection that carries HID++ long reports; empty when
    /// the device only has a short-report collection.
    pub channel_path: String,
    /// The model is one the adapter was physically verified with.
    pub verified: bool,
}

/// Groups collections into devices and keeps the ones an adapter can drive:
/// Logitech receivers, and directly connected Logitech mice that expose an
/// HID++ collection. Other vendors' devices and Logitech keyboards are left
/// out; the generic layer needs none of this.
pub fn devices(interfaces: &[HidInterface]) -> Vec<Device> {
    let mut groups: BTreeMap<String, Vec<&HidInterface>> = BTreeMap::new();
    for interface in interfaces {
        if interface.vendor_id == LOGITECH_VENDOR_ID {
            groups
                .entry(interface.device_key())
                .or_default()
                .push(interface);
        }
    }
    groups
        .into_iter()
        .filter_map(|(key, members)| {
            let hidpp = members
                .iter()
                .fold(HidppChannels::default(), |all, member| {
                    all.merge(member.hidpp())
                });
            let carrier = members.iter().find(|member| member.hidpp().any())?;
            let transport = carrier.transport();
            if !transport.is_receiver() && !members.iter().any(|member| member.is_mouse()) {
                return None;
            }
            // Bluetooth LE collections report no product string, so a verified
            // model's name comes first and the product id is the last resort.
            let model = verified_model(transport, carrier.product_id);
            let name = model.map(str::to_string).unwrap_or_else(|| {
                members
                    .iter()
                    .map(|member| member.product.trim())
                    .find(|product| !product.is_empty())
                    .map_or_else(
                        || format!("Logitech {:04X}", carrier.product_id),
                        str::to_string,
                    )
            });
            Some(Device {
                key,
                name,
                vendor_id: carrier.vendor_id,
                product_id: carrier.product_id,
                transport,
                adapter: Adapter::LogitechHidpp,
                hidpp,
                channel_path: members
                    .iter()
                    .find(|member| member.hidpp().long)
                    .map(|member| member.path.clone())
                    .unwrap_or_default(),
                verified: model.is_some(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(path: &str, product_id: u16, usage_page: u16, usage: u16) -> HidInterface {
        HidInterface {
            path: path.into(),
            vendor_id: LOGITECH_VENDOR_ID,
            product_id,
            usage_page,
            usage,
            product: String::new(),
        }
    }

    const RECEIVER: &str = r"\\?\HID#VID_046D&PID_C548&MI_0";
    const BLE: &str =
        r"\\?\HID#{00001812-0000-1000-8000-00805F9B34FB}_Dev_VID&02046d_PID&b099_REV&0001_";
    const GUID: &str = "#{4d1e55b2-f16f-11cf-88cb-001111000030}";

    #[test]
    fn receiver_collections_form_one_device_with_both_report_sizes() {
        let found = devices(&[
            interface(&format!("{RECEIVER}0#7&aaaa&0&0000{GUID}"), 0xC548, 1, 6),
            interface(
                &format!("{RECEIVER}1&Col01#7&bbbb&0&0000{GUID}"),
                0xC548,
                1,
                2,
            ),
            HidInterface {
                product: " USB Receiver ".into(),
                ..interface(
                    &format!("{RECEIVER}2&Col01#7&cccc&0&0000{GUID}"),
                    0xC548,
                    0xFF00,
                    1,
                )
            },
            interface(
                &format!("{RECEIVER}2&Col02#7&cccc&0&0001{GUID}"),
                0xC548,
                0xFF00,
                2,
            ),
        ]);
        assert_eq!(found.len(), 1);
        let receiver = &found[0];
        assert_eq!(receiver.key, "vid_046d&pid_c548");
        assert_eq!(receiver.name, "USB Receiver");
        assert_eq!(receiver.transport, Transport::BoltReceiver);
        assert!(receiver.transport.is_receiver());
        assert_eq!(
            receiver.hidpp,
            HidppChannels {
                short: true,
                long: true
            }
        );
    }

    #[test]
    fn bluetooth_mice_stay_apart_and_keyboards_are_left_out() {
        let mouse = |address: &str, collection: &str, usage_page, usage| {
            interface(
                &format!("{BLE}{address}&{collection}#9&dddd&0&0000{GUID}"),
                0xB099,
                usage_page,
                usage,
            )
        };
        let found = devices(&[
            mouse("aa0000000001", "Col01", 1, 2),
            mouse("aa0000000001", "Col03", 0xFF43, 0x0202),
            mouse("aa0000000002", "Col01", 1, 2),
            mouse("aa0000000002", "Col03", 0xFF43, 0x0202),
            // A Logitech keyboard: HID++ collection but no mouse collection.
            mouse("aa0000000003", "Col01", 1, 6),
            mouse("aa0000000003", "Col02", 0xFF43, 0x0202),
        ]);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|device| {
            device.transport == Transport::Bluetooth
                && device.hidpp
                    == HidppChannels {
                        short: false,
                        long: true,
                    }
                && device.name == "Logitech B099"
        }));
        assert_ne!(found[0].key, found[1].key);
        assert!(found.iter().all(|device| !device.verified));
    }

    #[test]
    fn a_verified_model_is_named_and_marked() {
        let mouse = |collection: &str, usage_page, usage| {
            interface(
                &format!(
                    "{}{collection}#9&dddd&0&0000{GUID}",
                    BLE.replace("b099", "b042")
                ),
                0xB042,
                usage_page,
                usage,
            )
        };
        let found = devices(&[
            mouse("aa0000000001&Col01", 1, 2),
            mouse("aa0000000001&Col02", 0xFF43, 0x0202),
            mouse("aa0000000001&Col03", 0x0E, 1),
        ]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "MX Master 4");
        assert!(found[0].verified);
        // The receiver is never the verified model, whatever is paired to it.
        assert!(verified_model(Transport::BoltReceiver, 0xB042).is_none());
    }

    #[test]
    fn other_vendors_and_plain_logitech_mice_have_no_adapter() {
        let plain = interface(
            &format!(r"\\?\HID#VID_046D&PID_C077#6&eeee&0&0000{GUID}"),
            0xC077,
            1,
            2,
        );
        let other_vendor = HidInterface {
            vendor_id: 0x1532,
            ..interface(
                &format!(r"\\?\HID#VID_1532&PID_0084&Col05#6&ffff&0&0004{GUID}"),
                0x0084,
                0xFF00,
                1,
            )
        };
        assert_eq!(other_vendor.hidpp(), HidppChannels::default());
        assert!(devices(&[plain, other_vendor]).is_empty());
    }
}
